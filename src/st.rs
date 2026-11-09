//! A bounded Structured Text frontend for the existing Delta instruction VM.
mod blocks;
mod emit;
mod library;
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
    /// Words and bits the program uses as temporaries. They are rewritten within a scan, so
    /// another ST program of the project must not place its variables there (sharing them as
    /// its own temporaries would be safe, but they are simply kept apart).
    pub temporaries: Vec<crate::plc::Device>,
    /// Block types whose instances run one shared body.
    pub shared_blocks: Vec<String>,
    /// IL lines of the program body; when shared block bodies exist, `FEND`, their
    /// subroutines and `END` follow, otherwise only `END`.
    pub main_lines: usize,
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
    /// Whether repeated function-block instances share one subroutine body.
    pub share: Share,
}
/// When the instances of a `FUNCTION_BLOCK` share one body instead of every call being
/// expanded in place. A shared body is emitted once after `FEND`, split into subroutines at
/// its timer / `CTU` calls (Delta timers and counters cannot be index-addressed, so each
/// instance runs its own `TMR` / `CNT` between the `CALL`s); the instance's variables are
/// reached through the index registers E6 (words) and E7 (bits).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Share {
    /// Every call is expanded in place.
    Never,
    /// Block types with two or more top-level instances are shared when their body allows it
    /// and sharing makes the program smaller (each such type is measured both ways).
    #[default]
    Repeated,
    /// Every block type with a top-level instance is shared (when its body allows it).
    Always,
}
pub use library::{LibraryEntry, LibraryParam, library_entries, library_errors};
pub fn lower_with(source: &str, options: &LowerOptions) -> Result<LoweredSt, Diagnostic> {
    #[allow(unused_mut)]
    let mut share = options.share;
    // Lets the whole test suite run with every eligible block shared.
    #[cfg(test)]
    if share == Share::Repeated && std::env::var("OPENDVP_ST_SHARE").as_deref() == Ok("always") {
        share = Share::Always;
    }
    let none = std::collections::BTreeSet::new();
    if share != Share::Repeated {
        return lower_once(source, options, share, &none);
    }
    let all = lower_once(source, options, Share::Repeated, &none);
    let candidates = match &all {
        Ok(l) if !l.shared_blocks.is_empty() => l.shared_blocks.clone(),
        Ok(_) => return all,
        // A failure of the shared form is not reported if the program lowers without it.
        Err(_) => return lower_once(source, options, Share::Never, &none).or(all),
    };
    // Program words (or the IL line count when the program does not encode) of a lowering.
    let size = |l: &Result<LoweredSt, Diagnostic>| -> (usize, usize) {
        let Ok(l) = l else {
            return (usize::MAX, usize::MAX);
        };
        let words = crate::plc::compile_il(&l.il)
            .ok()
            .and_then(|p| crate::es2_encoder::encode(&p).ok())
            .map_or(usize::MAX, |e| e.words.len());
        (words, l.il.lines().count())
    };
    let inline = lower_once(source, options, Share::Never, &none);
    let base = size(&inline);
    if candidates.len() == 1 {
        return if size(&all) <= base { all } else { inline };
    }
    // Keep each block type whose sharing alone makes the program smaller.
    let mut dropped = std::collections::BTreeSet::new();
    for block in &candidates {
        let others = candidates.iter().filter(|b| *b != block).cloned().collect();
        if size(&lower_once(source, options, Share::Repeated, &others)) > base {
            dropped.insert(block.clone());
        }
    }
    if dropped.len() == candidates.len() {
        return inline.or(all);
    }
    if dropped.is_empty() {
        return all;
    }
    lower_once(source, options, Share::Repeated, &dropped)
}
fn lower_once(
    source: &str,
    options: &LowerOptions,
    share: Share,
    exclude: &std::collections::BTreeSet<String>,
) -> Result<LoweredSt, Diagnostic> {
    let unit = syntax::parse(source, share, exclude)?;
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
