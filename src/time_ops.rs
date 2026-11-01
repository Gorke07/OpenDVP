//! Register-only calendar comparisons and binary time arithmetic.
//! Installed TADD/TSUB success has a separately tracked native-PC defect.
use crate::calendar_operand::CalendarValue;
use crate::plc::{Device, Memory, comparison_destinations, word_range};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarTimeOutcome {
    Nominal,
    ArithmeticSuccess,
    PlcError { offset: u16 },
    Limitation(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimeAction {
    Compare {
        setpoint: [CalendarValue; 3],
        source: Device,
        dest: Device,
    },
    Zone {
        low: Device,
        high: Device,
        source: Device,
        dest: Device,
    },
    Arithmetic {
        subtract: bool,
        left: Device,
        right: Device,
        dest: Device,
    },
}

fn triple(raw: &str) -> Result<Device, String> {
    let d = raw.parse()?;
    word_range(d, 3)?;
    Ok(d)
}
fn at(d: Device, offset: u16) -> Device {
    Device {
        area: d.area,
        index: d.index + offset,
    }
}
fn field(m: &Memory, d: Device, offset: u16) -> i16 {
    m.word(at(d, offset))
}
pub(crate) fn field_valid(value: i16, offset: u16) -> bool {
    (0..=if offset == 0 { 23 } else { 59 }).contains(&value)
}
/// A single validated field; callers preserve native progressive read/store
/// order rather than snapshotting an entire aliased time block.
pub(crate) fn arithmetic_field(
    a: i16,
    b: i16,
    incoming: i16,
    offset: u16,
    subtract: bool,
) -> (i16, i16) {
    let modulus = if offset == 0 { 24 } else { 60 };
    let raw = if subtract {
        a - b - incoming
    } else {
        a + b + incoming
    };
    let carry = i16::from(if subtract { raw < 0 } else { raw >= modulus });
    (raw.rem_euclid(modulus), carry)
}
fn seconds(m: &Memory, d: Device) -> Option<i32> {
    let values = [field(m, d, 0), field(m, d, 1), field(m, d, 2)];
    values
        .iter()
        .enumerate()
        .all(|(i, v)| field_valid(*v, i as u16))
        .then(|| i32::from(values[0]) * 3600 + i32::from(values[1]) * 60 + i32::from(values[2]))
}
impl TimeAction {
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut words = vec![self.header()];
        let numeric = |d| {
            crate::es2_encoder::calendar_operand(&CalendarValue::Additional(
                crate::additional_processing::AdditionalValue::Word(crate::plc::Operand::Register(
                    d,
                )),
            ))
        };
        match self {
            Self::Compare {
                setpoint,
                source,
                dest,
            } => {
                for value in setpoint {
                    words.extend(crate::es2_encoder::calendar_operand(value)?);
                }
                words.extend(numeric(*source)?);
                words.extend(crate::es2_encoder::comparison_result_operand(*dest)?);
            }
            Self::Zone {
                low,
                high,
                source,
                dest,
            } => {
                for device in [low, high, source] {
                    words.extend(numeric(*device)?);
                }
                words.extend(crate::es2_encoder::comparison_result_operand(*dest)?);
            }
            Self::Arithmetic {
                left, right, dest, ..
            } => {
                for device in [left, right, dest] {
                    words.extend(numeric(*device)?);
                }
            }
        }
        Ok(words)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let expected = match name {
            "TCMP" => 5,
            "TZCP" => 4,
            "TADD" | "TSUB" => 3,
            _ => return Err("Bilinmeyen saat işlemi".into()),
        };
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        Ok(match name {
            "TCMP" => {
                let dest = args[4].parse()?;
                comparison_destinations(dest)?;
                Self::Compare {
                    setpoint: [
                        CalendarValue::parse(&args[0], false)?,
                        CalendarValue::parse(&args[1], false)?,
                        CalendarValue::parse(&args[2], false)?,
                    ],
                    source: triple(&args[3])?,
                    dest,
                }
            }
            "TZCP" => {
                let dest = args[3].parse()?;
                comparison_destinations(dest)?;
                Self::Zone {
                    low: triple(&args[0])?,
                    high: triple(&args[1])?,
                    source: triple(&args[2])?,
                    dest,
                }
            }
            _ => Self::Arithmetic {
                subtract: name == "TSUB",
                left: triple(&args[0])?,
                right: triple(&args[1])?,
                dest: triple(&args[2])?,
            },
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Compare { .. } => "TCMP",
            Self::Zone { .. } => "TZCP",
            Self::Arithmetic {
                subtract: false, ..
            } => "TADD",
            _ => "TSUB",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Compare { .. } => 0x6001,
            Self::Zone { .. } => 0x6201,
            Self::Arithmetic {
                subtract: false, ..
            } => 0x6401,
            _ => 0x6601,
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Compare { dest, .. }
            | Self::Zone { dest, .. }
            | Self::Arithmetic { dest, .. } => *dest,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Compare {
                setpoint,
                source,
                dest,
            } => format!(
                "TCMP {} {} {} {source} {dest}",
                setpoint[0], setpoint[1], setpoint[2]
            ),
            Self::Zone {
                low,
                high,
                source,
                dest,
            } => format!("TZCP {low} {high} {source} {dest}"),
            Self::Arithmetic {
                left, right, dest, ..
            } => format!("{} {left} {right} {dest}", self.name()),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        Memory::error_devices(out);
        let mut triples = Vec::new();
        match self {
            Self::Compare {
                setpoint,
                source,
                dest,
            } => {
                for v in setpoint {
                    v.devices(out);
                }
                triples.push(*source);
                out.extend(comparison_destinations(*dest).unwrap());
            }
            Self::Zone {
                low,
                high,
                source,
                dest,
            } => {
                triples.extend([*low, *high, *source]);
                out.extend(comparison_destinations(*dest).unwrap());
            }
            Self::Arithmetic {
                left, right, dest, ..
            } => {
                triples.extend([*left, *right, *dest]);
                out.extend((1020..=1022).map(Memory::system_device));
            }
        }
        for d in triples {
            out.extend(word_range(d, 3).unwrap());
        }
    }
    pub fn observed_devices(&self, _m: &Memory, _out: &mut BTreeSet<Device>) {}
    pub fn execute(&self, m: &mut Memory) -> CalendarTimeOutcome {
        // Implicit triples are guarded logical spans, not raw pointer extension.
        let mut devices = BTreeSet::new();
        self.devices(&mut devices);
        if devices.iter().any(|d| {
            d.area == crate::plc::Area::D && d.index > m.data_register_limit.unwrap_or(9999)
        }) {
            let reason = "Saat işlemi modelin D sınırını aşıyor".to_string();
            m.execution_limitations.insert(reason.clone());
            return CalendarTimeOutcome::Limitation(reason);
        }
        let result = match self {
            Self::Compare {
                setpoint, source, ..
            } => {
                let mut v = [0i32; 3];
                for (i, s) in setpoint.iter().enumerate() {
                    v[i] = s.read(m);
                    if !(0..=if i == 0 { 23 } else { 59 }).contains(&v[i]) {
                        m.operation_error(0xe1a, 1 + i as u16 * 2);
                        return CalendarTimeOutcome::PlcError {
                            offset: 1 + i as u16 * 2,
                        };
                    }
                }
                let Some(current) = seconds(m, *source) else {
                    m.operation_error(0xe1a, 7);
                    return CalendarTimeOutcome::PlcError { offset: 7 };
                };
                let set = v[0] * 3600 + v[1] * 60 + v[2];
                [set > current, set == current, set < current]
            }
            Self::Zone {
                low, high, source, ..
            } => {
                let mut times = [0; 3];
                for (i, d) in [low, high, source].iter().enumerate() {
                    let Some(t) = seconds(m, **d) else {
                        m.operation_error(0xe1a, 1 + i as u16 * 2);
                        return CalendarTimeOutcome::PlcError {
                            offset: 1 + i as u16 * 2,
                        };
                    };
                    times[i] = t;
                }
                [
                    times[2] < times[0].min(times[1]),
                    times[2] >= times[0].min(times[1]) && times[2] <= times[0].max(times[1]),
                    times[2] > times[0].max(times[1]),
                ]
            }
            Self::Arithmetic {
                subtract,
                left,
                right,
                dest,
            } => {
                // Native writes seconds, then minutes, then hours. Aliases and
                // late validation errors therefore retain progressive writes.
                let mut carry = 0i16;
                let mut output = [0i16; 3];
                for offset in [2, 1, 0] {
                    let (a, b) = (field(m, *left, offset), field(m, *right, offset));
                    if !field_valid(a, offset) || !field_valid(b, offset) {
                        m.operation_error(0xe1a, 3);
                        return CalendarTimeOutcome::PlcError { offset: 3 };
                    }
                    let (value, next) = arithmetic_field(a, b, carry, offset, *subtract);
                    carry = next;
                    output[offset as usize] = value;
                    if offset == 0 {
                        m.set_system(if *subtract { 1021 } else { 1022 }, carry != 0);
                    }
                    m.set_word(at(*dest, offset), value);
                }
                m.set_system(1020, output == [0; 3]);
                return CalendarTimeOutcome::ArithmeticSuccess;
            }
        };
        for (d, on) in comparison_destinations(self.dest())
            .unwrap()
            .into_iter()
            .zip(result)
        {
            m.set_internal(d, on);
        }
        CalendarTimeOutcome::Nominal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(s: &str) -> Device {
        s.parse().unwrap()
    }
    fn put(m: &mut Memory, base: &str, values: [i16; 3]) {
        for (i, v) in values.into_iter().enumerate() {
            m.set_word(at(d(base), i as u16), v);
        }
    }
    fn action(op: &str, args: &[&str]) -> TimeAction {
        TimeAction::parse(op, &args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }
    #[test]
    fn arithmetic_wrap_flags_and_error_partial_writes() {
        let mut m = Memory::default();
        put(&mut m, "D0", [23, 59, 59]);
        put(&mut m, "D10", [0, 0, 1]);
        m.set_system(1021, true);
        action("TADD", &["D0", "D10", "D20"]).execute(&mut m);
        assert!((0..3).all(|i| field(&m, d("D20"), i) == 0));
        assert!(m.system_bit(1020) && m.system_bit(1021) && m.system_bit(1022));
        action("TSUB", &["D10", "D0", "D20"]).execute(&mut m);
        assert_eq!(
            (0..3).map(|i| field(&m, d("D20"), i)).collect::<Vec<_>>(),
            [0, 0, 2]
        );
        assert!(m.system_bit(1021) && m.system_bit(1022));
        assert!(!m.system_bit(1020));
        put(&mut m, "D0", [24, 59, 59]);
        put(&mut m, "D20", [7, 8, 9]);
        action("TADD", &["D0", "D10", "D20"]).execute(&mut m);
        assert_eq!(
            (0..3).map(|i| field(&m, d("D20"), i)).collect::<Vec<_>>(),
            [7, 0, 0]
        );
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), 3);
    }
    #[test]
    fn arithmetic_aliases_are_progressive_not_snapshots() {
        let mut m = Memory::default();
        put(&mut m, "D0", [1, 2, 3]);
        put(&mut m, "D10", [4, 5, 6]);
        action("TADD", &["D0", "D10", "D1"]).execute(&mut m);
        // Seconds destinationD3 is written first; minute destinationD2 then
        // overwrites source seconds, while source hour remains unchanged.
        assert_eq!(
            (1..=3).map(|i| m.word(at(d("D0"), i))).collect::<Vec<_>>(),
            [5, 7, 9]
        );
        let mut m = Memory::default();
        put(&mut m, "D2", [1, 2, 3]);
        put(&mut m, "D10", [4, 5, 6]);
        action("TADD", &["D2", "D10", "D1"]).execute(&mut m);
        assert_eq!(
            (1..=3).map(|i| m.word(at(d("D0"), i))).collect::<Vec<_>>(),
            [18, 14, 9]
        );
    }
    #[test]
    fn reversed_zones_are_inclusive_and_invalid_fields_hold_bits() {
        let mut m = Memory::default();
        put(&mut m, "D0", [18, 0, 0]);
        put(&mut m, "D10", [6, 0, 0]);
        put(&mut m, "D20", [6, 0, 0]);
        let a = action("TZCP", &["D0", "D10", "D20", "M0"]);
        a.execute(&mut m);
        assert!(m.bit(d("M1")));
        put(&mut m, "D20", [5, 59, 59]);
        a.execute(&mut m);
        assert!(m.bit(d("M0")));
        put(&mut m, "D20", [5, 60, 0]);
        a.execute(&mut m);
        assert!(m.bit(d("M0")));
        assert_eq!(m.word(d("D1068")), 5);
        put(&mut m, "D20", [12, 20, 45]);
        action("TCMP", &["K12", "H14", "K45", "D20", "M10"]).execute(&mut m);
        assert!(m.bit(d("M11")));
    }
}
