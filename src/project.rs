use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

pub const DEMO: &str = "; Motor mühürleme · X0 başlat / X1 durdur\nLD X0\nOR Y0\nANI X1\nOUT Y0\n\n; Çalıştıktan 3 saniye sonra hazır çıkışı\nLD Y0\nTMR T0 K30\n\n; Hazır lambası\nLD T0\nOUT Y1\n\n; Parça sayacı · X2 sensör\nLD X2\nCNT C0 K5\n\n; Sayacı sıfırla · X3\nLD X3\nRST C0\n\n; Üretim ayarı\nLD Y0\nMOV K125 D0\nEND\n";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceLanguage {
    #[default]
    Il,
    St,
}
pub const ST_TEMPLATE: &str = "PROGRAM Main\nVAR\n    start AT X0 : BOOL;\n    run AT Y0 : BOOL;\nEND_VAR\n\nrun := start;\nEND_PROGRAM\n";
impl SourceLanguage {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Il => "il",
            Self::St => "st",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Il => "IL",
            Self::St => "Structured Text",
        }
    }
}

/// Kind of an extra program unit; both live behind `FEND` in the PLC program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnitKind {
    /// Called with `CALL P<n>` (or `CALL <name>`); ends with `SRET`.
    Subroutine,
    /// Interrupt routine `I<nnn>`; ends with `IRET`.
    Interrupt,
    /// A further part of the main program: a program in the cyclic task, run after Main
    /// in the order of the list (Delta editor: several POUs assigned to the cyclic task).
    Cyclic,
}
impl UnitKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Subroutine => "Alt program",
            Self::Interrupt => "Kesme",
            Self::Cyclic => "Ana program parçası",
        }
    }
    /// Cyclic parts have no pointer; the other kinds are entered by label.
    pub fn has_pointer(self) -> bool {
        self != Self::Cyclic
    }
    /// Label line in the PLC program: `P0`, `I001`.
    pub fn pointer(self, number: u16) -> String {
        match self {
            Self::Subroutine => format!("P{number}"),
            Self::Interrupt => format!("I{number:03}"),
            Self::Cyclic => String::new(),
        }
    }
    fn closing(self) -> &'static str {
        match self {
            Self::Subroutine => "SRET",
            Self::Interrupt => "IRET",
            Self::Cyclic => "",
        }
    }
    /// Whether the PLC knows `number` as this kind of pointer: `P0..P255` for the ES2
    /// family, or one of the documented interrupt labels (`I001` = X0 rising edge, timer
    /// `I602..I699`, high-speed counter `I010..I080`, COM `I140..I160`).
    pub fn number_valid(self, number: u16) -> bool {
        match self {
            Self::Subroutine => number <= 255,
            Self::Interrupt => crate::interrupt::InterruptId::from_str_label(number).is_some(),
            Self::Cyclic => number == 0,
        }
    }
    /// What triggers an interrupt label, e.g. `X0 ↑`; none for subroutines.
    pub fn pointer_note(self, number: u16) -> Option<String> {
        use crate::interrupt::InterruptId;
        if self != Self::Interrupt {
            return None;
        }
        Some(match InterruptId::from_str_label(number)? {
            InterruptId::External { input, rising } => {
                format!("X{input} {}", if rising { "↑" } else { "↓" })
            }
            InterruptId::Timer { channel, period } => {
                format!("{period} {}", if channel == 2 { "×0,1 ms" } else { "ms" })
            }
            InterruptId::Counter { slot } => format!("HSC {slot}"),
            InterruptId::Communication { port } => format!("COM{port}"),
        })
    }
}
/// An IL program unit besides Main. `source` ends with `END` like the Main source so the
/// same editing code works on every unit; the END is dropped when the units are joined.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub name: String,
    pub kind: UnitKind,
    /// Pointer number: `P<n>` for a subroutine, `I<nnn>` for an interrupt.
    pub number: u16,
    pub source: String,
    /// A disabled unit stays in the project but is left out of the compiled program.
    #[serde(default = "enabled_by_default", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// IL (default) or Structured Text.
    #[serde(default, skip_serializing_if = "is_il")]
    pub language: SourceLanguage,
}
fn enabled_by_default() -> bool {
    true
}
fn is_true(value: &bool) -> bool {
    *value
}
impl Unit {
    /// `Pump · P0`, `INT_1 · I001 · X0 ↑`, `PRG_1 · cyclic`; marked when disabled.
    pub fn label(&self) -> String {
        use crate::i18n::tr;
        let mut label = match (self.kind.has_pointer(), self.kind.pointer_note(self.number)) {
            (false, _) => format!("{} · {}", self.name, tr("Döngüsel")),
            (true, Some(note)) => {
                format!("{} · {} · {note}", self.name, self.kind.pointer(self.number))
            }
            (true, None) => format!("{} · {}", self.name, self.kind.pointer(self.number)),
        };
        if self.language == SourceLanguage::St {
            label.push_str(" · ST");
        }
        if !self.enabled {
            label.push_str(&format!(" ({})", tr("devre dışı")));
        }
        label
    }
}
/// All units joined into the one program the PLC runs, with the lines each unit's body
/// occupies (index 0 = Main; 1-based, inclusive; an empty body has `end < start`).
#[derive(Debug, Clone)]
pub struct Layout {
    pub il: String,
    pub ranges: Vec<(usize, usize)>,
    /// For an ST program: the ST source line of each of its IL lines (`None` for IL).
    pub maps: Vec<Option<Vec<usize>>>,
    /// Shared block bodies of ST programs, placed after `FEND`: program index, line range
    /// and the ST source line of each IL line.
    pub shared: Vec<(usize, (usize, usize), Vec<usize>)>,
    /// ST programs that failed to lower (program index, diagnostic in ST lines).
    pub errors: Vec<(usize, crate::plc::Diagnostic)>,
}
/// A compile error with the program (0 = Main) and the line of that program's own source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub program: usize,
    pub line: usize,
    pub message: String,
}
fn locate_in(layout: &Layout, line: usize) -> Option<(usize, usize)> {
    if let Some((index, (start, _), map)) = layout
        .shared
        .iter()
        .find(|(_, (start, end), _)| line >= *start && line <= *end)
    {
        return Some((*index, map.get(line - start).copied().unwrap_or(1)));
    }
    let (index, (start, _)) = layout
        .ranges
        .iter()
        .enumerate()
        .find(|(_, (start, end))| line >= *start && line <= *end)?;
    let local = line - start + 1;
    Some((
        index,
        match &layout.maps[index] {
            Some(map) => map.get(local - 1).copied().unwrap_or(local),
            None => local,
        },
    ))
}
fn st_unit_template(name: &str) -> String {
    format!("PROGRAM {name}\nEND_PROGRAM\n")
}
fn is_il(language: &SourceLanguage) -> bool {
    *language == SourceLanguage::Il
}
const MAX_UNITS: usize = 64;

/// Does IL source name the index registers E6 / E7 (`E6`, `D10E7`, `K4M0E6` …)?
fn uses_shared_index_registers(source: &str) -> bool {
    source
        .lines()
        .map(|l| l.split(';').next().unwrap_or(""))
        .flat_map(str::split_whitespace)
        .any(|word| {
            let w = word.to_ascii_uppercase();
            (w.ends_with("E6") || w.ends_with("E7"))
                && (w.len() == 2 || w.as_bytes()[w.len() - 3].is_ascii_digit())
        })
}

fn strip_end(source: &str) -> String {
    let mut lines: Vec<&str> = source.lines().collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if lines
        .last()
        .is_some_and(|l| l.trim().eq_ignore_ascii_case("END"))
    {
        lines.pop();
    }
    let mut body = lines.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    body
}
fn line_count(text: &str) -> usize {
    text.lines().count()
}
fn has_line(source: &str, mnemonic: &str) -> bool {
    source
        .lines()
        .any(|l| l.trim().eq_ignore_ascii_case(mnemonic))
}
/// `CALL SR_Pump` becomes `CALL P0`; the line count never changes.
fn resolve_calls(body: &str, names: &[(String, String)]) -> String {
    if names.is_empty() {
        return body.to_owned();
    }
    let mut out = String::with_capacity(body.len());
    for line in body.lines() {
        let mut tokens = line.split_whitespace();
        let replaced = match (tokens.next(), tokens.next(), tokens.next()) {
            (Some(op), Some(target), None)
                if op.eq_ignore_ascii_case("CALL") || op.eq_ignore_ascii_case("CALLP") =>
            {
                names
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(target))
                    .map(|(_, pointer)| {
                        let indent = &line[..line.len() - line.trim_start().len()];
                        format!("{indent}{op} {pointer}")
                    })
            }
            _ => None,
        };
        out.push_str(replaced.as_deref().unwrap_or(line));
        out.push('\n');
    }
    out
}
fn slice_program(whole: &crate::plc::Program, range: (usize, usize)) -> crate::plc::Program {
    use crate::plc::Flow;
    let (start, end) = range;
    let inside = |n: &crate::plc::Network| n.line >= start && n.end_line <= end;
    let first = whole.networks.iter().position(inside).unwrap_or(0);
    let count = whole.networks.iter().filter(|n| inside(n)).count();
    let mut view = whole.clone();
    view.networks = whole
        .networks
        .iter()
        .filter(|n| inside(n))
        .cloned()
        .map(|mut n| {
            n.line = n.line - start + 1;
            n.end_line = n.end_line - start + 1;
            n
        })
        .collect();
    let owned = |i: usize| i >= first && i < first + count;
    let anchors: Vec<usize> = whole
        .flow
        .iter()
        .enumerate()
        .filter(|(_, step)| matches!(step, Flow::Network(i) | Flow::Capture(i) if owned(*i)))
        .map(|(pos, _)| pos)
        .collect();
    let (low, high) = (anchors.first().copied(), anchors.last().copied());
    view.flow = whole
        .flow
        .iter()
        .enumerate()
        .filter_map(|(pos, step)| match step {
            Flow::Network(i) if owned(*i) => Some(Flow::Network(*i - first)),
            Flow::Capture(i) if owned(*i) => Some(Flow::Capture(*i - first)),
            Flow::Control { line, .. } if *line >= start && *line <= end => Some(step.clone()),
            Flow::Evaluate(_) | Flow::Restore(_)
                if low.is_some_and(|l| pos > l) && high.is_some_and(|h| pos < h) =>
            {
                Some(step.clone())
            }
            _ => None,
        })
        .collect();
    view
}
/// Is `name` usable as a program name (no clash check)?
pub fn check_unit_name(name: &str) -> bool {
    valid_unit_name(name) && !name.eq_ignore_ascii_case("main") && name.parse::<crate::plc::Device>().is_err()
}
fn valid_unit_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.len() <= 32
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A named device of the project (Delta editor: global symbol). IL operands and ST identifiers may
/// use the name instead of the address; the compiled object code only holds the address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub ty: crate::st::DataType,
    /// Canonical Delta device (`X0`, `D100`); `None` until the allocator assigns one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// The device was handed out by the allocator, not typed in by the user.
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto: bool,
    /// Keeps its value through power-off: the device must lie in a latched area.
    #[serde(default, skip_serializing_if = "is_false")]
    pub retain: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    /// Initial value; written to the PLC only when the download asks for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub init: Option<String>,
}
fn is_false(value: &bool) -> bool {
    !*value
}
fn alloc_is_default(alloc: &crate::alloc::Alloc) -> bool {
    *alloc == crate::alloc::Alloc::default()
}
fn retain_is_default(retain: &crate::alloc::Retain) -> bool {
    *retain == crate::alloc::Retain::default()
}
/// A symbol's initial value as words (one bit for BOOL: `words[0] != 0`), low word first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialValue {
    pub device: crate::plc::Device,
    pub bit: bool,
    pub words: Vec<u16>,
}
impl InitialValue {
    /// Map to Modbus addresses. A double word must not straddle the D4095/D4096 boundary,
    /// where the PLC's address map jumps.
    pub fn to_modbus(&self) -> Result<crate::plc_download::Initial, String> {
        use crate::{es2_bridge, plc::Device, plc_download::Initial};
        if self.bit {
            return Ok(Initial::Bit {
                address: es2_bridge::bit_address(self.device)?,
                on: self.words.first().is_some_and(|w| *w != 0),
            });
        }
        let address = es2_bridge::word_address(self.device)?;
        let last = Device {
            area: self.device.area,
            index: self.device.index + self.words.len() as u16 - 1,
        };
        if es2_bridge::word_address(last)? != address + self.words.len() as u16 - 1 {
            return Err(format!(
                "{} çift kelimesi D4095/D4096 sınırını aşıyor; başlangıç değeri yazılamaz",
                self.device
            ));
        }
        Ok(Initial::Words {
            address,
            words: self.words.clone(),
        })
    }
}
/// Parse an initial value for `ty`: BOOL `TRUE/FALSE/ON/OFF/1/0`; integers in decimal or
/// `H1F` / `0x1F`; REAL as a decimal number.
pub fn parse_initial(ty: crate::st::DataType, text: &str) -> Result<Vec<u16>, String> {
    use crate::st::DataType::*;
    let text = text.trim();
    let upper = text.to_ascii_uppercase();
    let number = |bits: u32, signed: bool| -> Result<i64, String> {
        let hex = upper
            .strip_prefix('H')
            .or_else(|| upper.strip_prefix("0X"))
            .or_else(|| upper.strip_prefix("16#"));
        let value = match hex {
            Some(digits) => i64::from_str_radix(digits, 16).ok(),
            None => text.parse::<i64>().ok(),
        }
        .ok_or_else(|| format!("\"{text}\" bir tamsayı olarak okunamadı"))?;
        let (low, high) = match (signed, hex.is_some()) {
            (true, false) => (-(1i64 << (bits - 1)), (1i64 << (bits - 1)) - 1),
            _ => (0, (1i64 << bits) - 1),
        };
        if value < low || value > high {
            return Err(format!("\"{text}\" {low}..{high} aralığının dışında"));
        }
        Ok(value)
    };
    Ok(match ty {
        Bool => match upper.as_str() {
            "TRUE" | "ON" | "1" => vec![1],
            "FALSE" | "OFF" | "0" => vec![0],
            _ => return Err(format!("BOOL başlangıç değeri TRUE/FALSE olmalı: \"{text}\"")),
        },
        Word => vec![number(16, false)? as u16],
        Int => vec![number(16, true)? as i16 as u16],
        Dword => {
            let v = number(32, false)? as u32;
            vec![v as u16, (v >> 16) as u16]
        }
        Dint => {
            let v = number(32, true)? as i32 as u32;
            vec![v as u16, (v >> 16) as u16]
        }
        Real => {
            let v: f32 = text
                .replace(',', ".")
                .parse()
                .map_err(|_| format!("\"{text}\" bir sayı olarak okunamadı"))?;
            if !v.is_finite() {
                return Err("REAL başlangıç değeri sonlu olmalı".into());
            }
            let bits = v.to_bits();
            vec![bits as u16, (bits >> 16) as u16]
        }
    })
}

/// Delta editor's symbol-name rules: at most 30 characters, no device or constant spelling
/// (`M0`, `K10`), no `* # ? \ % @` or blanks, no doubled or trailing underscore.
pub fn check_symbol_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let ok_start = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !ok_start
        || name.chars().count() > 30
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(
            "Sembol adı harf veya _ ile başlamalı, yalnız harf, rakam ve _ içermeli (en çok 30)"
                .into(),
        );
    }
    if name.contains("__") || name.ends_with('_') {
        return Err("Sembol adında art arda veya sondaki alt çizgi olamaz".into());
    }
    let upper = name.to_ascii_uppercase();
    let constant = upper
        .strip_prefix('K')
        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        || upper
            .strip_prefix('H')
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_hexdigit()));
    if name.parse::<crate::plc::Device>().is_ok() || constant {
        return Err(format!(
            "\"{name}\" cihaz ya da sabit gibi okunur; başına _ koyun (örn. _{name})"
        ));
    }
    Ok(())
}
/// Replace symbol names in operand position (never the mnemonic, never a comment) by their
/// device. Spacing and the line count are untouched, so diagnostics keep their lines.
fn resolve_symbols(il: &str, devices: &BTreeMap<String, String>) -> String {
    if devices.is_empty() {
        return il.to_owned();
    }
    let mut out = String::with_capacity(il.len());
    for line in il.lines() {
        let (code, comment) = line.split_at(line.find(';').unwrap_or(line.len()));
        let mut rebuilt = String::with_capacity(line.len());
        let mut rest = code;
        let mut token_index = 0;
        while !rest.is_empty() {
            let blanks = rest.len() - rest.trim_start().len();
            rebuilt.push_str(&rest[..blanks]);
            rest = &rest[blanks..];
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let (token, tail) = rest.split_at(end);
            match devices.get(&token.to_ascii_lowercase()) {
                Some(device) if token_index > 0 => rebuilt.push_str(device),
                _ => rebuilt.push_str(token),
            }
            if !token.is_empty() {
                token_index += 1;
            }
            rest = tail;
        }
        out.push_str(&rebuilt);
        out.push_str(comment);
        out.push('\n');
    }
    out
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    pub model: String,
    pub source: String,
    /// Older format_version1 projects omitted language and remain IL.
    #[serde(default)]
    pub language: SourceLanguage,
    pub watch: Vec<String>,
    /// Target PLC model id from `devices::MODELS`; projects saved before the model
    /// choice existed are ES2.
    #[serde(default = "default_device")]
    pub device: String,
    /// Right-side extension modules in slot order (names from `hardware::CATALOG`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hardware: Vec<String>,
    /// Device comments (symbol table): device such as `X0`, `D100` → text.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub comments: BTreeMap<String, String>,
    /// Extra program units behind Main (IL projects only); `source` stays the Main program.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<Unit>,
    /// Named devices usable by name in IL operands and ST identifiers.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub symbols: BTreeMap<String, Symbol>,
    /// Range the allocator draws ST locals, temporaries and automatic symbols from.
    #[serde(default, skip_serializing_if = "alloc_is_default")]
    pub alloc: crate::alloc::Alloc,
    /// Latched ranges for retained symbols and `VAR_RETAIN` variables.
    #[serde(default, skip_serializing_if = "retain_is_default")]
    pub retain: crate::alloc::Retain,
    /// Named parameter sets; empty for projects saved before recipes existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipes: Vec<crate::recipes::Recipe>,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            format_version: 1,
            name: "Motor kontrolü".into(),
            model: "DVP-ES2 / offline subset".into(),
            source: DEMO.into(),
            language: SourceLanguage::Il,
            watch: ["X0", "X1", "X2", "X3", "Y0", "Y1", "T0", "C0", "D0"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            comments: BTreeMap::new(),
            device: crate::devices::DEFAULT.into(),
            hardware: Vec::new(),
            units: Vec::new(),
            symbols: BTreeMap::new(),
            alloc: crate::alloc::Alloc::default(),
            retain: crate::alloc::Retain::default(),
            recipes: Vec::new(),
        }
    }
}
fn default_device() -> String {
    crate::devices::DEFAULT.into()
}
impl Project {
    /// Set (or with an empty text remove) the comment of `device`.
    pub fn set_comment(&mut self, device: crate::plc::Device, text: &str) {
        let text: String = text
            .replace(['\n', '\r'], " ")
            .trim()
            .chars()
            .take(256)
            .collect();
        if text.is_empty() {
            self.comments.remove(&device.to_string());
        } else {
            self.comments.insert(device.to_string(), text);
        }
    }
    /// Comments from another tool; entries whose device OpenDVP cannot address (for
    /// example AH-series `X0.0`) are skipped.
    pub fn import_comments(&mut self, comments: &[(String, String)]) {
        for (device, text) in comments {
            if let Ok(device) = device.trim().parse::<crate::plc::Device>() {
                self.set_comment(device, text);
            }
        }
    }
    pub fn comment(&self, device: crate::plc::Device) -> Option<&str> {
        self.comments
            .get(&device.to_string())
            .map(String::as_str)
            .or_else(|| {
                self.symbol_at(device)
                    .map(|(_, s)| s.comment.as_str())
                    .filter(|c| !c.is_empty())
            })
    }
    /// The ST lowering options: the project's allocation ranges and every symbol that has a
    /// device, visible to the ST body without a declaration.
    pub fn lower_options(&self) -> crate::st::LowerOptions {
        crate::st::LowerOptions {
            reserved: Vec::new(),
            label_start: 0,
            avoid_labels: Vec::new(),
            share: crate::st::Share::default(),
            alloc: self.alloc,
            retain: self.retain,
            globals: self
                .symbols
                .iter()
                .filter_map(|(name, s)| {
                    Some(crate::st::Global {
                        name: name.clone(),
                        data_type: s.ty,
                        address: s.device.clone()?,
                    })
                })
                .collect(),
        }
    }
    /// Lower-case symbol name → device, for the IL preprocessor.
    fn symbol_devices(&self) -> BTreeMap<String, String> {
        self.symbols
            .iter()
            .filter_map(|(name, s)| Some((name.to_ascii_lowercase(), s.device.clone()?)))
            .collect()
    }
    /// The symbol that owns `device`, if any (its comment stands in for a device comment).
    pub fn symbol_at(&self, device: crate::plc::Device) -> Option<(&str, &Symbol)> {
        let text = device.to_string();
        self.symbols
            .iter()
            .find(|(_, s)| s.device.as_deref() == Some(text.as_str()))
            .map(|(name, s)| (name.as_str(), s))
    }
    fn check_symbol(&self, name: &str, symbol: &Symbol) -> Result<(), String> {
        use crate::{plc::Area, st::DataType};
        check_symbol_name(name)?;
        if self
            .symbols
            .keys()
            .any(|other| other != name && other.eq_ignore_ascii_case(name))
        {
            return Err(format!("\"{name}\" adı büyük/küçük harf farkıyla zaten var"));
        }
        if let Some(text) = &symbol.init {
            parse_initial(symbol.ty, text).map_err(|e| format!("\"{name}\": {e}"))?;
        }
        if let Some(text) = &symbol.device {
            let device: crate::plc::Device = text.parse()?;
            if device.to_string() != *text {
                return Err(format!("Cihaz normalize değil: {text}"));
            }
            let bit = symbol.ty == DataType::Bool;
            if symbol.retain {
                let latched = if bit {
                    crate::alloc::Retain::bit_is_latched(device.index) && device.area == Area::M
                } else {
                    crate::alloc::Retain::word_is_latched(device.index, symbol.ty.words())
                };
                if !latched {
                    return Err(format!("{text} kalıcı (latch'li) bir alanda değil"));
                }
            }
            let fits = if bit {
                device.area != Area::D
            } else {
                device.area == Area::D
            };
            if !fits {
                return Err(if bit {
                    "BOOL sembolü bit cihazına (X, Y, M, S, T, C) bağlanmalı".into()
                } else {
                    "Sayısal sembol D cihazına bağlanmalı".into()
                });
            }
            let width = if bit { 1 } else { symbol.ty.words() as u16 };
            let span = |d: crate::plc::Device, w: u16| (d.area, d.index, d.index + w);
            let mine = span(device, width);
            for (other_name, other) in &self.symbols {
                if other_name == name {
                    continue;
                }
                let Some(other_device) = other.device.as_deref().and_then(|d| d.parse().ok())
                else {
                    continue;
                };
                let other_width = if other.ty == DataType::Bool {
                    1
                } else {
                    other.ty.words() as u16
                };
                let theirs = span(other_device, other_width);
                if mine.0 == theirs.0 && mine.1 < theirs.2 && theirs.1 < mine.2 {
                    return Err(format!("{text} cihazı \"{other_name}\" sembolüyle çakışıyor"));
                }
            }
        }
        Ok(())
    }
    /// Add or replace a symbol. A typed-in device makes it non-automatic.
    pub fn set_symbol(&mut self, name: &str, mut symbol: Symbol) -> Result<(), String> {
        if symbol.device.is_none() {
            symbol.auto = false;
        }
        self.check_symbol(name, &symbol)?;
        if self.symbols.len() >= 4096 && !self.symbols.contains_key(name) {
            return Err("Sembol sayısı 4096 sınırını aşıyor".into());
        }
        self.symbols.insert(name.to_owned(), symbol);
        Ok(())
    }
    /// The initial values of every symbol that has one, mapped to its device. Symbols
    /// without a device cannot be written (the compile assigns one first).
    pub fn initial_values(&self) -> Result<Vec<InitialValue>, String> {
        let mut values = Vec::new();
        for (name, symbol) in &self.symbols {
            let Some(text) = &symbol.init else { continue };
            let device = symbol
                .device
                .as_deref()
                .ok_or_else(|| format!("\"{name}\" sembolünün adresi yok"))?
                .parse()?;
            values.push(InitialValue {
                device,
                bit: symbol.ty == crate::st::DataType::Bool,
                words: parse_initial(symbol.ty, text).map_err(|e| format!("\"{name}\": {e}"))?,
            });
        }
        Ok(values)
    }
    pub fn remove_symbol(&mut self, name: &str) {
        self.symbols.remove(name);
    }
    /// Give every symbol without a device one from the allocation range. Returns whether
    /// anything was assigned. Devices spelled in the program text and the ones other symbols
    /// already hold are skipped.
    pub fn assign_symbol_addresses(&mut self) -> Result<bool, String> {
        use crate::{alloc::Allocator, st::DataType};
        if self.symbols.values().all(|s| s.device.is_some()) {
            return Ok(false);
        }
        let mut allocator = Allocator::with_retain(self.alloc, self.retain);
        allocator.reserve_text(&self.source);
        for unit in &self.units {
            allocator.reserve_text(&unit.source);
        }
        for symbol in self.symbols.values() {
            if let Some(device) = symbol.device.as_deref().and_then(|d| d.parse().ok()) {
                allocator.reserve(device, symbol.ty.words());
            }
        }
        for (name, symbol) in self.symbols.iter_mut().filter(|(_, s)| s.device.is_none()) {
            let device = match (symbol.ty == DataType::Bool, symbol.retain) {
                (true, false) => allocator.take_bit(),
                (true, true) => allocator.take_retain_bit(),
                (false, false) => allocator.take_words(symbol.ty.words()),
                (false, true) => allocator.take_retain_words(symbol.ty.words()),
            }
            .map_err(|e| format!("\"{name}\" için adres atanamadı: {e}"))?;
            symbol.device = Some(device.to_string());
            symbol.auto = true;
        }
        Ok(true)
    }
    /// Forget the automatically assigned addresses (Delta editor "Remove Address"); the next
    /// compile assigns them again.
    pub fn clear_auto_addresses(&mut self) {
        for symbol in self.symbols.values_mut().filter(|s| s.auto) {
            symbol.device = None;
            symbol.auto = false;
        }
    }
    pub fn set_alloc(&mut self, alloc: crate::alloc::Alloc) -> Result<(), String> {
        alloc.validate()?;
        self.alloc = alloc;
        Ok(())
    }
    /// Set the latched range retained symbols are drawn from (checked against the alloc range).
    pub fn set_retain(&mut self, retain: crate::alloc::Retain) -> Result<(), String> {
        retain.validate(&self.alloc)?;
        self.retain = retain;
        Ok(())
    }
    /// Split a flat IL program (Main … `FEND`, then `P<n>` … `SRET` and `I<nnn>` … `IRET`
    /// blocks, `END`) into Main plus units: what Delta editor does when it uploads object code.
    /// Joining the result with [`Project::layout`] gives the same program back. Anything not
    /// clearly structured (several FEND, a block without its return, an undocumented
    /// interrupt label, code behind FEND without a label) is refused with the reason.
    pub fn split_flat(il: &str) -> Result<(String, Vec<Unit>), String> {
        let lines: Vec<&str> = il.lines().collect();
        let code = |line: &str| line.split(';').next().unwrap_or("").trim().to_owned();
        let is = |line: &str, word: &str| code(line).eq_ignore_ascii_case(word);
        let fends: Vec<usize> = (0..lines.len()).filter(|i| is(lines[*i], "FEND")).collect();
        let [fend] = fends[..] else {
            return Err(if fends.is_empty() {
                "Programda FEND yok; ayrılacak alt program ya da kesme bulunamadı".into()
            } else {
                "Birden çok FEND var; programı otomatik ayırmak güvenli değil".into()
            });
        };
        let end = (fend + 1..lines.len())
            .rev()
            .find(|i| is(lines[*i], "END"))
            .ok_or("FEND'den sonra END bulunamadı")?;
        let pointer = |text: &str, prefix: char| -> Option<u16> {
            let digits = text.strip_prefix([prefix, prefix.to_ascii_lowercase()])?;
            (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                .then(|| digits.parse().ok())?
        };
        // P labels that are CALL targets start a subroutine; any other P label is a CJ target
        // inside a body. Interrupt labels always start a block.
        let mut called = std::collections::BTreeSet::new();
        for line in &lines {
            let mut tokens = code(line).split_whitespace().map(str::to_owned).collect::<Vec<_>>();
            if tokens.len() == 2 && ["CALL", "CALLP"].contains(&tokens[0].to_ascii_uppercase().as_str()) {
                called.extend(pointer(&tokens.remove(1), 'P'));
            }
        }
        struct Block {
            kind: UnitKind,
            number: u16,
            body: Vec<String>,
        }
        let mut blocks: Vec<Block> = Vec::new();
        let mut leading: Vec<String> = Vec::new();
        for line in &lines[fend + 1..end] {
            let text = code(line);
            let is_label = !text.contains(char::is_whitespace);
            let start = if is_label {
                pointer(&text, 'I')
                    .map(|n| (UnitKind::Interrupt, n))
                    .or_else(|| pointer(&text, 'P').filter(|n| called.contains(n)).map(|n| (UnitKind::Subroutine, n)))
            } else {
                None
            };
            match (start, blocks.last_mut()) {
                (Some((kind, number)), _) => {
                    if !kind.number_valid(number) {
                        return Err(format!("{} belgelenmiş bir kesme etiketi değil", kind.pointer(number)));
                    }
                    if blocks.iter().any(|b| b.kind == kind && b.number == number) {
                        return Err(format!("{} birden çok kez tanımlı", kind.pointer(number)));
                    }
                    blocks.push(Block { kind, number, body: Vec::new() });
                }
                (None, Some(block)) => block.body.push((*line).to_owned()),
                (None, None) if text.is_empty() => leading.push((*line).to_owned()),
                (None, None) => {
                    return Err("FEND'den sonra etiketsiz kod var; alt program sınırı belirsiz".into());
                }
            }
        }
        if blocks.is_empty() {
            return Err("FEND'den sonra alt program ya da kesme bloğu yok".into());
        }
        let mut units = Vec::new();
        for (i, mut block) in blocks.into_iter().enumerate() {
            if i == 0 {
                block.body.splice(0..0, leading.drain(..));
            }
            while block.body.last().is_some_and(|l| l.trim().is_empty()) {
                block.body.pop();
            }
            let closing = block.kind.closing();
            if !block.body.last().is_some_and(|l| is(l, closing)) {
                return Err(format!(
                    "{} bloğu {closing} ile bitmiyor; otomatik ayrılamaz",
                    block.kind.pointer(block.number)
                ));
            }
            block.body.pop();
            let mut source = block.body.join("\n");
            if !source.is_empty() {
                source.push('\n');
            }
            source.push_str("END\n");
            let name = match block.kind {
                UnitKind::Interrupt => format!("INT_{:03}", block.number),
                _ => format!("SR_{}", block.number),
            };
            units.push(Unit {
                name,
                kind: block.kind,
                number: block.number,
                source,
                enabled: true,
                language: SourceLanguage::Il,
            });
        }
        let mut main = lines[..fend].join("\n");
        while main.ends_with('\n') {
            main.pop();
        }
        if !main.is_empty() {
            main.push('\n');
        }
        main.push_str("END\n");
        Ok((main, units))
    }
    /// Can Main be split into programs? (IL, no units yet, one FEND with blocks behind it.)
    pub fn can_split(&self) -> bool {
        self.language == SourceLanguage::Il
            && self.units.is_empty()
            && Self::split_flat(&self.source).is_ok()
    }
    /// Turn a flat Main into Main plus units; returns how many units were created.
    pub fn split_main(&mut self) -> Result<usize, String> {
        if self.language != SourceLanguage::Il || !self.units.is_empty() {
            return Err("Yalnız ek programı olmayan IL projesi ayrılabilir".into());
        }
        let (main, units) = Self::split_flat(&self.source)?;
        let count = units.len();
        self.source = main;
        self.units = units;
        Ok(count)
    }
    /// Number of programs: Main plus the extra units.
    pub fn program_count(&self) -> usize {
        1 + self.units.len()
    }
    /// Display name of program `index` (0 = Main).
    pub fn program_name(&self, index: usize) -> &str {
        match index {
            0 => "Main",
            i => self.units.get(i - 1).map_or("?", |u| u.name.as_str()),
        }
    }
    /// Source of program `index` (0 = Main), END-terminated.
    pub fn program_source(&self, index: usize) -> &str {
        match index {
            0 => &self.source,
            i => self.units.get(i - 1).map_or("", |u| u.source.as_str()),
        }
    }
    pub fn set_program_source(&mut self, index: usize, source: String) {
        match index {
            0 => self.source = source,
            i => {
                if let Some(unit) = self.units.get_mut(i - 1) {
                    unit.source = source;
                }
            }
        }
    }
    fn layout_with(&self, replaced: Option<(usize, &str)>) -> Layout {
        let source_of = |index: usize| match replaced {
            Some((i, text)) if i == index => text,
            _ => self.program_source(index),
        };
        let devices = self.symbol_devices();
        let count = self.program_count();
        if self.units.is_empty() {
            let il = if self.language == SourceLanguage::Il {
                resolve_symbols(source_of(0), &devices)
            } else {
                source_of(0).to_owned()
            };
            let lines = line_count(&il);
            return Layout {
                il,
                ranges: vec![(1, lines)],
                maps: vec![None],
                shared: Vec::new(),
                errors: Vec::new(),
            };
        }
        // CALL by name resolves to every subroutine's pointer, enabled or not, so calling a
        // disabled one reports the missing label instead of an unknown operand.
        let names: Vec<(String, String)> = self
            .units
            .iter()
            .filter(|u| u.kind == UnitKind::Subroutine)
            .map(|u| (u.name.clone(), u.kind.pointer(u.number)))
            .collect();
        // A unit being edited is compiled even when it is disabled, so the editor can work on it.
        let active = |i: usize, unit: &Unit| {
            unit.enabled || replaced.is_some_and(|(r, _)| r == i + 1)
        };
        // Every program as IL, in program order. ST programs are lowered one after the other,
        // each placing its locals around the previous ones and numbering its labels after them.
        let mut options = self.lower_options();
        let mut taken: std::collections::BTreeSet<u16> = self.cj_pointers();
        taken.extend(
            self.units
                .iter()
                .filter(|u| u.kind == UnitKind::Subroutine)
                .map(|u| u.number),
        );
        options.avoid_labels = taken.into_iter().collect();
        options.label_start = 1;
        let mut maps: Vec<Option<Vec<usize>>> = vec![None; count];
        let mut errors = Vec::new();
        let mut pieces: Vec<String> = Vec::with_capacity(count);
        // Shared block bodies of each ST program (IL after its FEND, and its line map).
        let mut subroutines: Vec<(usize, String, Vec<usize>)> = Vec::new();
        // Shared bodies keep their instance bases in E6 / E7; an IL program using them
        // would change them under a shared body.
        let il_uses_e6_e7 = (0..count).any(|index| {
            self.program_language(index) == SourceLanguage::Il
                && uses_shared_index_registers(source_of(index))
        });
        for index in 0..count {
            let wanted = index == 0 || active(index - 1, &self.units[index - 1]);
            if !wanted || self.program_language(index) == SourceLanguage::Il {
                pieces.push(source_of(index).to_owned());
                continue;
            }
            // An interrupt could fire inside a shared body and change E6 / E7 under it.
            options.share = if il_uses_e6_e7
                || index > 0 && self.units[index - 1].kind == UnitKind::Interrupt
            {
                crate::st::Share::Never
            } else {
                crate::st::Share::default()
            };
            match crate::st::lower_with(source_of(index), &options) {
                Ok(lowered) => {
                    for symbol in lowered.symbols.iter().filter(|s| !s.explicitly_mapped) {
                        if let Ok(device) = symbol.address.parse::<crate::plc::Device>() {
                            let width = match symbol.array {
                                Some((lo, hi)) => (hi - lo + 1).max(1) as usize,
                                None => 1,
                            } * symbol.data_type.words().max(1);
                            if device.bit() {
                                for k in 0..width {
                                    options.reserved.push((
                                        crate::plc::Device {
                                            area: device.area,
                                            index: device.index + k as u16,
                                        },
                                        1,
                                    ));
                                }
                            } else {
                                options.reserved.push((device, width));
                            }
                        }
                    }
                    for device in &lowered.temporaries {
                        options.reserved.push((*device, 1));
                    }
                    options.label_start = lowered.next_label;
                    let lines: Vec<&str> = lowered.il.lines().collect();
                    let main = lowered.main_lines.min(lines.len());
                    // After the body: FEND, the shared bodies, END.
                    if lines.len() > main + 2 {
                        let body = lines[main + 1..lines.len() - 1].join("\n") + "\n";
                        let map = lowered.line_map[main + 1..lines.len() - 1].to_vec();
                        subroutines.push((index, body, map));
                    }
                    let mut map = lowered.line_map;
                    map.truncate(main);
                    maps[index] = Some(map);
                    pieces.push(lines[..main].join("\n") + "\n");
                }
                Err(diagnostic) => {
                    errors.push((index, diagnostic));
                    pieces.push("END\n".to_owned());
                }
            }
        }
        let main = resolve_calls(&strip_end(&pieces[0]), &names);
        let mut il = main.clone();
        // Ranges of units left out of the program are empty (1, 0).
        let mut ranges = vec![(1, 0); count];
        ranges[0] = (1, line_count(&main));
        for (i, unit) in self.units.iter().enumerate() {
            if unit.kind == UnitKind::Cyclic && active(i, unit) {
                let body = resolve_calls(&strip_end(&pieces[i + 1]), &names);
                let start = line_count(&il) + 1;
                il.push_str(&body);
                ranges[i + 1] = (start, start + line_count(&body) - 1);
            }
        }
        if !has_line(&main, "FEND") {
            il.push_str("FEND\n");
        }
        for (i, unit) in self.units.iter().enumerate() {
            if unit.kind.has_pointer() && active(i, unit) {
                il.push_str(&unit.kind.pointer(unit.number));
                il.push('\n');
                let body = resolve_calls(&strip_end(&pieces[i + 1]), &names);
                let start = line_count(&il) + 1;
                il.push_str(&body);
                ranges[i + 1] = (start, start + line_count(&body) - 1);
                il.push_str(unit.kind.closing());
                il.push('\n');
            }
        }
        let mut shared = Vec::new();
        for (index, body, map) in subroutines {
            let start = line_count(&il) + 1;
            il.push_str(&body);
            shared.push((index, (start, start + line_count(&body) - 1), map));
        }
        il.push_str("END\n");
        Layout {
            il: resolve_symbols(&il, &devices),
            ranges,
            maps,
            shared,
            errors,
        }
    }
    /// The program as the PLC runs it: Main, `FEND`, then every unit behind its label.
    pub fn layout(&self) -> Layout {
        self.layout_with(None)
    }
    /// Language of program `index` (0 = Main).
    pub fn program_language(&self, index: usize) -> SourceLanguage {
        match index {
            0 => self.language,
            i => self.units.get(i - 1).map_or(SourceLanguage::Il, |u| u.language),
        }
    }
    /// Which program, and which source line inside it, a line of the joined program belongs
    /// to (an ST program's IL line is mapped back to its ST line); `None` for the generated
    /// FEND / label / SRET / END lines.
    pub fn locate(&self, line: usize) -> Option<(usize, usize)> {
        locate_in(&self.layout(), line)
    }
    /// Compile and report an error with the program and source line it belongs to.
    pub fn compile_located(&self) -> Result<crate::plc::Program, Located> {
        let at_main = |d: crate::plc::Diagnostic| Located {
            program: 0,
            line: d.line,
            message: d.message,
        };
        if self.units.is_empty() {
            return match self.language {
                SourceLanguage::Il => crate::plc::compile_il(&self.layout().il),
                SourceLanguage::St => {
                    crate::plc::compile_st_with(&self.source, &self.lower_options())
                }
            }
            .map_err(at_main);
        }
        if self.language == SourceLanguage::Il
            && self.units.iter().any(|u| u.kind == UnitKind::Cyclic && u.enabled)
            && let Some(line) = self
                .source
                .lines()
                .position(|l| l.trim().eq_ignore_ascii_case("FEND"))
        {
            return Err(Located {
                program: 0,
                line: line + 1,
                message: "Ana program parçaları varken Main'de FEND kullanılamaz".into(),
            });
        }
        let layout = self.layout();
        if let Some((program, diagnostic)) = layout.errors.first() {
            return Err(Located {
                program: *program,
                line: diagnostic.line,
                message: diagnostic.message.clone(),
            });
        }
        crate::plc::compile_il(&layout.il).map_err(|d| match locate_in(&layout, d.line) {
            Some((program, line)) => Located {
                program,
                line,
                message: d.message,
            },
            None => at_main(d),
        })
    }
    pub fn compile(&self) -> Result<crate::plc::Program, crate::plc::Diagnostic> {
        self.compile_located().map_err(|l| crate::plc::Diagnostic {
            line: l.line,
            message: l.message,
        })
    }
    pub fn instruction_list(&self) -> Result<String, crate::plc::Diagnostic> {
        if !self.units.is_empty() {
            let layout = self.layout();
            return match layout.errors.into_iter().next() {
                Some((_, diagnostic)) => Err(diagnostic),
                None => Ok(layout.il),
            };
        }
        match self.language {
            SourceLanguage::Il => Ok(self.layout().il),
            SourceLanguage::St => {
                crate::st::lower_with(&self.source, &self.lower_options()).map(|lowered| lowered.il)
            }
        }
    }
    /// Switch an unused unit between IL and ST; only an empty program can change language
    /// (its text would otherwise stop making sense).
    pub fn set_unit_language(
        &mut self,
        index: usize,
        language: SourceLanguage,
    ) -> Result<(), String> {
        if index == 0 || index > self.units.len() {
            return Err("Main'in dili proje ayarından gelir.".into());
        }
        let unit = &mut self.units[index - 1];
        if unit.language == language {
            return Ok(());
        }
        let pristine = match unit.language {
            SourceLanguage::Il => unit.source.trim().eq_ignore_ascii_case("END"),
            SourceLanguage::St => unit.source == st_unit_template(&unit.name),
        };
        if !pristine {
            return Err("Dili değiştirmek için önce programın içeriğini boşaltın.".into());
        }
        unit.language = language;
        unit.source = match language {
            SourceLanguage::Il => "END\n".to_owned(),
            SourceLanguage::St => st_unit_template(&unit.name),
        };
        Ok(())
    }
    /// The networks of program `index` out of the compiled joined program, numbered and
    /// positioned as in that program's own source. Control flow outside its body does not
    /// count against it, so a subroutine is ladder-editable on its own.
    pub fn program_view(&self, whole: &crate::plc::Program, index: usize) -> crate::plc::Program {
        let index = index.min(self.units.len());
        if index > 0 && !self.units[index - 1].enabled {
            // Not part of the compiled program: show its own networks, if it compiles in place.
            if let Ok(view) = self.compile_program_text(index, self.program_source(index)) {
                return view;
            }
        }
        slice_program(whole, self.layout().ranges[index])
    }
    /// Compile the joined program with the source of `index` replaced by `text` and return
    /// that program's own view; the graphical editor validates its candidates with this.
    pub fn compile_program_text(
        &self,
        index: usize,
        text: &str,
    ) -> Result<crate::plc::Program, crate::plc::Diagnostic> {
        let layout = self.layout_with(Some((index, text)));
        let whole = crate::plc::compile_il(&layout.il)?;
        Ok(slice_program(
            &whole,
            layout.ranges[index.min(self.units.len())],
        ))
    }
    /// Add a unit and return its program index. Subroutines take the lowest free `P`,
    /// interrupts the lowest free `I` from 1.
    pub fn add_unit(&mut self, kind: UnitKind) -> Result<usize, String> {
        if self.units.len() >= MAX_UNITS {
            return Err(format!("En çok {MAX_UNITS} ek program eklenebilir."));
        }
        if kind == UnitKind::Cyclic
            && self.language == SourceLanguage::Il
            && self
                .program_source(0)
                .lines()
                .any(|l| l.trim().eq_ignore_ascii_case("FEND"))
        {
            return Err(
                "Main programında FEND var; ana program parçaları FEND'den önce çalışır, önce FEND'i kaldırın."
                    .into(),
            );
        }
        // I001 first, then the other labels; P numbers must not clash with CJ pointers.
        let cj = self.cj_pointers();
        let first = if kind == UnitKind::Interrupt { 1 } else { 0 };
        let number = std::iter::once(first)
            .chain(0..=999)
            .find(|n| {
                kind.number_valid(*n)
                    && !(kind.has_pointer()
                        && self.units.iter().any(|u| u.kind == kind && u.number == *n))
                    && !(kind == UnitKind::Subroutine && cj.contains(n))
            })
            .ok_or("Boş işaretçi kalmadı.")?;
        let prefix = match kind {
            UnitKind::Subroutine => "SR",
            UnitKind::Interrupt => "INT",
            UnitKind::Cyclic => "PRG",
        };
        let name = (1..)
            .map(|n| format!("{prefix}_{n}"))
            .find(|name| !self.units.iter().any(|u| u.name.eq_ignore_ascii_case(name)))
            .unwrap_or_default();
        self.units.push(Unit {
            name,
            kind,
            number,
            source: "END\n".into(),
            enabled: true,
            language: SourceLanguage::Il,
        });
        Ok(self.units.len())
    }
    /// Enable or disable program `index` (Main cannot be disabled).
    pub fn set_unit_enabled(&mut self, index: usize, enabled: bool) {
        if index >= 1 && index <= self.units.len() {
            self.units[index - 1].enabled = enabled;
        }
    }
    /// Move a unit one place up or down the list; returns its new program index.
    pub fn move_unit(&mut self, index: usize, up: bool) -> Option<usize> {
        let at = index.checked_sub(1)?;
        let to = if up { at.checked_sub(1)? } else { at + 1 };
        if at >= self.units.len() || to >= self.units.len() {
            return None;
        }
        self.units.swap(at, to);
        Some(to + 1)
    }
    /// `P` numbers the programs use for CJ (a label line or a CJ operand). The manual
    /// forbids reusing them as CALL pointers; a CALL operand is only a reference.
    pub(crate) fn cj_pointer_numbers(&self) -> std::collections::BTreeSet<u16> {
        self.cj_pointers()
    }
    fn cj_pointers(&self) -> std::collections::BTreeSet<u16> {
        let number = |token: &str| {
            let digits = token.strip_prefix(['P', 'p'])?;
            digits.parse::<u16>().ok()
        };
        let mut found = std::collections::BTreeSet::new();
        for index in 0..self.program_count() {
            for line in self.program_source(index).lines() {
                let code = line.split(';').next().unwrap_or("");
                let mut tokens = code.split_whitespace();
                let (first, second) = (tokens.next(), tokens.next());
                match (first, second) {
                    (Some(label), None) => found.extend(number(label)),
                    (Some(op), Some(target))
                        if op.eq_ignore_ascii_case("CJ") || op.eq_ignore_ascii_case("CJP") =>
                    {
                        found.extend(number(target));
                    }
                    _ => {}
                }
            }
        }
        found
    }
    pub fn remove_unit(&mut self, index: usize) {
        if index >= 1 && index <= self.units.len() {
            self.units.remove(index - 1);
        }
    }
    pub fn rename_unit(&mut self, index: usize, name: &str) -> Result<(), String> {
        let name = name.trim();
        if index == 0 || index > self.units.len() {
            return Err("Main programı yeniden adlandırılamaz.".into());
        }
        if !valid_unit_name(name) {
            return Err(
                "Ad harf veya _ ile başlamalı; yalnız harf, rakam ve _ içermeli (en çok 32).".into(),
            );
        }
        if name.eq_ignore_ascii_case("main")
            || name.parse::<crate::plc::Device>().is_ok()
            || self
                .units
                .iter()
                .enumerate()
                .any(|(i, u)| i + 1 != index && u.name.eq_ignore_ascii_case(name))
        {
            return Err(format!("\"{name}\" adı kullanılamaz."));
        }
        self.units[index - 1].name = name.to_owned();
        Ok(())
    }
    pub fn set_unit_number(&mut self, index: usize, number: u16) -> Result<(), String> {
        if index == 0 || index > self.units.len() {
            return Err("Main programının işaretçisi yoktur.".into());
        }
        let kind = self.units[index - 1].kind;
        if !kind.has_pointer() {
            return Err("Ana program parçasının işaretçisi yoktur.".into());
        }
        if !kind.number_valid(number) {
            return Err(match kind {
                UnitKind::Subroutine => "Alt program işaretçisi P0..P255 olmalı.".into(),
                UnitKind::Interrupt => format!(
                    "{} belgelenmiş bir kesme etiketi değil (I001, I101…, I602…, I010…, I140…).",
                    kind.pointer(number)
                ),
                UnitKind::Cyclic => String::new(),
            });
        }
        if kind == UnitKind::Subroutine && self.cj_pointers().contains(&number) {
            return Err(format!(
                "{} bir CJ işaretçisi olarak kullanılıyor; CALL ile aynı numara olamaz.",
                kind.pointer(number)
            ));
        }
        if self
            .units
            .iter()
            .enumerate()
            .any(|(i, u)| i + 1 != index && u.kind == kind && u.number == number)
        {
            return Err(format!("{} zaten kullanılıyor.", kind.pointer(number)));
        }
        self.units[index - 1].number = number;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format_version != 1 {
            return Err(format!(
                "Desteklenmeyen proje sürümü: {}",
                self.format_version
            ));
        }
        if self.model != "DVP-ES2 / offline subset" {
            return Err(format!("Desteklenmeyen model profili: {}", self.model));
        }
        if self.name.len() > 200 || self.source.len() > 128 * 1024 || self.watch.len() > 64 {
            return Err("Proje boyut sınırlarını aşıyor".into());
        }
        for w in &self.watch {
            w.parse::<crate::plc::Device>()?;
        }
        if !self.units.is_empty() {
            if self.units.len() > MAX_UNITS {
                return Err("En çok 64 ek program olabilir".into());
            }
            for (i, unit) in self.units.iter().enumerate() {
                if !valid_unit_name(&unit.name)
                    || unit.name.eq_ignore_ascii_case("main")
                    || !unit.kind.number_valid(unit.number)
                    || unit.source.len() > 128 * 1024
                {
                    return Err(format!("Geçersiz ek program: {}", unit.name));
                }
                if self.units[..i].iter().any(|o| {
                    o.name.eq_ignore_ascii_case(&unit.name)
                        || (unit.kind.has_pointer()
                            && o.kind == unit.kind
                            && o.number == unit.number)
                }) {
                    return Err(format!(
                        "Ek program adı veya işaretçisi yineleniyor: {}",
                        unit.name
                    ));
                }
            }
        }
        self.alloc.validate()?;
        if !retain_is_default(&self.retain) {
            self.retain.validate(&self.alloc)?;
        }
        if self.symbols.len() > 4096 {
            return Err("Sembol sayısı 4096 sınırını aşıyor".into());
        }
        for (name, symbol) in &self.symbols {
            self.check_symbol(name, symbol)?;
        }
        if crate::devices::find(&self.device).is_none() {
            return Err(format!("Bilinmeyen PLC modeli: {}", self.device));
        }
        crate::hardware::assign(&self.hardware)?;
        crate::recipes::validate(&self.recipes)?;
        if self.comments.len() > 8192 {
            return Err("Cihaz açıklaması sayısı 8192 sınırını aşıyor".into());
        }
        for (device, text) in &self.comments {
            let parsed = device.parse::<crate::plc::Device>()?;
            if parsed.to_string() != *device {
                return Err(format!(
                    "Cihaz açıklaması anahtarı normalize değil: {device}"
                ));
            }
            if text.chars().count() > 256 || text.contains(['\n', '\r']) {
                return Err(format!(
                    "{device} açıklaması tek satır ve en çok 256 karakter olmalı"
                ));
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        if fs::metadata(path).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
            return Err("Dosya 1 MiB sınırını aşıyor".into());
        }
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        let name: String = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into();
        if extension.eq_ignore_ascii_case("dvz") {
            // EasyDownload package: only the program image is imported; register sections,
            // comments and the original source are not part of an IL project.
            let package = crate::dvz::read(&fs::read(path).map_err(|e| e.to_string())?)?;
            let source = crate::es2_program_decode::decode(&package.program)
                .map_err(|e| format!("DVZ programı IL'e çözülemedi: {e}"))?;
            let mut project = Self {
                name,
                source,
                language: SourceLanguage::Il,
                watch: Vec::new(),
                ..Self::default()
            };
            // An uploaded program is flat; split its subroutines and interrupts into programs.
            let _ = project.split_main();
            project.validate()?;
            return Ok(project);
        }
        if extension.eq_ignore_ascii_case("dvp") {
            // Delta editor program: the program image becomes IL; device comments go to the
            // project's comment table.
            let file = crate::dvp_file::read(&fs::read(path).map_err(|e| e.to_string())?)?;
            // Preserve the file's target instead of inheriting Default's ES2.
            // Unknown/unsupported targets must not silently become another PLC.
            let device = file
                .model_name()
                .and_then(crate::devices::find)
                .ok_or_else(|| {
                    format!(
                        "DVP dosyasının PLC modeli desteklenmiyor: {} (kod 0x{:04X})",
                        file.model_name().unwrap_or("bilinmeyen"),
                        file.model_code
                    )
                })?
                .id
                .to_owned();
            // The editor saves its own instruction form. Files written by `export_dvp`
            // carry PLC memory words (es2_encoder), which that decoder rejects
            // (`xxF0` blocks, 0x84/0x84 bit tags, 0x80 TMR targets), so they fall back.
            let source = crate::dvp_decode::decode(&file.program)
                .or_else(|editor_error| {
                    crate::es2_program_decode::decode(&file.program).map_err(|_| editor_error)
                })
                .map_err(|e| format!("Delta editor programı IL'e çözülemedi: {e}"))?;
            let mut project = Self {
                name,
                source,
                device,
                language: SourceLanguage::Il,
                watch: Vec::new(),
                ..Self::default()
            };
            project.import_comments(&file.device_comments);
            let _ = project.split_main();
            project.validate()?;
            return Ok(project);
        }
        if extension.eq_ignore_ascii_case("isp") {
            // Delta editor project: IL and LD POUs become one IL program, or an ST-only project
            // one ST program. FBD/SFC POUs are listed as notes at the top.
            let imported = crate::isp_file::import(&fs::read(path).map_err(|e| e.to_string())?)?;
            let mut source = String::new();
            for note in &imported.notes {
                source.push_str(&match imported.language {
                    SourceLanguage::Il => format!("; İçe aktarma notu: {note}\n"),
                    SourceLanguage::St => format!("(* İçe aktarma notu: {note} *)\n"),
                });
            }
            if !imported.notes.is_empty() {
                // Keep the notes apart from the first network's own comment.
                source.push('\n');
            }
            source.push_str(&imported.source);
            let mut project = Self {
                name,
                source,
                language: imported.language,
                watch: Vec::new(),
                ..Self::default()
            };
            project.import_comments(&imported.device_comments);
            project.validate()?;
            return Ok(project);
        }
        let source = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let project =
            if extension.eq_ignore_ascii_case("il") || extension.eq_ignore_ascii_case("st") {
                Self {
                    name: path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                    source,
                    language: if extension.eq_ignore_ascii_case("st") {
                        SourceLanguage::St
                    } else {
                        SourceLanguage::Il
                    },
                    watch: Vec::new(),
                    ..Self::default()
                }
            } else {
                serde_json::from_str(&source).map_err(|e| format!("OpenDVP JSON okunamadı: {e}"))?
            };
        project.validate()?;
        Ok(project)
    }
    /// Write the compiled program and the device comments as a Delta editor `.dvp` file
    /// (ES2 only). Returns how many comments the format could not carry and were left out.
    pub fn export_dvp(&self, path: &Path) -> Result<usize, String> {
        if self.device != "ES2" {
            return Err("Delta editor .dvp dışa aktarma şu an yalnız DVP-ES2 projeleri için".into());
        }
        let program = self.compile().map_err(|e| e.to_string())?;
        let words = crate::es2_encoder::encode(&program)?.words;
        let mut devices: std::collections::BTreeSet<String> = self.comments.keys().cloned().collect();
        devices.extend(self.symbols.values().filter(|s| !s.comment.is_empty()).filter_map(|s| s.device.clone()));
        let mut comments = Vec::new();
        let mut skipped = 0;
        for device in devices {
            let Some(text) = device.parse::<crate::plc::Device>().ok().and_then(|d| self.comment(d)) else {
                continue;
            };
            if crate::dvp_file::comment_line(&device, text).is_ok() {
                comments.push((device, text.to_owned()));
            } else {
                skipped += 1;
            }
        }
        // Delta editor titles are 20 characters of ANSI text.
        let title: String = self
            .name
            .chars()
            .filter(|c| !c.is_control())
            .take(20)
            .collect();
        let bytes = crate::dvp_file::write(&words, &title, &comments)
            .or_else(|_| crate::dvp_file::write(&words, "", &comments))?;
        write_atomic(path, &bytes)?;
        Ok(skipped)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("il") || e.eq_ignore_ascii_case("st"))
        {
            return Err(
                "Proje metaverisi için .odvp kullanın; .st/.il kaynağını Dışa aktar ile kaydedin"
                    .into(),
            );
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if json.len() > 1024 * 1024 {
            return Err("Proje dosyası 1 MiB sınırını aşıyor; reçete veya program boyutunu azaltın.".into());
        }
        write_atomic(path, json.as_bytes())
    }
    /// Source export never changes project metadata or substitutes generated IL
    /// for the canonical ST editor text. Project-save remains a separate action.
    pub fn export_source(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("dvp") {
            return self.export_dvp(path).map(|_| ());
        }
        let source = if extension.eq_ignore_ascii_case(self.language.extension()) {
            // An IL export is the whole joined program, not just Main.
            match self.language {
                SourceLanguage::Il => self.layout().il,
                SourceLanguage::St => self.source.clone(),
            }
        } else if extension.eq_ignore_ascii_case("il") {
            self.instruction_list().map_err(|e| e.to_string())?
        } else {
            return Err(
                "Kaynak uzantısı seçili dile göre .st veya .il olmalı; IL doğrudan ST'ye çevrilmez"
                    .into(),
            );
        };
        write_atomic(path, source.as_bytes())
    }
}
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    // Same-directory staging avoids leaving a partially written project on failure.
    let staging = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .map_err(|e| format!("Geçici dosya oluşturulamadı: {e}"))?;
    use std::io::Write;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&staging, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&staging);
        return Err(format!("Proje kaydedilemedi: {e}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn demo_compiles() {
        let p = Project::default();
        p.validate().unwrap();
        assert_eq!(crate::plc::compile(&p.source).unwrap().networks.len(), 6);
    }
    #[test]
    fn project_roundtrip_and_version_guard() {
        let p = Project::default();
        let json = serde_json::to_string(&p).unwrap();
        let mut q: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(q.source, p.source);
        q.format_version = 2;
        assert!(q.validate().is_err());
        q.format_version = 1;
        q.model = "EH3".into();
        assert!(q.validate().is_err());
    }
    #[test]
    fn old_project_without_language_defaults_to_il_and_st_metadata_roundtrips() {
        let mut legacy = serde_json::to_value(Project::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("language");
        let legacy: Project = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.language, SourceLanguage::Il);
        let p = Project {
            language: SourceLanguage::St,
            source: ST_TEMPLATE.into(),
            ..Project::default()
        };
        let q: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(q.language, SourceLanguage::St);
        assert_eq!(q.source, ST_TEMPLATE);
    }
    #[test]
    fn dvz_package_imports_program_as_il() {
        let dir = std::env::temp_dir().join(format!("opendvp-dvz-project-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Hat 3.dvz");
        fs::write(&path, crate::dvz::write(8, &[0x0024, 0x00c5, 0x0f00])).unwrap();
        let project = Project::load(&path).unwrap();
        assert_eq!(project.name, "Hat 3");
        assert_eq!(project.language, SourceLanguage::Il);
        assert_eq!(
            project.source.split_whitespace().collect::<Vec<_>>(),
            ["LD", "X0", "OUT", "Y0", "END"]
        );
        fs::write(&path, b"not a package").unwrap();
        assert!(Project::load(&path).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn editor_device_comments_land_in_the_comment_table_and_survive_save() {
        let dir = std::env::temp_dir().join(format!("opendvp-dvp-project-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("es2.dvp");
        fs::write(
            &path,
            &fixture!("isp-dvp-formats/dvp-samples/editor_resaved_es2.dvp"),
        )
        .unwrap();
        let project = Project::load(&path).unwrap();
        assert_eq!(project.comment("X0".parse().unwrap()), Some("start"));
        assert_eq!(project.comment("Y0".parse().unwrap()), Some("lamp"));
        // Comments are no longer IL lines, so the network has no stray title.
        let program = project.compile().unwrap();
        assert_eq!(program.networks[0].comment, "");
        let saved = dir.join("es2.odvp");
        project.save(&saved).unwrap();
        assert_eq!(Project::load(&saved).unwrap().comments, project.comments);
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn hardware_configuration_is_saved_and_validated() {
        let dir = std::env::temp_dir().join(format!("opendvp-hw-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hw.odvp");
        let project = Project {
            hardware: vec!["DVP16SP11R".into(), "DVP06XA-S2".into()],
            ..Project::default()
        };
        project.save(&path).unwrap();
        assert_eq!(Project::load(&path).unwrap().hardware, project.hardware);
        let bad = Project {
            hardware: vec!["DVP99ZZ".into()],
            ..Project::default()
        };
        assert!(bad.validate().is_err());
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn device_model_is_saved_and_old_projects_default_to_es2() {
        let dir = std::env::temp_dir().join(format!("opendvp-device-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ss2.odvp");
        let project = Project {
            device: "SS2".into(),
            ..Project::default()
        };
        project.save(&path).unwrap();
        assert_eq!(Project::load(&path).unwrap().device, "SS2");
        // A project file written before the model field existed.
        let mut json: serde_json::Value = serde_json::to_value(Project::default()).unwrap();
        json.as_object_mut().unwrap().remove("device");
        fs::write(&path, json.to_string()).unwrap();
        assert_eq!(
            Project::load(&path).unwrap().device,
            crate::devices::DEFAULT
        );
        let bad = Project {
            device: "FX3U".into(),
            ..Project::default()
        };
        assert!(bad.validate().is_err());
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn comment_table_is_normalized_and_validated() {
        let mut p = Project::default();
        p.set_comment("D100".parse().unwrap(), "  Hız\nayarı ");
        assert_eq!(p.comment("D100".parse().unwrap()), Some("Hız ayarı"));
        p.set_comment("D100".parse().unwrap(), "");
        assert!(p.comments.is_empty());
        // Old projects without the field load; empty tables are not written.
        let json = serde_json::to_string(&Project::default()).unwrap();
        assert!(!json.contains("comments"));
        let mut bad = Project::default();
        bad.comments.insert("Q9".into(), "x".into());
        assert!(bad.validate().is_err());
    }
    #[test]
    fn consecutive_comment_lines_form_title_and_description() {
        let p = crate::plc::compile(
            "; dosya başlığı\n\n; Pompa\n; seviye düşükse çalışır\nLD X0\nOUT Y0\nEND\n",
        )
        .unwrap();
        assert_eq!(p.networks[0].comment, "Pompa\nseviye düşükse çalışır");
    }
    #[test]
    fn st_import_and_separate_source_exports_preserve_canonical_source() {
        let dir = std::env::temp_dir().join(format!("opendvp-st-project-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("motor.ST");
        fs::write(&path, ST_TEMPLATE).unwrap();
        let p = Project::load(&path).unwrap();
        assert_eq!(p.language, SourceLanguage::St);
        assert_eq!(p.source, ST_TEMPLATE);
        let json = dir.join("project.odvp");
        p.save(&json).unwrap();
        assert_eq!(Project::load(&json).unwrap().source, ST_TEMPLATE);
        let raw = dir.join("source.st");
        p.export_source(&raw).unwrap();
        assert_eq!(fs::read_to_string(&raw).unwrap(), ST_TEMPLATE);
        let il = dir.join("generated.il");
        p.export_source(&il).unwrap();
        let generated = Project::load(&il).unwrap();
        assert_eq!(generated.language, SourceLanguage::Il);
        assert!(crate::plc::compile_il(&generated.source).is_ok());
        assert_eq!(p.source, ST_TEMPLATE);
        assert!(
            p.save(&raw).is_err(),
            "project JSON must not overwrite a source file"
        );
        for file in [path, json, raw, il] {
            fs::remove_file(file).unwrap();
        }
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn save_replaces_existing_file_and_loads() {
        let dir = std::env::temp_dir().join(format!("opendvp-project-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("project.odvp");
        let mut p = Project::default();
        p.save(&path).unwrap();
        p.name = "Güncel".into();
        p.save(&path).unwrap();
        assert_eq!(Project::load(&path).unwrap().name, "Güncel");
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&dir).unwrap();
    }

    fn with_units() -> Project {
        let mut p = Project {
            source: "LD X0\nCALL SR_1\nLD X1\nOUT Y0\nEND\n".into(),
            ..Project::default()
        };
        let sub = p.add_unit(UnitKind::Subroutine).unwrap();
        p.set_program_source(sub, "; pump\nLD M1000\nOUT Y1\nEND\n".into());
        let int = p.add_unit(UnitKind::Interrupt).unwrap();
        p.set_program_source(int, "LD M1000\nINC D10\nEND\n".into());
        p
    }
    #[test]
    fn units_join_behind_fend_with_labels_and_returns() {
        let p = with_units();
        p.validate().unwrap();
        let layout = p.layout();
        assert_eq!(
            layout.il,
            "LD X0\nCALL P0\nLD X1\nOUT Y0\nFEND\nP0\n; pump\nLD M1000\nOUT Y1\nSRET\nI001\nLD M1000\nINC D10\nIRET\nEND\n"
        );
        assert_eq!(layout.ranges, vec![(1, 4), (7, 9), (12, 13)]);
        assert!(p.compile().is_ok(), "{:?}", p.compile().err());
        assert_eq!(p.locate(8), Some((1, 2)));
        assert_eq!(p.locate(5), None, "FEND is generated");
    }
    #[test]
    fn project_without_units_is_unchanged() {
        let p = Project::default();
        assert_eq!(p.layout().il, p.source);
        assert_eq!(p.program_count(), 1);
    }
    #[test]
    fn each_program_gets_its_own_view_of_the_networks() {
        let p = with_units();
        let whole = p.compile().unwrap();
        let main = p.program_view(&whole, 0);
        let sub = p.program_view(&whole, 1);
        let int = p.program_view(&whole, 2);
        assert_eq!(main.networks.len(), 1, "LD X0 + CALL is control flow, not a network");
        assert_eq!(sub.networks.len(), 1);
        assert_eq!(int.networks.len(), 1);
        // Lines are relative to the program's own source (comment line 1, LD on line 2).
        assert_eq!(sub.networks[0].line, 2);
        assert_eq!(sub.networks[0].comment, "pump");
        assert!(whole.has_control());
        assert!(!sub.has_control(), "the subroutine body has no control flow of its own");
        assert!(sub.graphically_editable());
        assert!(!main.graphically_editable(), "Main contains CALL");
    }
    #[test]
    fn candidate_text_is_validated_inside_the_joined_program() {
        let p = with_units();
        let edited = p
            .compile_program_text(1, "LD M1000\nOUT Y1\n\nLD M1001\nOUT Y2\nEND\n")
            .unwrap();
        assert_eq!(edited.networks.len(), 2);
        assert!(p.compile_program_text(1, "LD M1000\nBOGUS Y1\nEND\n").is_err());
    }
    #[test]
    fn il_export_contains_every_program() {
        let p = with_units();
        let path = std::env::temp_dir().join("opendvp-units-export.il");
        p.export_source(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(text, p.layout().il);
        assert!(text.contains("FEND
P0
"));
    }
    #[test]
    fn units_survive_save_and_old_projects_load() {
        let p = with_units();
        let json = serde_json::to_string(&p).unwrap();
        let q: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(q.units, p.units);
        let plain = serde_json::to_string(&Project::default()).unwrap();
        assert!(!plain.contains("units"), "no units key when there are none");
        let old: Project = serde_json::from_str(&plain).unwrap();
        assert!(old.units.is_empty());
    }
    #[test]
    fn unit_names_and_numbers_are_checked() {
        let mut p = with_units();
        assert!(p.rename_unit(1, "SR_Pump").is_ok());
        assert!(p.rename_unit(1, "bad name").is_err());
        assert!(p.rename_unit(1, "Main").is_err());
        assert!(p.rename_unit(1, "X0").is_err(), "a device name is not a program name");
        assert!(p.rename_unit(0, "Other").is_err());
        let second = p.add_unit(UnitKind::Subroutine).unwrap();
        assert_eq!(p.units[second - 1].number, 1, "lowest free pointer");
        assert!(p.rename_unit(second, "sr_pump").is_err(), "names are unique ignoring case");
        assert!(p.set_unit_number(second, 0).is_err(), "P0 is taken");
        assert!(p.set_unit_number(second, 7).is_ok());
        assert!(p.set_unit_number(2, 2).is_err(), "I002 is not a documented label");
        assert!(p.set_unit_number(2, 101).is_ok(), "I101 = X1 rising edge");
        assert!(p.set_unit_number(second, 256).is_err(), "pointers stop at P255");
    }
    #[test]
    fn call_by_name_resolves_and_renaming_keeps_it_working() {
        let mut p = with_units();
        p.rename_unit(1, "Pump").unwrap();
        p.source = "LD X0\nCALL Pump\nEND\n".into();
        assert!(p.layout().il.starts_with("LD X0\nCALL P0\nFEND\nP0\n"));
        assert!(p.compile().is_ok());
    }
    #[test]
    fn st_projects_take_extra_programs_too() {
        let mut p = Project {
            language: SourceLanguage::St,
            source: ST_TEMPLATE.into(),
            ..Project::default()
        };
        let unit = p.add_unit(UnitKind::Interrupt).unwrap();
        p.set_program_source(unit, "LD M1000\nINC D10\nEND\n".into());
        p.validate().unwrap();
        assert!(p.compile().is_ok(), "{:?}", p.compile().err());
    }

    #[test]
    fn a_subroutine_runs_only_while_main_calls_it_and_a_skipped_one_holds_its_outputs() {
        use crate::plc::{Device, Simulator};
        let mut p = Project {
            source: "LD X0
CALL Pump
END
".into(),
            ..Project::default()
        };
        let index = p.add_unit(UnitKind::Subroutine).unwrap();
        p.rename_unit(index, "Pump").unwrap();
        p.set_program_source(index, "LD M1000
OUT Y1
END
".into());
        let program = p.compile().unwrap();
        let y1: Device = "Y1".parse().unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_input("X0".parse().unwrap(), true);
        sim.scan(&program, 0);
        assert!(sim.memory.bit(y1), "X0 on: the subroutine ran");
        sim.memory.set_input("X0".parse().unwrap(), false);
        sim.scan(&program, 0);
        assert!(
            sim.memory.bit(y1),
            "X0 off: the call is skipped, so the coil keeps its state like on the PLC"
        );
    }

    #[test]
    fn call_pointers_avoid_cj_pointers_and_interrupts_use_documented_labels() {
        let mut p = Project {
            source: "LD X0
CJ P0
OUT Y0
P0
LD X1
CALL P3
END
".into(),
            ..Project::default()
        };
        let sub = p.add_unit(UnitKind::Subroutine).unwrap();
        assert_eq!(p.units[sub - 1].number, 1, "P0 belongs to the CJ in Main");
        assert!(p.set_unit_number(sub, 0).is_err());
        assert!(p.set_unit_number(sub, 3).is_ok(), "a CALL operand is a reference, not a CJ");
        let int = p.add_unit(UnitKind::Interrupt).unwrap();
        assert_eq!(p.units[int - 1].number, 1);
        let second = p.add_unit(UnitKind::Interrupt).unwrap();
        assert_eq!(p.units[second - 1].number, 0, "I000 = X0 falling edge is the next free label");
        let note = |n| UnitKind::Interrupt.pointer_note(n);
        assert_eq!(note(1).as_deref(), Some("X0 ↑"));
        assert_eq!(note(100).as_deref(), Some("X1 ↓"));
        assert_eq!(note(602).as_deref(), Some("2 ms"));
        assert_eq!(note(805).as_deref(), Some("5 ×0,1 ms"));
        assert_eq!(note(20).as_deref(), Some("HSC 2"));
        assert_eq!(note(150).as_deref(), Some("COM2"));
        assert_eq!(note(2), None);
        assert_eq!(UnitKind::Subroutine.pointer_note(0), None);
    }

    #[test]
    fn cyclic_parts_run_after_main_in_list_order() {
        let mut p = Project {
            source: "LD X0
OUT Y0
END
".into(),
            ..Project::default()
        };
        let first = p.add_unit(UnitKind::Cyclic).unwrap();
        p.set_program_source(first, "LD X1
OUT Y1
END
".into());
        let second = p.add_unit(UnitKind::Cyclic).unwrap();
        p.set_program_source(second, "LD X2
OUT Y2
END
".into());
        assert_eq!(p.units[first - 1].name, "PRG_1");
        assert_eq!(p.layout().il, "LD X0
OUT Y0
LD X1
OUT Y1
LD X2
OUT Y2
FEND
END
");
        let whole = p.compile().unwrap();
        assert_eq!(whole.networks.len(), 3);
        assert_eq!(p.program_view(&whole, second).networks.len(), 1);
        // Moving a part changes the execution order.
        assert_eq!(p.move_unit(second, true), Some(first));
        assert!(p.layout().il.contains("LD X2
OUT Y2
LD X1
OUT Y1
"));
        assert_eq!(p.move_unit(1, true), None, "already first");
        assert_eq!(p.move_unit(2, false), None, "already last");
        assert!(p.set_unit_number(1, 0).is_err(), "cyclic parts have no pointer");
        // A FEND in Main would put the parts behind it.
        p.source = "LD X0
OUT Y0
FEND
END
".into();
        assert!(p.compile().is_err());
        assert!(p.add_unit(UnitKind::Cyclic).is_err());
    }
    #[test]
    fn a_disabled_unit_is_left_out_but_stays_editable() {
        let mut p = with_units();
        p.set_unit_enabled(1, false);
        let layout = p.layout();
        assert_eq!(layout.ranges[1], (1, 0), "no lines in the program");
        assert!(!layout.il.contains("
P0
"), "no label line");
        assert!(!layout.il.contains("OUT Y1"), "its body is not compiled");
        // Main still calls it by name, so the label is reported missing.
        assert!(p.compile().unwrap_err().message.contains("P0"));
        p.source = "LD X1
OUT Y0
END
".into();
        let whole = p.compile().unwrap();
        let view = p.program_view(&whole, 1);
        assert_eq!(view.networks.len(), 1, "its own networks are still shown");
        assert!(p.compile_program_text(1, "LD M1000
OUT Y2
END
").is_ok());
        assert!(p.units[0].label().ends_with("(devre dışı)"));
        p.set_unit_enabled(1, true);
        assert!(p.layout().il.contains("P0
"));
    }
    #[test]
    fn unit_flags_and_kinds_roundtrip_in_json() {
        let mut p = with_units();
        let cyclic = p.add_unit(UnitKind::Cyclic).unwrap();
        p.set_unit_enabled(cyclic, false);
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json.matches("\"enabled\":false").count(), 1, "only disabled units carry the flag");
        let q: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(q.units, p.units);
        q.validate().unwrap();
    }

    fn sym(ty: crate::st::DataType, device: Option<&str>) -> Symbol {
        Symbol {
            ty,
            device: device.map(str::to_owned),
            auto: false,
            retain: false,
            comment: String::new(),
            init: None,
        }
    }
    #[test]
    fn symbol_names_follow_the_editor_rules() {
        for good in ["Start_BT", "_M0", "a", "Motor1", "x_y"] {
            assert!(check_symbol_name(good).is_ok(), "{good}");
        }
        let long = "a".repeat(31);
        for bad in ["M0", "Y17", "K10", "H1F", "a__b", "ends_", "has space", "1abc", "a-b", "a@b", "", long.as_str()] {
            assert!(check_symbol_name(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn il_operands_may_use_symbol_names_without_moving_any_line() {
        use crate::st::DataType::{Bool, Word};
        let mut p = Project {
            source: "; pump
LD Start
OUT Motor ; Start stays in comments

LD X1
SET Set
MOV K5 Count
END
".into(),
            ..Project::default()
        };
        p.set_symbol("Start", sym(Bool, Some("X0"))).unwrap();
        p.set_symbol("Motor", sym(Bool, Some("Y0"))).unwrap();
        p.set_symbol("Set", sym(Bool, Some("M5"))).unwrap();
        p.set_symbol("Count", sym(Word, Some("D20"))).unwrap();
        let il = p.layout().il;
        assert_eq!(
            il,
            "; pump
LD X0
OUT Y0 ; Start stays in comments

LD X1
SET M5
MOV K5 D20
END
",
            "mnemonics (even one spelled like a symbol) and comments are left alone"
        );
        assert_eq!(il.lines().count(), p.source.lines().count());
        let program = p.compile().unwrap();
        assert_eq!(program.networks.len(), 2);
        assert_eq!(program.networks[0].comment, "pump");
    }
    #[test]
    fn symbols_without_an_address_get_one_around_what_the_program_uses() {
        use crate::st::DataType::{Bool, Dword, Word};
        let mut p = Project {
            source: "LD X0
MOV K1 D4000
SET M800
END
".into(),
            ..Project::default()
        };
        p.set_symbol("Flag", sym(Bool, None)).unwrap();
        p.set_symbol("Count", sym(Word, None)).unwrap();
        p.set_symbol("Total", sym(Dword, None)).unwrap();
        assert!(p.assign_symbol_addresses().unwrap());
        let device = |n: &str| p.symbols[n].device.clone().unwrap();
        assert_eq!(device("Flag"), "M801", "M800 is written in Main");
        assert_eq!(device("Count"), "D4002", "D4000 and its neighbour are taken");
        assert_eq!(device("Total"), "D4003");
        assert!(p.symbols["Flag"].auto);
        assert!(!p.assign_symbol_addresses().unwrap(), "nothing left to assign");
        p.clear_auto_addresses();
        assert!(p.symbols.values().all(|s| s.device.is_none() && !s.auto));
        // A tiny range runs out with a clear message.
        p.set_alloc(crate::alloc::Alloc { d: (4000, 4003), m: (800, 999) }).unwrap();
        let error = p.assign_symbol_addresses().unwrap_err();
        assert!(error.contains("Total") && error.contains("dolu"), "{error}");
    }
    #[test]
    fn symbol_devices_must_fit_the_type_and_not_overlap() {
        use crate::st::DataType::{Bool, Dword, Word};
        let mut p = Project::default();
        assert!(p.set_symbol("Wide", sym(Dword, Some("D10"))).is_ok());
        assert!(p.set_symbol("Narrow", sym(Word, Some("D11"))).is_err(), "inside the double word");
        assert!(p.set_symbol("Next", sym(Word, Some("D12"))).is_ok());
        assert!(p.set_symbol("Flag", sym(Bool, Some("D5"))).is_err(), "BOOL needs a bit device");
        assert!(p.set_symbol("Num", sym(Word, Some("M5"))).is_err(), "numbers live in D");
        assert!(p.set_symbol("WIDE", sym(Word, Some("D50"))).is_err(), "case-insensitive duplicate");
        assert!(p.set_symbol("Bad", sym(Word, Some("d010"))).is_err(), "not canonical");
        p.validate().unwrap();
    }
    #[test]
    fn st_uses_global_symbols_and_the_configured_allocation_range() {
        use crate::st::DataType::Int;
        let mut p = Project {
            source: "PROGRAM Main
VAR
    t : INT;
END_VAR
t := Speed + 1;
Speed := t;
END_PROGRAM
".into(),
            language: SourceLanguage::St,
            ..Project::default()
        };
        p.set_symbol("Speed", sym(Int, Some("D100"))).unwrap();
        let il = p.instruction_list().unwrap();
        assert!(il.contains("D100"), "{il}");
        assert!(il.contains("D4000"), "locals start at the default range: {il}");
        p.set_alloc(crate::alloc::Alloc { d: (2000, 2999), m: (800, 999) }).unwrap();
        let il = p.instruction_list().unwrap();
        assert!(il.contains("D2000") && !il.contains("D4000"), "{il}");
        assert!(p.compile().is_ok());
        // A local of the same name shadows the global.
        p.source = "PROGRAM Main
VAR
    Speed : INT;
END_VAR
Speed := 3;
END_PROGRAM
".into();
        assert!(!p.instruction_list().unwrap().contains("D100"));
    }
    #[test]
    fn symbols_and_ranges_roundtrip_and_old_files_stay_clean() {
        use crate::st::DataType::Word;
        let mut p = Project::default();
        let plain = serde_json::to_string(&p).unwrap();
        assert!(!plain.contains("symbols") && !plain.contains("alloc"));
        p.set_symbol("Count", Symbol { comment: "pieces".into(), init: Some("5".into()), ..sym(Word, Some("D20")) })
            .unwrap();
        p.set_alloc(crate::alloc::Alloc { d: (2000, 2999), m: (800, 999) }).unwrap();
        let q: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(q.symbols, p.symbols);
        assert_eq!(q.alloc, p.alloc);
        q.validate().unwrap();
        // The symbol's comment documents its device when no device comment exists.
        assert_eq!(q.comment("D20".parse().unwrap()), Some("pieces"));
    }

    const FLAT: &str = "LD X0\nCALL P0\nFEND\nP0\nLD X1\nOUT Y0\nSRET\nI001\nLD M1000\nINC D10\nIRET\nEND\n";
    #[test]
    fn a_flat_program_splits_into_main_and_units_and_joins_back_unchanged() {
        let mut p = Project { source: FLAT.into(), ..Project::default() };
        assert!(p.can_split());
        assert_eq!(p.split_main().unwrap(), 2);
        assert_eq!(p.source, "LD X0\nCALL P0\nEND\n");
        assert_eq!(p.units[0].name, "SR_0");
        assert_eq!((p.units[0].kind, p.units[0].number), (UnitKind::Subroutine, 0));
        assert_eq!(p.units[0].source, "LD X1\nOUT Y0\nEND\n");
        assert_eq!(p.units[1].name, "INT_001");
        assert_eq!(p.layout().il, FLAT, "joining gives the original program back");
        p.validate().unwrap();
        assert!(!p.can_split(), "already split");
    }
    #[test]
    fn splitting_keeps_cj_labels_inside_a_body_and_comments_with_their_unit() {
        let flat = "LD X0\nCALL P0\nFEND\n; pump\nP0\nLD X1\nCJ P5\nOUT Y0\nP5\nLD X2\nOUT Y1\nSRET\nEND\n";
        let (main, units) = Project::split_flat(flat).unwrap();
        assert_eq!(main, "LD X0\nCALL P0\nEND\n");
        assert_eq!(units.len(), 1, "P5 is a CJ target, not a subroutine");
        assert_eq!(
            units[0].source,
            "; pump\nLD X1\nCJ P5\nOUT Y0\nP5\nLD X2\nOUT Y1\nEND\n",
            "the comment above the label moves into the unit"
        );
        let p = Project { source: main, units, ..Project::default() };
        assert_eq!(
            p.layout().il,
            "LD X0\nCALL P0\nFEND\nP0\n; pump\nLD X1\nCJ P5\nOUT Y0\nP5\nLD X2\nOUT Y1\nSRET\nEND\n"
        );
        assert!(p.compile().is_ok());
    }
    #[test]
    fn unclear_programs_are_refused_with_a_reason() {
        let reason = |il: &str| Project::split_flat(il).unwrap_err();
        assert!(reason("LD X0\nOUT Y0\nEND\n").contains("FEND yok"));
        assert!(reason("LD X0\nFEND\nP0\nSRET\nFEND\nEND\n").contains("Birden çok FEND"));
        assert!(reason("LD X0\nCALL P0\nFEND\nP0\nLD X1\nOUT Y0\nEND\n").contains("SRET ile bitmiyor"));
        assert!(reason("LD X0\nFEND\nI002\nIRET\nEND\n").contains("belgelenmiş"));
        assert!(reason("LD X0\nFEND\nLD X1\nOUT Y0\nEND\n").contains("etiketsiz"));
        assert!(reason("LD X0\nCALL P0\nFEND\nP0\nSRET\nP0\nSRET\nEND\n").contains("birden çok"));
        let mut p = Project { source: "LD X0\nOUT Y0\nEND\n".into(), ..Project::default() };
        assert!(p.split_main().is_err());
        assert_eq!(p.source, "LD X0\nOUT Y0\nEND\n", "a refused split changes nothing");
    }

    #[test]
    fn initial_values_parse_per_type() {
        use crate::st::DataType::*;
        assert_eq!(parse_initial(Bool, "TRUE").unwrap(), vec![1]);
        assert_eq!(parse_initial(Bool, "off").unwrap(), vec![0]);
        assert!(parse_initial(Bool, "2").is_err());
        assert_eq!(parse_initial(Word, "H1F").unwrap(), vec![31]);
        assert_eq!(parse_initial(Word, "65535").unwrap(), vec![0xFFFF]);
        assert!(parse_initial(Word, "70000").is_err());
        assert_eq!(parse_initial(Int, "-5").unwrap(), vec![0xFFFB]);
        assert!(parse_initial(Int, "40000").is_err());
        assert_eq!(parse_initial(Dword, "H12345678").unwrap(), vec![0x5678, 0x1234], "low word first");
        assert_eq!(parse_initial(Dint, "-1").unwrap(), vec![0xFFFF, 0xFFFF]);
        assert_eq!(parse_initial(Real, "1,5").unwrap(), vec![0x0000, 0x3FC0]);
        assert!(parse_initial(Real, "abc").is_err());
    }
    #[test]
    fn initial_values_map_to_modbus_addresses_and_reject_bad_text() {
        use crate::{plc_download::Initial, st::DataType::*};
        let mut p = Project::default();
        let with_init = |ty, device: &str, init: &str| Symbol {
            init: Some(init.into()),
            ..sym(ty, Some(device))
        };
        p.set_symbol("Count", with_init(Word, "D20", "5")).unwrap();
        p.set_symbol("Flag", with_init(Bool, "M5", "TRUE")).unwrap();
        p.set_symbol("Wide", with_init(Dword, "D4095", "H10002")).unwrap();
        assert!(p.set_symbol("Bad", with_init(Word, "D30", "70000")).is_err());
        let values = p.initial_values().unwrap();
        let by_device = |d: &str| values.iter().find(|v| v.device.to_string() == d).unwrap();
        assert_eq!(by_device("D20").to_modbus().unwrap(), Initial::Words { address: 0x1014, words: vec![5] });
        assert_eq!(by_device("M5").to_modbus().unwrap(), Initial::Bit { address: 0x0805, on: true });
        assert!(by_device("D4095").to_modbus().is_err(), "the address map jumps after D4095");
        p.set_symbol("Wide", with_init(Dword, "D4094", "H10002")).unwrap();
        let wide = p.initial_values().unwrap().into_iter().find(|v| v.device.to_string() == "D4094").unwrap();
        assert_eq!(wide.to_modbus().unwrap(), Initial::Words { address: 0x1000 + 4094, words: vec![2, 1] });
    }

    #[test]
    fn retained_symbols_come_from_the_latched_range() {
        use crate::st::DataType::{Bool, Word};
        let mut p = Project::default();
        let kept = |ty, device: Option<&str>| Symbol { retain: true, ..sym(ty, device) };
        p.set_symbol("Count", kept(Word, None)).unwrap();
        p.set_symbol("Flag", kept(Bool, None)).unwrap();
        p.set_symbol("Scratch", sym(Word, None)).unwrap();
        assert!(p.assign_symbol_addresses().unwrap());
        let device = |n: &str| p.symbols[n].device.clone().unwrap();
        assert_eq!(device("Count"), "D2000", "retain pool: latched D2000..");
        assert_eq!(device("Flag"), "M2048");
        assert_eq!(device("Scratch"), "D4000", "normal pool is untouched");
        // An explicit address must lie in a latched area.
        assert!(p.set_symbol("Bad", kept(Word, Some("D100"))).is_err());
        assert!(p.set_symbol("Good", kept(Word, Some("D450"))).is_ok());
        assert!(p.set_symbol("BadBit", kept(Bool, Some("M100"))).is_err());
        // A custom retain range is checked against the allocation range.
        assert!(p.set_retain(crate::alloc::Retain { d: (4000, 4100), m: (2048, 2303) }).is_err());
        p.validate().unwrap();
        let q: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert!(q.symbols["Count"].retain);
        assert!(!serde_json::to_string(&Project::default()).unwrap().contains("retain"));
    }
    #[test]
    fn st_var_retain_and_arrays_work_through_the_project() {
        let p = Project {
            source: "PROGRAM Main\nVAR_RETAIN\n    total : INT;\nEND_VAR\nVAR\n    a : ARRAY[0..3] OF INT;\n    i : INT;\nEND_VAR\ni := 2;\na[i] := total + 1;\ntotal := a[2];\nEND_PROGRAM\n".into(),
            language: SourceLanguage::St,
            ..Project::default()
        };
        let il = p.instruction_list().unwrap();
        assert!(il.contains("D2000"), "retained total sits in the latched range: {il}");
        assert!(il.contains("E0"), "variable index uses E0: {il}");
        assert!(p.compile().is_ok());
    }

    const ST_COUNTER: &str = "PROGRAM Counter\nVAR\n    n : INT;\nEND_VAR\nn := n + 1;\nIF n > 3 THEN\n    n := 0;\nEND_IF;\nEND_PROGRAM\n";
    #[test]
    fn st_units_share_one_allocation_and_one_label_space() {
        use crate::plc::{Device, Simulator};
        let mut p = Project {
            source: "LD X0\nOUT Y0\nEND\n".into(),
            ..Project::default()
        };
        let first = p.add_unit(UnitKind::Cyclic).unwrap();
        p.set_unit_language(first, SourceLanguage::St).unwrap();
        p.set_program_source(first, ST_COUNTER.into());
        let second = p.add_unit(UnitKind::Cyclic).unwrap();
        p.set_unit_language(second, SourceLanguage::St).unwrap();
        p.set_program_source(second, ST_COUNTER.replace("Counter", "Other").replace('n', "m").replace("PROGRAM Other", "PROGRAM Other"));
        let il = p.instruction_list().unwrap();
        // The second unit's local sits after the first unit's local and temporaries.
        let second_local = il
            .split("FEND")
            .next()
            .unwrap()
            .rsplit("P1\n")
            .next()
            .unwrap()
            .lines()
            .find(|l| l.starts_with("LD") && l.ends_with(" K3"))
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or_else(|| panic!("{il}"))
            .to_owned();
        assert_ne!(second_local, "D4000", "two locals, two devices: {il}");
        let program = p.compile().unwrap_or_else(|e| panic!("{e:?}\n{il}"));
        let mut sim = Simulator::default();
        let word = |sim: &Simulator, n: &str| sim.memory.word(n.parse::<Device>().unwrap());
        // Give the counters different phases: a shared word would show up as equal values.
        sim.memory
            .set_word(second_local.parse::<Device>().unwrap(), 2);
        for _ in 0..3 {
            sim.scan(&program, 0);
        }
        assert_eq!(
            (word(&sim, "D4000"), word(&sim, &second_local)),
            (3, 1),
            "each unit counts on its own local\n{il}"
        );
    }
    #[test]
    fn an_st_unit_error_is_reported_in_its_own_program_and_line() {
        let mut p = Project {
            source: "LD X0\nOUT Y0\nEND\n".into(),
            ..Project::default()
        };
        let unit = p.add_unit(UnitKind::Interrupt).unwrap();
        p.set_unit_language(unit, SourceLanguage::St).unwrap();
        p.set_program_source(unit, "PROGRAM Bad\nVAR\n    n : INT;\nEND_VAR\nn := missing;\nEND_PROGRAM\n".into());
        let error = p.compile_located().unwrap_err();
        assert_eq!(error.program, unit);
        assert_eq!(error.line, 5, "the ST line, not an IL line");
        assert!(error.message.contains("MISSING"), "{}", error.message);
    }
    #[test]
    fn st_labels_keep_clear_of_subroutine_pointers_and_the_language_switch_is_guarded() {
        let mut p = Project {
            source: "PROGRAM Main\nVAR\n    n : INT;\nEND_VAR\nIF n > 3 THEN\n    n := 0;\nEND_IF;\nEND_PROGRAM\n".into(),
            language: SourceLanguage::St,
            ..Project::default()
        };
        let sub = p.add_unit(UnitKind::Subroutine).unwrap();
        p.set_program_source(sub, "LD M1000\nINC D10\nEND\n".into());
        p.set_unit_number(sub, 1).unwrap();
        let il = p.instruction_list().unwrap();
        assert!(!il.contains("CJ P1\n") || il.contains("\nP1\nLD M1000\nINC D10"), "{il}");
        assert!(p.compile().is_ok(), "{il}");
        // Only an empty unit may change language.
        assert!(p.set_unit_language(sub, SourceLanguage::St).is_err());
        p.set_program_source(sub, "END\n".into());
        p.set_unit_language(sub, SourceLanguage::St).unwrap();
        assert_eq!(p.units[sub - 1].language, SourceLanguage::St);
        assert!(p.units[sub - 1].label().ends_with("· ST"));
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json.matches("\"language\":\"st\"").count(), 2, "Main and the unit");
        let q: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(q.units, p.units);
        p.set_unit_language(sub, SourceLanguage::Il).unwrap();
        assert_eq!(p.program_language(sub), SourceLanguage::Il);
    }

    /// A `.dvp` in the editor's own form (synthetic words: single-word AND/OR, `xxFF` blocks,
    /// edge contacts, STL) opens as IL, and the decimal X/Y comment rows map to octal names.
    #[test]
    fn an_editor_form_dvp_file_opens_as_il() {
        let words = [
            0xea2b, 0x00d0, // LD M1002, SET S0
            0x00a0, 0x0024, 0x0124, 0x0344, 0xf9ff, 0x0154, 0x10c5, // STL S0 .. OUT Y20
            0x0101, 0x1084, 0x0480, 0x14d0, // LDP X20, SET S20
            0x14a0, 0x0124, 0x00c0, 0x0e00, // STL S20, LD X1, OUT S0, RET
            0x0f00,
        ];
        let comments = [("X16".to_owned(), "start".to_owned())];
        let path = std::env::temp_dir().join(format!("opendvp-editor-form-{}.dvp", std::process::id()));
        // write() takes the words verbatim; the comment row is stored decimal (X16 = X20).
        let mut bytes = crate::dvp_file::write(&words, "Synthetic", &[]).unwrap();
        let marker = b"Function=Device Comment\n<TypeEnd>\n<DataBegin>\n";
        let at = bytes.windows(marker.len()).position(|w| w == marker).unwrap() + marker.len();
        bytes.splice(at..at, b"X16$&start\n".iter().copied());
        std::fs::write(&path, &bytes).unwrap();
        let p = Project::load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            p.source,
            "LD M1002\nSET S0\nSTL S0\nLD X0\nLD X1\nAND X3\nORB\nANI X1\nOUT Y20\nLDP X20\nSET S20\nSTL S20\nLD X1\nOUT S0\nRET\nEND\n"
        );
        assert_eq!(p.comment("X20".parse().unwrap()), Some(comments[0].1.as_str()));
    }

    #[test]
    fn dvp_import_preserves_supported_models_in_both_instruction_formats() {
        let source = "LD X0\nAND X1\nOUT Y0\nEND\n";
        let native = crate::es2_encoder::encode(&crate::plc::compile(source).unwrap())
            .unwrap()
            .words;
        // Single-word editor AND versus the PLC encoder's LD/ANB form. Exercise
        // both import decoders, rather than a program common to both formats.
        let editor = vec![0x0024, 0x0144, 0x00c5, 0x0f00];
        assert!(crate::dvp_decode::decode(&native).is_err());
        assert!(crate::dvp_decode::decode(&editor).is_ok());
        for &(model, code) in crate::dvz::MODEL_INDICES {
            if crate::devices::find(model).is_none() {
                continue;
            }
            for (format, words) in [("editor", &editor), ("plc", &native)] {
                let path = std::env::temp_dir().join(format!(
                    "opendvp-dvp-model-{}-{code}-{format}.dvp",
                    std::process::id()
                ));
                let saved = path.with_extension("odvp");
                let mut bytes = crate::dvp_file::write(
                    words,
                    "Model import",
                    &[("Y0".into(), "motor".into())],
                )
                .unwrap();
                // Model metadata is separate from the program checksum/key.
                bytes[0x54..0x56].copy_from_slice(&code.to_le_bytes());
                fs::write(&path, bytes).unwrap();
                let project = Project::load(&path).unwrap();
                assert_eq!(project.device, model, "{format} model 0x{code:04X}");
                assert_eq!(project.language, SourceLanguage::Il);
                assert_eq!(project.comment("Y0".parse().unwrap()), Some("motor"));
                assert_eq!(project.compile().unwrap().networks.len(), 1);
                project.save(&saved).unwrap();
                assert_eq!(Project::load(&saved).unwrap().device, model);
                fs::remove_file(&path).unwrap();
                fs::remove_file(&saved).unwrap();
            }
        }
    }

    #[test]
    fn dvp_import_rejects_unsupported_or_unknown_models_instead_of_defaulting_to_es2() {
        for (code, label) in [(0x12u16, "MC"), (0xffff, "bilinmeyen")] {
            let path = std::env::temp_dir().join(format!(
                "opendvp-dvp-unsupported-{}-{code}.dvp",
                std::process::id()
            ));
            let mut bytes = crate::dvp_file::write(&[0x0024, 0x00c5, 0x0f00], "", &[]).unwrap();
            bytes[0x54..0x56].copy_from_slice(&code.to_le_bytes());
            fs::write(&path, bytes).unwrap();
            let error = Project::load(&path).unwrap_err();
            assert!(error.contains(label), "{error}");
            assert!(error.contains(&format!("0x{code:04X}")), "{error}");
            fs::remove_file(&path).unwrap();
        }
    }

    #[test]
    fn a_project_exports_as_a_editor_dvp_file_that_reads_back() {
        let mut p = Project {
            source: "LD X0\nOUT Y0\nEND\n".into(),
            name: "Motor kontrolü çok uzun bir proje adı".into(),
            ..Project::default()
        };
        p.set_comment("X0".parse().unwrap(), "start button");
        p.set_comment("Y0".parse().unwrap(), "motor");
        p.set_comment("X17".parse().unwrap(), "octal name, decimal grid row");
        let path = std::env::temp_dir().join(format!("opendvp-export-{}.dvp", std::process::id()));
        let skipped = p.export_dvp(&path).unwrap();
        assert_eq!(skipped, 0);
        let file = crate::dvp_file::read(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(file.program, vec![0x0024, 0x00c5, 0x0f00]);
        assert_eq!(file.title.chars().count(), 20, "titles are cut to 20 characters");
        assert!(file.device_comments.iter().any(|(d, t)| d == "X0" && t == "start button"));
        assert!(file.device_comments.iter().any(|(d, _)| d == "X17"));
        let loaded = Project::load(&path).unwrap();
        assert_eq!(loaded.source, "LD X0\nOUT Y0\nEND\n");
        assert_eq!(
            loaded.comment("X17".parse().unwrap()),
            Some("octal name, decimal grid row")
        );
        // export_source routes by extension and the project can be read back as IL.
        assert!(p.export_source(&path).is_ok());
        std::fs::remove_file(&path).unwrap();
        let mut other = p.clone();
        other.device = "SS2".into();
        assert!(other.export_dvp(&path).is_err(), "only ES2 is supported");
    }
}
