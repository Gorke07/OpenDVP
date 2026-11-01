//! Per-model support of DVP special relays (M1000+) and special registers (D1000+).
//!
//! Data: Delta editor `Data\Converter\DVP2AS_SM.csv` / `DVP2AS_SD.csv` (the DVP→AS
//! project converter tables), re-encoded from Big5 to UTF-8 without other changes
//! (`src/data/`). Each row lists one device or range with a cell per DVP model:
//! `○` supported, `╳` not supported, a firmware version (for example `V3.60`) meaning
//! supported from that version, and an empty cell meaning the table makes no statement.
use crate::plc::{Area, Device};
use std::sync::OnceLock;

const SM_CSV: &str = include_str!("data/DVP2AS_SM.csv");
const SD_CSV: &str = include_str!("data/DVP2AS_SD.csv");

/// Model columns in table order.
pub const MODELS: [&str; 25] = [
    "ES", "EC", "EC3", "SS", "EX", "SA", "SX", "SC", "EH", "EH2", "EH2L", "SV", "ES2", "EX2",
    "SS2", "SA2", "SX2", "MC", "SE", "SE2", "TP04P", "TP70", "EH3", "EH3L", "SV2",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support<'a> {
    Yes,
    No,
    /// Supported from the quoted firmware note (kept verbatim, e.g. "V3.60").
    FromFirmware(&'a str),
    /// The table has no statement for this model.
    Unspecified,
}

struct Row {
    area: Area,
    first: u16,
    last: u16,
    cells: Vec<String>,
}

/// RFC 4180 records: quoted fields may contain commas, `""` and line breaks
/// (DVP2AS_SD.csv D1147 has a two-line firmware note).
fn records(csv: &str) -> Vec<Vec<String>> {
    const CR: char = '\u{0D}';
    const LF: char = '\u{0A}';
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut chars = csv.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => record.push(std::mem::take(&mut field)),
            (false, CR) => {}
            (false, LF) => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

fn parse(csv: &str, area: Area, prefix: char) -> Vec<Row> {
    let mut rows = Vec::new();
    for fields in records(csv).into_iter().skip(1) {
        if fields.len() < 26 {
            continue;
        }
        let name = fields[0].trim();
        let parse_index = |s: &str| s.trim().strip_prefix(prefix)?.parse::<u16>().ok();
        let (first, last) = match name.split_once('~') {
            Some((a, b)) => match (parse_index(a), parse_index(b)) {
                (Some(a), Some(b)) if a <= b => (a, b),
                _ => continue,
            },
            None => match parse_index(name) {
                Some(i) => (i, i),
                None => continue,
            },
        };
        let cells = fields[1..26]
            .iter()
            .map(|f| f.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect();
        rows.push(Row {
            area,
            first,
            last,
            cells,
        });
    }
    rows
}

fn table() -> &'static [Row] {
    static TABLE: OnceLock<Vec<Row>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut rows = parse(SM_CSV, Area::M, 'M');
        rows.extend(parse(SD_CSV, Area::D, 'D'));
        rows
    })
}

pub fn model_index(model: &str) -> Option<usize> {
    let wanted = model
        .trim()
        .trim_start_matches("DVP")
        .trim_start_matches('-')
        .replace('-', "");
    MODELS.iter().position(|m| m.eq_ignore_ascii_case(&wanted))
}

fn cell_support(cell: &str) -> Support<'_> {
    match cell {
        "○" => Support::Yes,
        "╳" => Support::No,
        "" => Support::Unspecified,
        note => Support::FromFirmware(note),
    }
}

fn rank(support: &Support<'_>) -> u8 {
    match support {
        Support::No => 0,
        Support::Unspecified => 1,
        Support::FromFirmware(_) => 2,
        Support::Yes => 3,
    }
}

/// Support of a special M (≥1000) or D (≥1000) device on `model`, or `None` when the
/// device is not listed in Delta's tables. Some devices have one row per model family
/// (e.g. M1019); the most permissive statement for the model is returned.
pub fn support(model: &str, device: Device) -> Option<Support<'static>> {
    let column = model_index(model)?;
    table()
        .iter()
        .filter(|r| r.area == device.area && (r.first..=r.last).contains(&device.index))
        .map(|r| cell_support(&r.cells[column]))
        .max_by_key(rank)
}

/// Special devices in `devices` that Delta's table marks unsupported on `model`.
pub fn unsupported<'a>(model: &str, devices: impl IntoIterator<Item = &'a Device>) -> Vec<Device> {
    let mut found: Vec<Device> = devices
        .into_iter()
        .filter(|d| matches!(support(model, **d), Some(Support::No)))
        .copied()
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Special M/D devices named in IL text that `model` does not support. Indexed or computed
/// addresses are not visible in the text and are not reported.
pub fn unsupported_in_il(model: &str, il: &str) -> Vec<Device> {
    let devices: Vec<Device> = il
        .lines()
        .map(|line| line.split(';').next().unwrap_or_default())
        .flat_map(|code| code.split(|c: char| !c.is_ascii_alphanumeric()))
        .filter_map(|token| {
            let area = match token.as_bytes().first()? {
                b'M' | b'm' => Area::M,
                b'D' | b'd' => Area::D,
                _ => return None,
            };
            let index: u16 = token[1..].parse().ok()?;
            (index >= 1000).then_some(Device { area, index })
        })
        .collect();
    unsupported(model, &devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_unsupported_devices_in_il_text() {
        let il = "LD M1019 ; M1135 yorumda\nMOV K1 D1147\nOUT Y0\nEND";
        // M1135 is only in a comment; D1147 is "╳" for ES.
        assert_eq!(
            unsupported_in_il("ES", il),
            [dev(Area::M, 1019), dev(Area::D, 1147)]
        );
        assert!(unsupported_in_il("ES2", il).is_empty());
    }

    fn dev(area: Area, index: u16) -> Device {
        Device { area, index }
    }

    #[test]
    fn tables_load_completely() {
        let rows = table();
        assert!(rows.iter().filter(|r| r.area == Area::M).count() > 800);
        assert!(rows.iter().filter(|r| r.area == Area::D).count() > 700);
    }

    #[test]
    fn known_rows_match_delta_tables() {
        // M1000 (RUN monitor) is supported on every model.
        assert_eq!(support("ES2", dev(Area::M, 1000)), Some(Support::Yes));
        assert_eq!(support("DVP-SS2", dev(Area::M, 1000)), Some(Support::Yes));
        // M1019 has two family rows; ES2 is "V3.60" in one, SE has a firmware note, ES "╳".
        assert_eq!(
            support("ES2", dev(Area::M, 1019)),
            Some(Support::FromFirmware("V3.60"))
        );
        assert_eq!(
            support("SE", dev(Area::M, 1019)),
            Some(Support::FromFirmware("12SE V1.88 26SE V1.00"))
        );
        assert_eq!(support("ES", dev(Area::M, 1019)), Some(Support::No));
        // D1147: a quoted two-line firmware note in the ES2 column.
        assert_eq!(
            support("ES2", dev(Area::D, 1147)),
            Some(Support::FromFirmware("40ES2M V3.41"))
        );
        // Ranges: D1050~D1055.
        assert_eq!(support("ES2", dev(Area::D, 1053)), Some(Support::Yes));
        assert_eq!(support("EH2L", dev(Area::D, 1053)), Some(Support::Yes));
        assert_eq!(
            support("SV", dev(Area::D, 1053)),
            Some(Support::Unspecified)
        );
        // Not a special device / unknown model.
        assert_eq!(support("ES2", dev(Area::D, 10)), None);
        assert_eq!(support("FX3U", dev(Area::M, 1000)), None);
    }

    #[test]
    fn reports_unsupported_devices_once() {
        let devices = [dev(Area::M, 1019), dev(Area::M, 1000), dev(Area::M, 1019)];
        assert_eq!(unsupported("ES", &devices), [dev(Area::M, 1019)]);
        assert!(unsupported("ES2", &devices).is_empty());
    }
}
