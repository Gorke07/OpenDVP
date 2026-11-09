//! Device configuration: the right-side extension modules of a DVP PLC and the I/O
//! addresses they receive.
//!
//! Addressing follows Delta's numbering for right-side extension units: digital inputs
//! and outputs continue from X20 / Y20 (octal), each module taking whole groups of 8; special
//! modules (analog, temperature, positioning) are numbered K0..K7 in slot order and are
//! reached through their control registers (CR) with FROM/TO. A real PLC reports the
//! totals in D1142/D1143 and the special-module types in D1320+ (`plc_modules::scan`).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Digital I/O: inputs and outputs, output technology.
    Digital {
        inputs: u16,
        outputs: u16,
        output: &'static str,
    },
    /// Special module with a type code (D1320+i / CR0) and channel description.
    Special { code: u16, channels: &'static str },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Module {
    pub name: &'static str,
    pub group: &'static str,
    pub summary: &'static str,
    pub kind: Kind,
}

impl Module {
    pub fn is_special(&self) -> bool {
        matches!(self.kind, Kind::Special { .. })
    }
}

const fn digital(
    name: &'static str,
    group: &'static str,
    summary: &'static str,
    inputs: u16,
    outputs: u16,
    output: &'static str,
) -> Module {
    Module {
        name,
        group,
        summary,
        kind: Kind::Digital {
            inputs,
            outputs,
            output,
        },
    }
}

const fn special(
    name: &'static str,
    group: &'static str,
    summary: &'static str,
    code: u16,
    channels: &'static str,
) -> Module {
    Module {
        name,
        group,
        summary,
        kind: Kind::Special { code, channels },
    }
}

pub const GROUPS: &[&str] = &[
    "Dijital giriş",
    "Dijital çıkış",
    "Dijital giriş / çıkış",
    "Analog",
    "Sıcaklık",
    "Konumlama / sayıcı",
];

const DI: &str = "Dijital giriş";
const DO: &str = "Dijital çıkış";
const DIO: &str = "Dijital giriş / çıkış";
const AN: &str = "Analog";
const TEMP: &str = "Sıcaklık";
const POS: &str = "Konumlama / sayıcı";

pub const CATALOG: &[Module] = &[
    digital("DVP08SM11N", DI, "8 dijital giriş (24 VDC)", 8, 0, ""),
    digital("DVP16SM11N", DI, "16 dijital giriş (24 VDC)", 16, 0, ""),
    digital("DVP08SN11R", DO, "8 röle çıkış", 0, 8, "Röle"),
    digital(
        "DVP08SN11T",
        DO,
        "8 NPN transistör çıkış",
        0,
        8,
        "NPN transistör",
    ),
    digital(
        "DVP16SN11T",
        DO,
        "16 NPN transistör çıkış",
        0,
        16,
        "NPN transistör",
    ),
    digital("DVP06SN11R", DO, "6 röle çıkış", 0, 6, "Röle"),
    digital(
        "DVP08SP11R",
        DIO,
        "4 dijital giriş + 4 röle çıkış",
        4,
        4,
        "Röle",
    ),
    digital(
        "DVP08SP11T",
        DIO,
        "4 dijital giriş + 4 NPN transistör çıkış",
        4,
        4,
        "NPN transistör",
    ),
    digital(
        "DVP16SP11R",
        DIO,
        "8 dijital giriş + 8 röle çıkış",
        8,
        8,
        "Röle",
    ),
    digital(
        "DVP16SP11T",
        DIO,
        "8 dijital giriş + 8 NPN transistör çıkış",
        8,
        8,
        "NPN transistör",
    ),
    digital(
        "DVP16SP11TS",
        DIO,
        "8 dijital giriş + 8 PNP transistör çıkış",
        8,
        8,
        "PNP transistör",
    ),
    digital("DVP16XM211N", DI, "16 dijital giriş (E2 tipi)", 16, 0, ""),
    digital("DVP16XN211R", DO, "16 röle çıkış (E2 tipi)", 0, 16, "Röle"),
    digital(
        "DVP16XN211T",
        DO,
        "16 NPN transistör çıkış (E2 tipi)",
        0,
        16,
        "NPN transistör",
    ),
    digital(
        "DVP24XP200R",
        DIO,
        "16 dijital giriş + 8 röle çıkış (E2 tipi)",
        16,
        8,
        "Röle",
    ),
    digital(
        "DVP32XP200R",
        DIO,
        "16 dijital giriş + 16 röle çıkış (E2 tipi)",
        16,
        16,
        "Röle",
    ),
    special(
        "DVP04AD-S2",
        AN,
        "4 kanal analog giriş",
        144,
        "4 AI · gerilim / akım",
    ),
    special(
        "DVP04DA-S2",
        AN,
        "4 kanal analog çıkış",
        145,
        "4 AO · gerilim / akım",
    ),
    special(
        "DVP06XA-S2",
        AN,
        "4 analog giriş + 2 analog çıkış",
        212,
        "4 AI + 2 AO",
    ),
    special("DVP04AD-S", AN, "4 kanal analog giriş", 136, "4 AI"),
    special("DVP04DA-S", AN, "4 kanal analog çıkış", 137, "4 AO"),
    special(
        "DVP06XA-S",
        AN,
        "4 analog giriş + 2 analog çıkış",
        204,
        "4 AI + 2 AO",
    ),
    special("DVP02DA-S", AN, "2 kanal analog çıkış", 73, "2 AO"),
    special(
        "DVP04AD-E2",
        AN,
        "4 kanal analog giriş (E2 tipi)",
        128,
        "4 AI",
    ),
    special(
        "DVP04DA-E2",
        AN,
        "4 kanal analog çıkış (E2 tipi)",
        129,
        "4 AO",
    ),
    special(
        "DVP06XA-E2",
        AN,
        "4 AI + 2 AO (E2 tipi)",
        196,
        "4 AI + 2 AO",
    ),
    special(
        "DVP04PT-S",
        TEMP,
        "4 kanal PT100 / PT1000 sıcaklık",
        138,
        "4 × RTD",
    ),
    special("DVP04TC-S", TEMP, "4 kanal termokupl", 139, "4 × TC"),
    special("DVP08NTC-S", TEMP, "8 kanal NTC sıcaklık", 269, "8 × NTC"),
    special(
        "DVP04PT-E2",
        TEMP,
        "4 kanal PT100 (E2 tipi)",
        130,
        "4 × RTD",
    ),
    special(
        "DVP04TC-E2",
        TEMP,
        "4 kanal termokupl (E2 tipi)",
        131,
        "4 × TC",
    ),
    special("DVP01PU-S", POS, "1 eksen darbe konumlama", 272, "1 eksen"),
    special(
        "DVP02PU-E2",
        POS,
        "2 eksen darbe konumlama (E2 tipi)",
        69,
        "2 eksen",
    ),
];

/// Right-side special modules a DVP CPU addresses (K0..K7).
pub const MAX_SPECIAL: usize = 8;
/// Slots offered in the rack view.
pub const MAX_MODULES: usize = 14;
/// First octal X/Y number of right-side digital extension units.
pub const EXTENSION_BASE: u16 = 0o20;

pub fn find(name: &str) -> Option<&'static Module> {
    CATALOG.iter().find(|m| m.name.eq_ignore_ascii_case(name))
}

/// Addresses one configured module receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assignment {
    Digital {
        /// Octal X index range (`first..first+count`), `None` without inputs.
        inputs: Option<(u16, u16)>,
        outputs: Option<(u16, u16)>,
    },
    Special {
        number: u8,
    },
}

fn round8(n: u16) -> u16 {
    n.div_ceil(8) * 8
}

/// Address assignment for modules in slot order. Unknown names are reported as errors.
pub fn assign(modules: &[String]) -> Result<Vec<Assignment>, String> {
    if modules.len() > MAX_MODULES {
        return Err(format!("En çok {MAX_MODULES} genişleme modülü eklenebilir"));
    }
    let mut x = EXTENSION_BASE;
    let mut y = EXTENSION_BASE;
    let mut specials = 0u8;
    let mut out = Vec::with_capacity(modules.len());
    for name in modules {
        let module = find(name).ok_or_else(|| format!("Bilinmeyen modül: {name}"))?;
        out.push(match module.kind {
            Kind::Digital {
                inputs, outputs, ..
            } => {
                let a = Assignment::Digital {
                    inputs: (inputs > 0).then_some((x, inputs)),
                    outputs: (outputs > 0).then_some((y, outputs)),
                };
                x += round8(inputs);
                y += round8(outputs);
                a
            }
            Kind::Special { .. } => {
                if usize::from(specials) >= MAX_SPECIAL {
                    return Err(format!(
                        "En çok {MAX_SPECIAL} özel modül (K0–K7) eklenebilir"
                    ));
                }
                specials += 1;
                Assignment::Special {
                    number: specials - 1,
                }
            }
        });
    }
    Ok(out)
}

/// `X20–X27` style text for an octal range.
pub fn range_text(prefix: char, (first, count): (u16, u16)) -> String {
    let last = first + count - 1;
    if count == 1 {
        format!("{prefix}{first:o}")
    } else {
        format!("{prefix}{first:o}–{prefix}{last:o}")
    }
}

/// Every extension X/Y device index with the module that provides it.
pub fn extension_devices(modules: &[String]) -> Vec<(char, u16, &'static str)> {
    let Ok(assignments) = assign(modules) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, assignment) in modules.iter().zip(assignments) {
        let module = find(name).map_or("", |m| m.name);
        if let Assignment::Digital { inputs, outputs } = assignment {
            for (prefix, range) in [('X', inputs), ('Y', outputs)] {
                if let Some((first, count)) = range {
                    out.extend((first..first + count).map(|i| (prefix, i, module)));
                }
            }
        }
    }
    out
}

/// A conservative, portable 16-bit transfer template. The helper uses at most
/// four CR words and CR0..48, common to the existing ES2/SS2 model profiles.
/// This validates addressing and compilation, not the selected module's CR semantics.
pub fn transfer_source(modules: &[String], index: usize, read: bool, cr: u16, data: &str, count: u16) -> Result<String, String> {
    let assignments = assign(modules)?;
    let Some(Assignment::Special { number }) = assignments.get(index) else {
        return Err("FROM/TO yardımcısı için özel modül seçin.".into());
    };
    if !read && cr == 0 { return Err("CR0 modül tipini bildirir; yazma hedefi olarak kullanılamaz.".into()); }
    if !(1..=4).contains(&count) || cr.checked_add(count - 1).is_none_or(|end| end > 48) {
        return Err("Yardımcı CR0..48 ve aktarım başına 1..4 kelime destekler.".into());
    }
    let data: crate::plc::Device = data.parse()?;
    crate::recipes::validate_target(data, crate::st::DataType::Int)?;
    let end = data.index.checked_add(count - 1).ok_or("D adres aralığı taşıyor.")?;
    crate::recipes::validate_target(crate::plc::Device { index: end, ..data }, crate::st::DataType::Int)?;
    if data.index <= 1999 && end >= 1000 { return Err("D aktarımı özel sistem alanıyla çakışıyor.".into()); }
    let name = &modules[index];
    let op = if read { "FROM" } else { "TO" };
    let source = format!("; {name} / slot {} / K{number}\nLD M1000\n{op} K{number} K{cr} {data} K{count}\nEND\n", index + 2);
    crate::plc::compile(&source).map_err(|e| e.to_string())?;
    Ok(source)
}

/// One special-module slot in a configuration/PLC comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecialSlot {
    pub number: u8,
    pub configured: Option<&'static str>,
    pub found_code: Option<u16>,
    pub found: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanComparison {
    pub specials: Vec<SpecialSlot>,
    /// Configured digital extension points (inputs, outputs).
    pub configured_points: (u16, u16),
    /// Points the PLC reports in D1142/D1143.
    pub found_points: (u16, u16),
    pub matches: bool,
}

/// Compare the configuration with what the PLC reports. Special modules are matched by
/// type code in K order. For digital points, D1142/D1143 is accepted either as the exact
/// point count or rounded to whole groups of 8 (Delta does not document which).
pub fn compare_scan(
    configured: &[String],
    scan: &crate::plc_modules::ModuleScan,
) -> ScanComparison {
    let mut wanted = Vec::new();
    let (mut x, mut y, mut x8, mut y8) = (0u16, 0u16, 0u16, 0u16);
    for module in configured.iter().filter_map(|n| find(n)) {
        match module.kind {
            Kind::Special { code, .. } => wanted.push((module.name, code)),
            Kind::Digital {
                inputs, outputs, ..
            } => {
                x += inputs;
                y += outputs;
                x8 += round8(inputs);
                y8 += round8(outputs);
            }
        }
    }
    let slots = wanted.len().max(scan.types.len());
    let specials: Vec<SpecialSlot> = (0..slots)
        .map(|i| {
            let found_code = scan.types.get(i).copied();
            SpecialSlot {
                number: i as u8,
                configured: wanted.get(i).map(|(n, _)| *n),
                found_code,
                found: found_code.and_then(crate::plc_modules::module_name),
            }
        })
        .collect();
    let specials_match = wanted.len() == scan.types.len()
        && wanted.iter().zip(&scan.types).all(|((_, c), f)| c == f);
    let found = (scan.x_points, scan.y_points);
    let points_match = found == (x, y) || found == (x8, y8);
    ScanComparison {
        specials,
        configured_points: (x, y),
        found_points: found,
        matches: specials_match && points_match,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plc_scan_is_compared_by_special_codes_and_point_totals() {
        let config = names(&["DVP16SP11R", "DVP06XA-S2", "DVP04PT-S"]);
        let scan = crate::plc_modules::ModuleScan {
            types: vec![212, 138],
            x_points: 8,
            y_points: 8,
        };
        let same = compare_scan(&config, &scan);
        assert!(same.matches, "{same:?}");
        let swapped = crate::plc_modules::ModuleScan {
            types: vec![138, 212],
            ..scan.clone()
        };
        let diff = compare_scan(&config, &swapped);
        assert!(!diff.matches);
        assert_eq!(diff.specials[0].found, Some("DVP04PT-S"));
        assert_eq!(diff.specials[0].configured, Some("DVP06XA-S2"));
        // A module the configuration does not have.
        let extra = crate::plc_modules::ModuleScan {
            types: vec![212, 138, 144],
            ..scan
        };
        let missing = compare_scan(&config, &extra);
        assert_eq!(missing.specials.len(), 3);
        assert_eq!(missing.specials[2].configured, None);
        assert!(!missing.matches);
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn digital_modules_continue_from_x20_y20_in_groups_of_eight() {
        let a = assign(&names(&[
            "DVP16SP11R",
            "DVP06XA-S2",
            "DVP08SP11T",
            "DVP06SN11R",
        ]))
        .unwrap();
        assert_eq!(
            a[0],
            Assignment::Digital {
                inputs: Some((0o20, 8)),
                outputs: Some((0o20, 8))
            }
        );
        assert_eq!(a[1], Assignment::Special { number: 0 });
        // 4 inputs still take a whole group of 8: next module starts at X40.
        assert_eq!(
            a[2],
            Assignment::Digital {
                inputs: Some((0o30, 4)),
                outputs: Some((0o30, 4))
            }
        );
        assert_eq!(
            a[3],
            Assignment::Digital {
                inputs: None,
                outputs: Some((0o40, 6))
            }
        );
        assert_eq!(range_text('X', (0o20, 8)), "X20–X27");
        assert_eq!(range_text('Y', (0o40, 6)), "Y40–Y45");
    }

    #[test]
    fn limits_and_unknown_modules_are_rejected() {
        assert!(assign(&names(&["DVP99XX"])).is_err());
        let nine = vec!["DVP04AD-S2".to_string(); 9];
        assert!(assign(&nine).unwrap_err().contains("K0–K7"));
        let many = vec!["DVP08SM11N".to_string(); MAX_MODULES + 1];
        assert!(assign(&many).is_err());
    }

    #[test]
    fn special_codes_match_the_plc_module_table() {
        for module in CATALOG {
            if let Kind::Special { code, .. } = module.kind {
                assert_eq!(
                    crate::plc_modules::module_name(code),
                    Some(module.name),
                    "{}",
                    module.name
                );
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        assert!(CATALOG.iter().all(|m| seen.insert(m.name)));
        assert!(CATALOG.iter().all(|m| GROUPS.contains(&m.group)));
    }

    #[test]
    fn extension_devices_list_every_point_with_its_module() {
        let devices = extension_devices(&names(&["DVP16SP11T"]));
        assert_eq!(devices.len(), 16);
        assert_eq!(devices[0], ('X', 0o20, "DVP16SP11T"));
        assert_eq!(devices[15], ('Y', 0o27, "DVP16SP11T"));
    }

    #[test]
    fn transfer_helper_uses_special_number_not_physical_slot() {
        let modules = names(&["DVP08SM11N", "DVP04AD-E2", "DVP04AD-S2"]);
        let read = transfer_source(&modules, 2, true, 0, "D100", 1).unwrap();
        assert!(read.contains("FROM K1 K0 D100 K1"));
        assert!(crate::plc::compile(&read).is_ok());
        assert!(transfer_source(&modules, 1, false, 5, "D200", 4).unwrap().contains("TO K0 K5 D200 K4"));
        assert!(transfer_source(&modules, 0, true, 0, "D100", 1).is_err());
        assert!(transfer_source(&modules, 1, true, 48, "D100", 2).is_err());
        assert!(transfer_source(&modules, 1, true, 0, "D9999", 2).is_err());
        assert!(transfer_source(&modules, 1, true, 0, "D999", 4).is_err());
        assert!(transfer_source(&modules, 1, false, 0, "D100", 1).is_err());
    }
}
