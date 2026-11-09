//! The built-in ST standard library: `FUNCTION_BLOCK`s and `FUNCTION`s every ST program
//! may use without declaring them. Only blocks a program actually instantiates or calls
//! are inlined, so an unused library costs no program steps and no memory.
//!
//! Sources live in `src/st/stdlib/*.st` (one file per category). A block is documented by
//! an English `(*! ... *)` comment and a Turkish `(*!tr ... *)` comment directly in front of
//! its `FUNCTION_BLOCK` / `FUNCTION` line (format: `src/st/stdlib/README.md`).
//! A program that defines a block or type with a library name uses its own definition.
use super::syntax::{self, Decl, LibraryDefs};
use crate::plc::Diagnostic;
use std::sync::OnceLock;

/// `(category, source)` in load order; a later file may use blocks of an earlier one.
pub(super) const FILES: &[(&str, &str)] = &[
    ("logic", include_str!("stdlib/logic.st")),
    ("math", include_str!("stdlib/math.st")),
    ("control", include_str!("stdlib/control.st")),
    ("process", include_str!("stdlib/process.st")),
];

struct Loaded {
    defs: LibraryDefs,
    errors: Vec<(String, Diagnostic)>,
}

fn loaded() -> &'static Loaded {
    static LOADED: OnceLock<Loaded> = OnceLock::new();
    LOADED.get_or_init(|| {
        let mut defs = LibraryDefs::default();
        let mut errors = Vec::new();
        for (category, source) in FILES {
            // A broken file is skipped as a whole; the others stay usable.
            let mut next = defs.clone();
            match syntax::parse_library(source, &mut next) {
                Ok(()) => defs = next,
                Err(e) => errors.push((category.to_string(), e)),
            }
        }
        Loaded { defs, errors }
    })
}

pub(super) fn defs() -> &'static LibraryDefs {
    &loaded().defs
}

/// Library files that failed to parse (`category`, diagnostic). Empty in a correct build.
pub fn library_errors() -> Vec<(String, Diagnostic)> {
    loaded().errors.clone()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryParam {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryEntry {
    pub name: String,
    /// `FUNCTION_BLOCK` or `FUNCTION`.
    pub kind: &'static str,
    pub category: &'static str,
    /// English documentation (`(*! *)`).
    pub doc: String,
    /// Turkish documentation (`(*!tr *)`); empty when missing.
    pub doc_tr: String,
    pub inputs: Vec<LibraryParam>,
    pub outputs: Vec<LibraryParam>,
    /// Result type of a `FUNCTION`.
    pub returns: Option<String>,
}

fn type_text(d: &Decl) -> String {
    let base = if let Some(fb) = d.fb {
        format!("{fb:?}").to_ascii_uppercase()
    } else if let Some(s) = &d.strukt {
        s.clone()
    } else if let Some(cap) = d.string {
        format!("STRING[{cap}]")
    } else {
        d.ty.name().to_string()
    };
    if d.array.is_some() && d.string.is_none() {
        format!("ARRAY OF {base}")
    } else {
        base
    }
}

impl LibraryEntry {
    /// The documentation in the UI language (Turkish falls back to English).
    pub fn doc_in(&self, turkish: bool) -> &str {
        if turkish && !self.doc_tr.is_empty() {
            &self.doc_tr
        } else {
            &self.doc
        }
    }

    /// The call with every input named and set to a neutral default, so an inserted
    /// call compiles at once and shows the parameter names: `inst(IN := FALSE, PT := 0);`
    /// for a block, `NAME(A := 0, B := 0)` (an expression) for a function.
    pub fn call(&self, instance: &str) -> String {
        let args: Vec<String> = self
            .inputs
            .iter()
            .map(|p| format!("{} := {}", p.name, default_value(&p.ty)))
            .collect();
        if self.kind == "FUNCTION" {
            format!("{}({})", self.name, args.join(", "))
        } else {
            format!("{instance}({});", args.join(", "))
        }
    }

    /// A ready-to-edit snippet: the declaration line and the call statement. For a block the
    /// declaration is the instance; for a function it is a variable taking the result.
    pub fn template(&self, instance: &str) -> (String, String) {
        match &self.returns {
            Some(ty) => (
                format!("{instance} : {ty};"),
                format!("{instance} := {};", self.call(instance)),
            ),
            None => (format!("{instance} : {};", self.name), self.call(instance)),
        }
    }

    /// Inserts a new instance into an ST program: the declaration goes into the program's
    /// first `VAR` section (one is created after `PROGRAM` when there is none), the call in
    /// front of `END_PROGRAM` (or at the end). Returns the new source and the one-based
    /// line of the inserted call.
    pub fn insert_into(&self, source: &str) -> (String, usize) {
        let instance = unique_instance(source, &self.name);
        let (decl, call) = self.template(&instance);
        let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
        let word = |l: &str, w: &str| {
            l.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .any(|x| x.eq_ignore_ascii_case(w))
        };
        // The program starts after its PROGRAM line, or after the last block/type definition.
        let start = lines
            .iter()
            .position(|l| word(l, "PROGRAM"))
            .map(|i| i + 1)
            .or_else(|| {
                lines
                    .iter()
                    .rposition(|l| {
                        word(l, "END_FUNCTION_BLOCK") || word(l, "END_FUNCTION") || word(l, "END_TYPE")
                    })
                    .map(|i| i + 1)
            })
            .unwrap_or(0);
        match lines[start.min(lines.len())..]
            .iter()
            .position(|l| word(l, "END_VAR"))
        {
            Some(i) => lines.insert(start + i, format!("    {decl}")),
            None => {
                let at = start.min(lines.len());
                lines.splice(at..at, ["VAR".to_owned(), format!("    {decl}"), "END_VAR".to_owned()]);
            }
        }
        let call_at = lines
            .iter()
            .rposition(|l| word(l, "END_PROGRAM"))
            .unwrap_or(lines.len());
        lines.insert(call_at, call);
        let mut out = lines.join("\n");
        if source.ends_with('\n') {
            out.push('\n');
        }
        (out, call_at + 1)
    }
}

/// `motor1`, `motor2` …: the first name not already used as a word in `source`.
fn unique_instance(source: &str, block: &str) -> String {
    let base = block.to_ascii_lowercase();
    let used: std::collections::BTreeSet<String> = source
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map(|w| w.to_ascii_lowercase())
        .collect();
    (1..)
        .map(|n| format!("{base}{n}"))
        .find(|n| !used.contains(n))
        .expect("unbounded")
}

fn default_value(ty: &str) -> &'static str {
    match ty {
        "BOOL" => "FALSE",
        "REAL" => "0.0",
        _ => "0",
    }
}

/// A block's documentation: (upper-case name, English, Turkish).
type BlockDoc = (String, String, String);

fn clean(text: &str) -> String {
    text.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Strips the common leading indentation so list layout inside a comment survives.
fn dedent(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let indent = lines
        .iter()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| if i == 0 { l.trim_start() } else { l.get(indent..).unwrap_or(l.trim_start()) })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `(*! *)` / `(*!tr *)` comments keyed by the block that follows them.
fn docs(source: &str) -> Vec<BlockDoc> {
    let mut out = Vec::new();
    let (mut en, mut tr) = (String::new(), String::new());
    let mut rest = source;
    while let Some(start) = rest.find("(*!") {
        let after = &rest[start + 3..];
        let Some(end) = after.find("*)") else { break };
        let body = &after[..end];
        if let Some(t) = body.strip_prefix("tr").filter(|t| t.starts_with(char::is_whitespace)) {
            tr = clean(&dedent(t));
        } else {
            en = clean(&dedent(body));
        }
        rest = &after[end + 2..];
        let next = rest.trim_start();
        if next.starts_with("(*!") {
            continue;
        }
        let mut words = next.split_whitespace();
        let keyword = words.next().unwrap_or("").to_ascii_uppercase();
        if (keyword == "FUNCTION_BLOCK" || keyword == "FUNCTION")
            && let Some(name) = words.next()
        {
            let name: String = name
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            out.push((name.to_ascii_uppercase(), std::mem::take(&mut en), std::mem::take(&mut tr)));
        }
        en.clear();
        tr.clear();
    }
    out
}

/// Every library block with its parameters and documentation, by category then name.
pub fn library_entries() -> Vec<LibraryEntry> {
    let lib = defs();
    let mut out = Vec::new();
    for (category, source) in FILES {
        let documented = docs(source);
        let mut names: Vec<&String> = lib
            .fbs
            .keys()
            .filter(|n| documented.iter().any(|(d, ..)| d == *n))
            .collect();
        names.sort();
        for name in names {
            let def = &lib.fbs[name];
            let param = |n: &String| {
                let d = def
                    .decls
                    .iter()
                    .find(|d| d.names.iter().any(|(x, _)| x == n))
                    .expect("declared parameter");
                LibraryParam {
                    name: n.clone(),
                    ty: type_text(d),
                }
            };
            out.push(LibraryEntry {
                name: name.clone(),
                kind: if def.returns.is_some() {
                    "FUNCTION"
                } else {
                    "FUNCTION_BLOCK"
                },
                category,
                doc: documented
                    .iter()
                    .find(|(d, ..)| d == name)
                    .map(|(_, en, _)| en.clone())
                    .unwrap_or_default(),
                doc_tr: documented
                    .iter()
                    .find(|(d, ..)| d == name)
                    .map(|(.., tr)| tr.clone())
                    .unwrap_or_default(),
                inputs: def.inputs.iter().map(param).collect(),
                outputs: def.outputs.iter().map(param).collect(),
                returns: def.returns.map(|t| t.name().to_string()),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_library_file_parses() {
        assert!(library_errors().is_empty(), "{:?}", library_errors());
    }

    #[test]
    fn every_library_block_is_documented_once() {
        let entries = library_entries();
        assert_eq!(entries.len(), defs().fbs.len(), "a block lacks a (*! *) doc comment");
        for e in &entries {
            assert!(!e.doc.is_empty(), "{} has an empty doc", e.name);
        }
    }

    #[test]
    fn every_block_is_documented_in_both_languages_with_all_parameters() {
        // Report every offender (with its category) so authors see only their own blocks.
        let mut problems = Vec::new();
        for e in library_entries() {
            if e.doc_tr.is_empty() {
                problems.push(format!("{}/{}: missing (*!tr *) doc", e.category, e.name));
            }
            for p in e.inputs.iter().chain(&e.outputs) {
                for (lang, doc) in [("en", &e.doc), ("tr", &e.doc_tr)] {
                    let named = doc
                        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                        .any(|w| w.eq_ignore_ascii_case(&p.name));
                    if !doc.is_empty() && !named {
                        problems.push(format!("{}/{}: {lang} doc does not name {}", e.category, e.name, p.name));
                    }
                }
            }
        }
        assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
    }

    #[test]
    fn templates_name_every_input() {
        for e in library_entries() {
            let (_, call) = e.template("x1");
            for p in &e.inputs {
                assert!(call.contains(&p.name), "{}: {call}", e.name);
            }
        }
    }

    #[test]
    fn every_block_inserted_into_the_st_template_compiles() {
        // The exact path of the library browser's "insert" button.
        for e in library_entries() {
            let (src, line) = e.insert_into(crate::project::ST_TEMPLATE);
            assert!(src.lines().nth(line - 1).unwrap().contains(&e.name.to_ascii_lowercase()) || src.lines().nth(line - 1).unwrap().contains(&e.name), "{}: line {line}\n{src}", e.name);
            if let Err(err) = crate::plc::compile_st(&src) {
                panic!("{} inserted does not compile: {err:?}\n{src}", e.name);
            }
        }
    }

    #[test]
    fn insertion_places_declaration_and_call_and_picks_free_names() {
        let entry = LibraryEntry {
            name: "R_TRIG".into(),
            kind: "FUNCTION_BLOCK",
            category: "logic",
            doc: String::new(),
            doc_tr: String::new(),
            inputs: vec![LibraryParam { name: "CLK".into(), ty: "BOOL".into() }],
            outputs: vec![],
            returns: None,
        };
        let src = "PROGRAM Main\nVAR\n    r_trig1 : R_TRIG;\nEND_VAR\nY0 := X0;\nEND_PROGRAM\n";
        let (out, line) = entry.insert_into(src);
        assert_eq!(
            out,
            "PROGRAM Main\nVAR\n    r_trig1 : R_TRIG;\n    r_trig2 : R_TRIG;\nEND_VAR\nY0 := X0;\nr_trig2(CLK := FALSE);\nEND_PROGRAM\n"
        );
        assert_eq!(line, 7);
        // No VAR section and no PROGRAM wrapper: a VAR section is created at the top.
        let (out, line) = entry.insert_into("Y0 := X0;");
        assert_eq!(out, "VAR\n    r_trig1 : R_TRIG;\nEND_VAR\nY0 := X0;\nr_trig1(CLK := FALSE);");
        assert_eq!(line, 5);
        // A user block's own VAR section is skipped.
        let (out, _) = entry.insert_into("FUNCTION_BLOCK F\nVAR a : INT; END_VAR\nEND_FUNCTION_BLOCK\nPROGRAM P\nEND_PROGRAM");
        assert!(out.contains("PROGRAM P\nVAR\n    r_trig1 : R_TRIG;\nEND_VAR\nr_trig1(CLK := FALSE);\nEND_PROGRAM"), "{out}");
    }

    #[test]
    fn docs_pair_english_and_turkish_comments() {
        let d = docs("(*! Adds.\n    Inputs:\n    - A : INT *)\n(*!tr Toplar. *)\nFUNCTION ADD2 : INT\n(*! Other. *)\nFUNCTION_BLOCK FB1");
        assert_eq!(d.len(), 2);
        assert_eq!(d[0], ("ADD2".into(), "Adds.\nInputs:\n- A : INT".into(), "Toplar.".into()));
        assert_eq!(d[1], ("FB1".into(), "Other.".into(), String::new()));
    }

    #[test]
    fn every_library_block_compiles_when_used() {
        // Instantiate/call each block with default inputs: the inlined code must lower
        // and compile to IL on its own.
        for e in library_entries() {
            let src = if e.kind == "FUNCTION" {
                let args = e
                    .inputs
                    .iter()
                    .map(|p| match p.ty.as_str() {
                        "BOOL" => "FALSE",
                        "REAL" => "0.0",
                        _ => "0",
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "PROGRAM P\nVAR r : {}; END_VAR\nr := {}({args});\nEND_PROGRAM",
                    e.returns.as_deref().unwrap(),
                    e.name
                )
            } else {
                format!("PROGRAM P\nVAR b : {}; END_VAR\nb();\nEND_PROGRAM", e.name)
            };
            if let Err(err) = crate::plc::compile_st(&src) {
                panic!("{} does not compile when used: {err:?}\n{src}", e.name);
            }
        }
    }

    #[test]
    fn a_program_may_redefine_a_library_block() {
        let Some(e) = library_entries().into_iter().find(|e| e.kind == "FUNCTION_BLOCK") else {
            return;
        };
        let src = format!(
            "FUNCTION_BLOCK {}\nVAR_OUTPUT mine : INT; END_VAR\nmine := 7;\nEND_FUNCTION_BLOCK\nPROGRAM P\nVAR b : {}; r AT D20 : INT; END_VAR\nb(); r := b.mine;\nEND_PROGRAM",
            e.name, e.name
        );
        let program = crate::plc::compile_st(&src).unwrap();
        let mut sim = crate::plc::Simulator::default();
        sim.scan(&program, 100);
        assert_eq!(sim.memory.word("D20".parse().unwrap()), 7);
    }

    /// `TON_TICKS` (PT in raw 100 ms ticks, no range check) is for library code only.
    #[test]
    fn a_program_cannot_declare_the_library_internal_tick_timer() {
        let src = "PROGRAM P\nVAR t : TON_TICKS; END_VAR\nt(IN := TRUE, PT := 5);\nEND_PROGRAM";
        assert!(crate::plc::compile_st(src).is_err());
    }
}
