//! Numeric contacts that test bitwise results or absolute differences.
use crate::additional_processing::AdditionalValue;
use crate::bit_index_operand::BitIndexValue;
use crate::calendar_operand::CalendarValue;
use crate::plc::{Area, Device, Memory, Operand, Relation, WordLogic};
use crate::wide::WideOperand;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub enum ContactKind {
    Logic(WordLogic),
    Difference(Relation),
    Indexed(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContactSpec {
    pub prefix: &'static str,
    pub wide: bool,
    pub kind: ContactKind,
    pub arity: usize,
}

/// Recognize 18 logic, 36 difference and 12 indexed contact spellings.
pub fn parse_spec(name: &str) -> Option<ContactSpec> {
    let (wide, name) = if let Some(name) = name.strip_prefix('D') {
        (true, name)
    } else {
        (false, name)
    };
    let indexed = match name {
        "BLD" => Some(("LD", false)),
        "BLDI" => Some(("LD", true)),
        "BAND" => Some(("AND", false)),
        "BANI" => Some(("AND", true)),
        "BOR" => Some(("OR", false)),
        "BORI" => Some(("OR", true)),
        _ => None,
    };
    if let Some((prefix, inverted)) = indexed {
        return Some(ContactSpec {
            prefix,
            wide,
            kind: ContactKind::Indexed(inverted),
            arity: 2,
        });
    }
    for prefix in ["LD", "AND", "OR"] {
        let Some(suffix) = name.strip_prefix(prefix) else {
            continue;
        };
        let kind = match suffix {
            "&" => ContactKind::Logic(WordLogic::And),
            "|" => ContactKind::Logic(WordLogic::Or),
            "^" => ContactKind::Logic(WordLogic::Xor),
            _ => ContactKind::Difference(Relation::parse(suffix.strip_prefix('Z')?)?),
        };
        let arity = if matches!(kind, ContactKind::Difference(_)) {
            3
        } else {
            2
        };
        return Some(ContactSpec {
            prefix,
            wide,
            kind,
            arity,
        });
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub enum NumericPredicate {
    Logic(WordLogic, [CalendarValue; 2]),
    LogicWide(WordLogic, [CalendarValue; 2]),
    Difference(Relation, [AdditionalValue; 3]),
    DifferenceWide(Relation, [AdditionalValue; 3]),
    Indexed {
        source: CalendarValue,
        index: BitIndexValue,
        inverted: bool,
    },
}

impl NumericPredicate {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let spec = parse_spec(name).ok_or("Bilinmeyen sayısal kontak")?;
        if args.len() != spec.arity {
            return Err(format!("{name}: {} operand gerekli", spec.arity));
        }
        Ok(match (spec.kind, spec.wide) {
            (ContactKind::Indexed(inverted), wide) => {
                let source = CalendarValue::parse(&args[0], wide)?;
                // Unlike logic contacts, indexed S excludes KnX, E/F and
                // literals. Reading a KnM source does not require writable M.
                if !matches!(
                    &source,
                    CalendarValue::Additional(
                        AdditionalValue::Word(Operand::Register(_))
                            | AdditionalValue::Double(WideOperand::Register(_))
                    ) | CalendarValue::Additional(AdditionalValue::Group {
                        base: Device {
                            area: Area::Y | Area::M | Area::S,
                            ..
                        },
                        ..
                    })
                ) {
                    return Err("İndeksli kontak kaynağı KnY/KnM/KnS veya T/C/D olmalı".into());
                }
                let index = BitIndexValue::parse(&args[1], wide)?;
                let max = if wide { 31 } else { 15 };
                if index.constant().is_some_and(|n| !(0..=max).contains(&n)) {
                    return Err(format!("Bit numarası 0..{max} aralığında olmalı"));
                }
                Self::Indexed {
                    source,
                    index,
                    inverted,
                }
            }
            // Logic contacts permit packed bits and direct E/F values. The
            // difference-contact matrix omits E/F, so keep its parser distinct.
            (ContactKind::Logic(op), false) => Self::Logic(
                op,
                [
                    CalendarValue::parse(&args[0], false)?,
                    CalendarValue::parse(&args[1], false)?,
                ],
            ),
            (ContactKind::Logic(op), true) => Self::LogicWide(
                op,
                [
                    CalendarValue::parse(&args[0], true)?,
                    CalendarValue::parse(&args[1], true)?,
                ],
            ),
            (ContactKind::Difference(op), false) => Self::Difference(
                op,
                [
                    AdditionalValue::parse(&args[0], false, false)?,
                    AdditionalValue::parse(&args[1], false, false)?,
                    AdditionalValue::parse(&args[2], false, false)?,
                ],
            ),
            (ContactKind::Difference(op), true) => Self::DifferenceWide(
                op,
                [
                    AdditionalValue::parse(&args[0], true, false)?,
                    AdditionalValue::parse(&args[1], true, false)?,
                    AdditionalValue::parse(&args[2], true, false)?,
                ],
            ),
        })
    }

    pub fn width(&self) -> u8 {
        match self {
            Self::Logic(..) | Self::Difference(..) => 16,
            Self::LogicWide(..) | Self::DifferenceWide(..) => 32,
            Self::Indexed { source, .. } => {
                if source.wide() {
                    32
                } else {
                    16
                }
            }
        }
    }

    pub fn evaluate(&self, m: &Memory) -> bool {
        match self {
            Self::Indexed {
                source,
                index,
                inverted,
            } => {
                let source_value = source.read(m) as u32;
                let index = index.read(m) as u32 & if source.wide() { 31 } else { 15 };
                ((source_value & (1u32 << index)) != 0) ^ inverted
            }
            Self::Logic(op, [a, b]) | Self::LogicWide(op, [a, b]) => {
                logic(op, a.read(m), b.read(m))
            }
            Self::Difference(op, [a, b, c]) => {
                // Packed groups shorter than four digits are zero-extended;
                // the subtraction still wraps at the instruction's 16 bits.
                let difference = (a.read(m) as i16).wrapping_sub(b.read(m) as i16);
                op.test(absolute_word(difference), absolute_word(c.read(m) as i16))
            }
            Self::DifferenceWide(op, [a, b, c]) => {
                let difference = a.read(m).wrapping_sub(b.read(m));
                op.test(absolute_double(difference), absolute_double(c.read(m)))
            }
        }
    }

    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Indexed { source, index, .. } => {
                source.devices(out);
                index.devices(out);
            }
            Self::Logic(_, sources) | Self::LogicWide(_, sources) => {
                for source in sources {
                    source.devices(out);
                }
            }
            Self::Difference(_, sources) | Self::DifferenceWide(_, sources) => {
                for source in sources {
                    source.devices(out);
                }
            }
        }
    }

    pub fn label(&self, prefix: &str) -> String {
        let wide = if self.width() == 32 { "D" } else { "" };
        match self {
            Self::Indexed {
                source,
                index,
                inverted,
            } => {
                let name = match (prefix, *inverted) {
                    ("LD", false) => "BLD",
                    ("LD", true) => "BLDI",
                    ("AND", false) => "BAND",
                    ("AND", true) => "BANI",
                    ("OR", false) => "BOR",
                    ("OR", true) => "BORI",
                    _ => unreachable!("Numeric contact labels require LD/AND/OR prefix"),
                };
                format!("{wide}{name} {source} {index}")
            }
            Self::Logic(op, [a, b]) => format!("{wide}{prefix}{} {a} {b}", logic_symbol(op)),
            Self::LogicWide(op, [a, b]) => format!("{wide}{prefix}{} {a} {b}", logic_symbol(op)),
            Self::Difference(op, [a, b, c]) => {
                format!("{wide}{prefix}Z{} {a} {b} {c}", op.symbol())
            }
            Self::DifferenceWide(op, [a, b, c]) => {
                format!("{wide}{prefix}Z{} {a} {b} {c}", op.symbol())
            }
        }
    }
}

// Native comparison helpers wrap subtraction to operand width,
// replace the unrepresentable signed minimum with zero, then take abs.
// This is deliberately distinct from mathematical distance in a wider type.
fn absolute_word(value: i16) -> i16 {
    if value == i16::MIN { 0 } else { value.abs() }
}

fn absolute_double(value: i32) -> i32 {
    if value == i32::MIN { 0 } else { value.abs() }
}

fn logic(op: &WordLogic, a: i32, b: i32) -> bool {
    match op {
        WordLogic::And => a & b != 0,
        WordLogic::Or => a | b != 0,
        WordLogic::Xor => a ^ b != 0,
    }
}

fn logic_symbol(op: &WordLogic) -> &'static str {
    match op {
        WordLogic::And => "&",
        WordLogic::Or => "|",
        WordLogic::Xor => "^",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plc::{Simulator, compile};
    fn predicate(name: &str, args: &[&str]) -> NumericPredicate {
        NumericPredicate::parse(
            name,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn d(name: &str) -> Device {
        name.parse().unwrap()
    }

    fn set_packed_bits(m: &mut Memory, base: &str, width: u16, value: u32) {
        let base = d(base);
        for offset in 0..width {
            let bit = Device {
                area: base.area,
                index: base.index + offset,
            };
            let on = value & (1u32 << offset) != 0;
            if bit.area == crate::plc::Area::X {
                m.set_input(bit, on);
            } else {
                m.set_internal(bit, on);
            }
        }
    }

    #[test]
    fn all_54_numeric_contacts_read_packed_bits_and_roundtrip_labels() {
        for bases in [
            ["X0", "X40", "X100"],
            ["Y0", "Y40", "Y100"],
            ["M0", "M32", "M64"],
            ["S0", "S32", "S64"],
        ] {
            for (a, b, c) in [(0u32, 0u32, 0u32), (1, 2, 3), (9, 3, 6), (15, 15, 0)] {
                let mut m = Memory::default();
                for (base, value) in bases.into_iter().zip([a, b, c]) {
                    set_packed_bits(&mut m, base, 4, value);
                }
                let distance = (a as i32 - b as i32).unsigned_abs();
                for wide in ["", "D"] {
                    for prefix in ["LD", "AND", "OR"] {
                        for (suffix, expected) in [
                            ("&", a & b != 0),
                            ("|", a | b != 0),
                            ("^", a ^ b != 0),
                            ("Z>", distance > c),
                            ("Z>=", distance >= c),
                            ("Z<", distance < c),
                            ("Z<=", distance <= c),
                            ("Z=", distance == c),
                            ("Z<>", distance != c),
                        ] {
                            let name = format!("{wide}{prefix}{suffix}");
                            let args: Vec<_> = bases
                                .iter()
                                .take(if suffix.starts_with('Z') { 3 } else { 2 })
                                .map(|base| format!("K1{base}"))
                                .collect();
                            let p = NumericPredicate::parse(&name, &args).unwrap();
                            assert_eq!(p.evaluate(&m), expected, "{name} {args:?}");
                            let label = p.label(prefix);
                            let parts: Vec<_> = label.split_whitespace().collect();
                            assert_eq!(predicate(parts[0], &parts[1..]), p);
                            let source = if prefix == "LD" {
                                format!("{label}\nOUT Y200\nEND")
                            } else {
                                format!("LD X200\n{label}\nOUT Y200\nEND")
                            };
                            let program = compile(&source).unwrap();
                            let mut simulator = Simulator::default();
                            simulator.memory = m.clone();
                            for gate in [false, true] {
                                simulator.memory.set_input(d("X200"), gate);
                                simulator.scan(&program, 100);
                                let output = match prefix {
                                    "AND" => gate && expected,
                                    "OR" => gate || expected,
                                    _ => expected,
                                };
                                assert_eq!(simulator.memory.bit(d("Y200")), output, "{source}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn packed_contact_width_preserves_signed_minimum_and_wrapping_difference() {
        let mut m = Memory::default();
        set_packed_bits(&mut m, "M0", 32, 0x8000);
        assert!(predicate("LD&", &["K4M0", "H8000"]).evaluate(&m));
        assert!(predicate("LDZ=", &["K4M0", "K0", "K0"]).evaluate(&m));
        assert!(predicate("LDZ=", &["K0", "K0", "K4M0"]).evaluate(&m));
        // A short group is zero-extended, even when its highest bit is set.
        set_packed_bits(&mut m, "M0", 32, 0x800);
        assert!(predicate("LDZ=", &["K3M0", "K0", "K2048"]).evaluate(&m));
        set_packed_bits(&mut m, "M0", 32, 0x8000_0000);
        assert!(predicate("DLD&", &["K8M0", "H80000000"]).evaluate(&m));
        assert!(predicate("DLDZ=", &["K8M0", "K0", "K0"]).evaluate(&m));
        assert!(predicate("DLDZ=", &["K0", "K0", "K8M0"]).evaluate(&m));
        set_packed_bits(&mut m, "M0", 32, 0x0800_0000);
        assert!(predicate("DLDZ=", &["K7M0", "K0", "K134217728"]).evaluate(&m));
        for (name, digits, positive, negative) in [
            ("LDZ=", 4, 0x7fff, 0x8000),
            ("DLDZ=", 8, 0x7fff_ffff, 0x8000_0000),
        ] {
            set_packed_bits(&mut m, "M0", 32, positive);
            set_packed_bits(&mut m, "M32", 32, negative);
            let args = [format!("K{digits}M0"), format!("K{digits}M32"), "K1".into()];
            assert!(NumericPredicate::parse(name, &args).unwrap().evaluate(&m));
        }
    }

    #[test]
    fn packed_contact_spans_track_every_bit_and_enforce_area_limits() {
        for (name, valid, invalid) in [
            ("LD&", "K4X360", "K4X361"),
            ("LD&", "K4Y360", "K4Y361"),
            ("LD&", "K4M4080", "K4M4081"),
            ("LD&", "K4S1008", "K4S1009"),
            ("DLD&", "K8X340", "K8X341"),
            ("DLD&", "K8Y340", "K8Y341"),
            ("DLD&", "K8M4064", "K8M4065"),
            ("DLD&", "K8S992", "K8S993"),
        ] {
            assert!(NumericPredicate::parse(name, &[valid.into(), "K0".into()]).is_ok());
            assert!(NumericPredicate::parse(name, &[invalid.into(), "K0".into()]).is_err());
        }
        for (name, group) in [
            ("LD&", "K0M0"),
            ("LD&", "K5M0"),
            ("DLD&", "K9M0"),
            ("LD&", "K4M999"),
            ("LD&", "X0"),
            ("DLD&", "M0"),
        ] {
            assert!(NumericPredicate::parse(name, &[group.into(), "K0".into()]).is_err());
        }
        let p = predicate("DLDZ=", &["K8M4064", "K4X7", "K1S1020"]);
        let mut tracked = BTreeSet::new();
        p.devices(&mut tracked);
        let expected: BTreeSet<_> = (4064..=4095)
            .map(|index| d(&format!("M{index}")))
            .chain((7..=22).map(|index| d(&format!("X{index:o}"))))
            .chain((1020..=1023).map(|index| d(&format!("S{index}"))))
            .collect();
        assert_eq!(tracked, expected);
    }

    #[test]
    fn logic_and_indexed_operands_keep_width_and_role_specific_index_rules() {
        let mut m = Memory::default();
        m.set_word(d("D1028"), i16::MIN);
        m.set_word(d("D1029"), 1);
        m.set_double_word(d("D1194"), i32::MIN);
        for prefix in ["LD", "AND", "OR"] {
            assert!(predicate(&format!("{prefix}&"), &["E0", "H8000"]).evaluate(&m));
            assert!(predicate(&format!("{prefix}&"), &["F0", "K1"]).evaluate(&m));
            assert!(predicate(&format!("D{prefix}&"), &["E7", "H80000000"]).evaluate(&m));
            for suffix in ["&", "|", "^"] {
                assert!(
                    NumericPredicate::parse(
                        &format!("{prefix}{suffix}"),
                        &["E7".into(), "F7".into()]
                    )
                    .is_ok()
                );
                assert!(
                    NumericPredicate::parse(
                        &format!("D{prefix}{suffix}"),
                        &["F0".into(), "K0".into()]
                    )
                    .is_err()
                );
            }
            for wide in ["", "D"] {
                for suffix in ["Z>", "Z>=", "Z<", "Z<=", "Z=", "Z<>"] {
                    for index in ["E0", "F0"] {
                        for position in 0..3 {
                            let mut args = ["K0".into(), "K0".into(), "K0".into()];
                            args[position] = index.into();
                            assert!(
                                NumericPredicate::parse(&format!("{wide}{prefix}{suffix}"), &args)
                                    .is_err()
                            );
                        }
                    }
                }
            }
        }
        for bad in ["E8", "F8", "E00", "E-1", "F1.5", "D0E0"] {
            assert!(NumericPredicate::parse("LD&", &[bad.into(), "K1".into()]).is_err());
        }
        let p = predicate("DLD&", &["E7", "K8M0"]);
        let mut tracked = BTreeSet::new();
        p.devices(&mut tracked);
        assert_eq!(tracked.len(), 34);
        assert!(tracked.contains(&d("D1194")) && tracked.contains(&d("D1195")));
        assert_eq!(p.label("OR"), "DOR& E7 K8M0");
        m.set_internal(d("M0"), true);
        assert!(predicate("BLD", &["K4M0", "K0"]).evaluate(&m));
        assert!(NumericPredicate::parse("BLD", &["E0".into(), "K0".into()]).is_err());
    }

    #[test]
    fn exact_contact_spelling_inventory_and_no_pulse_suffixes() {
        let mut count = 0;
        for wide in ["", "D"] {
            for prefix in ["LD", "AND", "OR"] {
                for suffix in ["&", "|", "^", "Z>", "Z>=", "Z<", "Z<=", "Z=", "Z<>"] {
                    let name = format!("{wide}{prefix}{suffix}");
                    let spec = parse_spec(&name).unwrap();
                    assert_eq!(spec.prefix, prefix);
                    assert_eq!(spec.wide, wide == "D");
                    assert_eq!(spec.arity, if suffix.starts_with('Z') { 3 } else { 2 });
                    assert!(parse_spec(&format!("{name}P")).is_none());
                    count += 1;
                }
            }
        }
        assert_eq!(count, 54);
        for name in [
            "LD", "DLD", "LD=", "LDZ", "LDD&", "DDLD&", "LD&&", "ORZ!=", "ANDZ==",
        ] {
            assert!(parse_spec(name).is_none(), "{name}");
        }
    }

    #[test]
    fn bitwise_contacts_test_nonzero_including_sign_bits() {
        let m = Memory::default();
        for (name, args, expected) in [
            ("LD&", ["H8000", "H8000"], true),
            ("LD&", ["K1", "K2"], false),
            ("LD|", ["K0", "H8000"], true),
            ("LD^", ["HFFFF", "HFFFF"], false),
            ("DLD&", ["H80000000", "H80000000"], true),
            ("DLD^", ["H80000000", "K0"], true),
            ("DLD|", ["K0", "K0"], false),
        ] {
            assert_eq!(predicate(name, &args).evaluate(&m), expected, "{name}");
        }
    }

    #[test]
    fn absolute_difference_wraps_width_and_maps_signed_minimum_to_zero() {
        let m = Memory::default();
        for (name, args, expected) in [
            ("LDZ<=", ["K170", "K200", "K-30"], true),
            ("LDZ<=", ["K231", "K200", "K-30"], false),
            ("LDZ=", ["K-32768", "K0", "K-32768"], true),
            ("LDZ=", ["K32767", "K-32768", "K1"], true),
            ("LDZ=", ["K-32768", "K0", "K0"], true),
            ("LDZ=", ["K0", "K0", "K-32768"], true),
            ("LDZ>", ["K32767", "K-32768", "K-32768"], true),
            (
                "DLDZ>",
                ["K2147483647", "K-2147483648", "K-2147483648"],
                true,
            ),
            ("DLDZ=", ["K-2147483648", "K0", "K-2147483648"], true),
            ("DLDZ=", ["K2147483647", "K-2147483648", "K1"], true),
            ("DLDZ=", ["K-2147483648", "K0", "K0"], true),
            ("DLDZ=", ["K0", "K0", "K-2147483648"], true),
        ] {
            let p = predicate(name, &args);
            assert_eq!(p.evaluate(&m), expected);
            let swapped = predicate(name, &[args[1], args[0], args[2]]);
            assert_eq!(p.evaluate(&m), swapped.evaluate(&m));
        }
    }

    #[test]
    fn compiler_nested_numeric_contacts_and_mps_keep_boolean_structure() {
        let p = compile("LD X0\nLD& H8000 H8000\nORZ= K400 K200 K-200\nANB\nMPS\nAND| K0 K1\nOUT Y0\nMPP\nDANDZ<= K170 K200 K-30\nOUT Y1\nEND").unwrap();
        let mut s = Simulator::default();
        for input in [false, true, false] {
            s.memory.set_input(d("X0"), input);
            s.scan(&p, 100);
            assert_eq!(s.memory.bit(d("Y0")), input);
            assert_eq!(s.memory.bit(d("Y1")), input);
        }
        assert!(compile("AND& K1 K1\nOUT Y0\nEND").is_err());
        assert!(compile("LD& K1\nOUT Y0\nEND").is_err());
        assert!(compile("LDZ= K1 K1\nOUT Y0\nEND").is_err());
        assert!(compile("LD& K1 K1\nOUT Y0\nANDZ= K1 K1 K0\nOUT Y1\nEND").is_err());
    }

    #[test]
    fn labels_and_source_tracking_cover_word_pairs_and_natural_counters() {
        let p = predicate("DANDZ>=", &["D0", "C200", "T254"]);
        assert_eq!(p.label("OR"), "DORZ>= D0 C200 T254");
        let mut devices = BTreeSet::new();
        p.devices(&mut devices);
        assert_eq!(
            devices,
            ["D0", "D1", "C200", "T254", "T255"]
                .map(d)
                .into_iter()
                .collect()
        );
        assert!(NumericPredicate::parse("LD&", &["C200".into(), "K1".into()]).is_err());
        let p = compile("LD& D10 T0\nOUT Y0\nDLDZ= D20 C255 K0\nOUT Y1\nEND").unwrap();
        assert!(
            ["D10", "T0", "D20", "D21", "C255"]
                .iter()
                .all(|name| p.devices.contains(&d(name)))
        );
    }

    #[test]
    fn indexed_contact_spelling_and_operand_matrix_are_exact() {
        for wide in ["", "D"] {
            for (name, prefix, inverted) in [
                ("BLD", "LD", false),
                ("BLDI", "LD", true),
                ("BAND", "AND", false),
                ("BANI", "AND", true),
                ("BOR", "OR", false),
                ("BORI", "OR", true),
            ] {
                let name = format!("{wide}{name}");
                let spec = parse_spec(&name).unwrap();
                assert_eq!(spec.prefix, prefix);
                assert_eq!(spec.arity, 2);
                assert_eq!(spec.kind, ContactKind::Indexed(inverted));
                assert!(parse_spec(&format!("{name}P")).is_none());
                let p = predicate(&name, &["D0", "K0"]);
                assert_eq!(p.label(prefix), format!("{name} D0 K0"));
            }
        }
        for (name, source, index) in [
            ("BLD", "K1", "K0"),
            ("BLD", "HFFFF", "K0"),
            ("BLD", "X0", "K0"),
            ("BLD", "C200", "K0"),
            ("BLD", "D0", "K16"),
            ("DBLDI", "D0", "K32"),
        ] {
            assert!(NumericPredicate::parse(name, &[source.into(), index.into()]).is_err());
        }
        assert!(NumericPredicate::parse("DBLD", &["C200".into(), "K31".into()]).is_ok());
    }

    #[test]
    fn dynamic_indexed_contacts_wrap_negative_and_upper_bits_without_errors() {
        let mut m = Memory::default();
        m.set_word(d("D0"), i16::MIN);
        m.set_word(d("D10"), -1);
        assert!(predicate("BLD", &["D0", "D10"]).evaluate(&m));
        assert!(!predicate("BLDI", &["D0", "D10"]).evaluate(&m));
        m.set_word(d("D0"), 1);
        m.set_word(d("D10"), 16);
        assert!(predicate("BLD", &["D0", "D10"]).evaluate(&m));
        m.set_double_word(d("D0"), i32::MIN);
        for index in [-1, 31, 65567] {
            m.set_double_word(d("D10"), index);
            assert!(predicate("DBLD", &["D0", "D10"]).evaluate(&m));
            assert!(!predicate("DBLDI", &["D0", "D10"]).evaluate(&m));
        }
        assert!(!m.system_bit(1067));
        assert!(!m.system_bit(1068));
        let mut devices = BTreeSet::new();
        predicate("DBLD", &["C200", "D10"]).devices(&mut devices);
        assert_eq!(devices, ["C200", "D10", "D11"].map(d).into_iter().collect());
    }

    #[test]
    fn indexed_contacts_preserve_nested_blocks_and_mps_series_parallel_semantics() {
        let p = compile("BLD D0 K0\nLD X0\nDBLD C200 K31\nORB\nANB\nMPS\nBANI D0 K1\nOUT Y0\nMPP\nBOR D0 K1\nOUT Y1\nEND").unwrap();
        let mut s = Simulator::default();
        for value in [0, 1, 2, 3] {
            for input in [false, true] {
                for counter in [0, i32::MIN] {
                    s.memory.set_word(d("D0"), value);
                    s.memory.set_input(d("X0"), input);
                    s.memory.set_double_word(d("C200"), counter);
                    s.scan(&p, 100);
                    let condition = value & 1 != 0 && (input || counter < 0);
                    assert_eq!(s.memory.bit(d("Y0")), condition && value & 2 == 0);
                    assert_eq!(s.memory.bit(d("Y1")), condition || value & 2 != 0);
                }
            }
        }
        assert!(compile("BAND D0 K0\nOUT Y0\nEND").is_err());
        assert!(compile("BLDP D0 K0\nOUT Y0\nEND").is_err());
    }
}
