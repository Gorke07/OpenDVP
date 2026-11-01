//! A bounded Structured Text frontend for the existing Delta instruction VM.
mod emit;
mod syntax;
#[cfg(test)]
mod tests;

use crate::plc::Diagnostic;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    Bool,
    Int,
    Dint,
    Word,
    Dword,
    Real,
}
impl DataType {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bool => "BOOL",
            Self::Int => "INT",
            Self::Dint => "DINT",
            Self::Word => "WORD",
            Self::Dword => "DWORD",
            Self::Real => "REAL",
        }
    }
    pub fn words(self) -> usize {
        match self {
            Self::Dint | Self::Dword | Self::Real => 2,
            _ => 1,
        }
    }
    pub(super) fn bits(self) -> bool {
        matches!(self, Self::Word | Self::Dword)
    }
}
impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StSymbol {
    pub name: String,
    pub data_type: DataType,
    /// Canonical Delta device, e.g. D4000 or X17 (octal X/Y numbering).
    pub address: String,
    pub line: usize,
    pub explicitly_mapped: bool,
    /// Declared in `VAR_RETAIN`: the device lives in a latched (power-fail retentive) area.
    pub retained: bool,
    /// For `ARRAY[lo..hi] OF T`: the inclusive bounds. `data_type` is the element type and
    /// `address` the device of element `lo`; elements follow in consecutive words/bits.
    pub array: Option<(i32, i32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredSt {
    pub il: String,
    /// One original, one-based ST line for each emitted IL line.
    pub line_map: Vec<usize>,
    pub symbols: Vec<StSymbol>,
    /// First `P` label number the lowering did not use; the next ST program of a project
    /// starts here so the labels of two programs never collide.
    pub next_label: u16,
}

/// Does not recursively call plc::compile. The caller compiles `il` and maps
/// any target diagnostic through `line_map` before presenting it to the user.
pub fn lower(source: &str) -> Result<LoweredSt, Diagnostic> {
    lower_with(source, &LowerOptions::default())
}
/// A project-wide symbol the ST body may use without declaring it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Global {
    pub name: String,
    pub data_type: DataType,
    /// Canonical Delta device, e.g. `D100` or `M5`.
    pub address: String,
}
/// Where locals/temporaries go and which project symbols are visible.
#[derive(Debug, Clone, Default)]
pub struct LowerOptions {
    pub alloc: crate::alloc::Alloc,
    /// Latched ranges handed to `VAR_RETAIN` variables.
    pub retain: crate::alloc::Retain,
    pub globals: Vec<Global>,
    /// Devices already taken by other programs of the project (their locals), so this
    /// program's locals and temporaries are placed around them.
    pub reserved: Vec<(crate::plc::Device, usize)>,
    /// First `P` label number control flow may use (0 behaves like 1).
    pub label_start: u16,
    /// Label numbers that belong to someone else (subroutine pointers).
    pub avoid_labels: Vec<u16>,
}
pub fn lower_with(source: &str, options: &LowerOptions) -> Result<LoweredSt, Diagnostic> {
    let unit = syntax::parse(source)?;
    emit::lower(unit, options)
}
/// Comments and string contents cannot select the frontend. Explicit project
/// language remains preferable for declaration-free, control-only ST bodies.
pub fn is_st_source(source: &str) -> bool {
    syntax::is_st_source(source)
}

pub(super) fn error(line: usize, column: usize, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        line,
        message: format!("ST sütun {column}: {}", message.into()),
    }
}
