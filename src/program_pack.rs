//! Program packages (`.odprog`): programs exported from one project and imported into
//! another, and the user library folder they can be kept in. Delta editor's POU export/import and
//! user library do the same; an imported program arrives disabled ("not assigned to a task")
//! so it cannot change the running program until the user enables it.
use crate::project::{Project, SourceLanguage, Symbol, Unit, UnitKind};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const FORMAT: &str = "opendvp-programs";
const VERSION: u32 = 1;
const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Package {
    format: String,
    version: u32,
    units: Vec<Unit>,
    /// Symbols the programs use by name, with the devices they stand for.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    symbols: BTreeMap<String, Symbol>,
}

/// What an import did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Imported {
    /// Names the programs have in the project (renamed when they clashed).
    pub programs: Vec<String>,
    /// Anything the user should know: renamed programs, renumbered pointers, symbols kept.
    pub notes: Vec<String>,
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
}

/// Export programs `indices` (0 = Main, exported as a cyclic program) with the symbols they
/// mention.
pub fn export(project: &Project, indices: &[usize]) -> Result<String, String> {
    if indices.is_empty() {
        return Err("Dışa aktarılacak program seçilmedi".into());
    }
    let mut units = Vec::new();
    for &index in indices {
        if index >= project.program_count() {
            return Err("Geçersiz program".into());
        }
        if index == 0 {
            let name: String = project
                .name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            let name = if crate::project::check_unit_name(&name) {
                name
            } else {
                "Main_part".to_owned()
            };
            units.push(Unit {
                name,
                kind: UnitKind::Cyclic,
                number: 0,
                source: project.source.clone(),
                enabled: true,
                language: project.language,
            });
        } else {
            units.push(project.units[index - 1].clone());
        }
    }
    let mentioned: std::collections::BTreeSet<String> =
        units.iter().flat_map(|u| words(&u.source)).collect();
    let symbols = project
        .symbols
        .iter()
        .filter(|(name, symbol)| {
            symbol.device.is_some() && mentioned.contains(&name.to_ascii_lowercase())
        })
        .map(|(name, symbol)| (name.clone(), symbol.clone()))
        .collect();
    serde_json::to_string_pretty(&Package {
        format: FORMAT.into(),
        version: VERSION,
        units,
        symbols,
    })
    .map_err(|e| e.to_string())
}

/// Replace `CALL P<from>` / `CALLP P<from>` by the new pointer in IL text.
fn renumber_calls(source: &str, map: &BTreeMap<u16, u16>) -> String {
    if map.is_empty() {
        return source.to_owned();
    }
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let (code, comment) = line.split_at(line.find(';').unwrap_or(line.len()));
        let mut tokens: Vec<&str> = code.split_whitespace().collect();
        let replaced = if tokens.len() == 2
            && ["CALL", "CALLP"].contains(&tokens[0].to_ascii_uppercase().as_str())
        {
            tokens[1]
                .strip_prefix(['P', 'p'])
                .and_then(|n| n.parse::<u16>().ok())
                .and_then(|n| map.get(&n))
                .map(|new| format!("P{new}"))
        } else {
            None
        };
        match replaced {
            Some(pointer) => {
                tokens[1] = &pointer;
                let indent = &code[..code.len() - code.trim_start().len()];
                out.push_str(&format!("{indent}{}{comment}", tokens.join(" ")));
            }
            None => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

/// Import a package into `project`. Nothing changes when anything is wrong: the import is
/// applied to a copy that replaces the project only on success.
pub fn import(project: &mut Project, json: &str) -> Result<Imported, String> {
    if json.len() as u64 > MAX_BYTES {
        return Err("Paket 1 MiB sınırını aşıyor".into());
    }
    let package: Package =
        serde_json::from_str(json).map_err(|e| format!("Program paketi okunamadı: {e}"))?;
    if package.format != FORMAT || package.version != VERSION {
        return Err("Bu bir OpenDVP program paketi değil (ya da sürümü desteklenmiyor)".into());
    }
    if package.units.is_empty() {
        return Err("Pakette program yok".into());
    }
    let mut work = project.clone();
    let mut report = Imported::default();
    let mut renumbered: BTreeMap<u16, u16> = BTreeMap::new();
    let mut imported: Vec<usize> = Vec::new();
    for mut unit in package.units {
        if !crate::project::check_unit_name(&unit.name) {
            return Err(format!("Paketteki program adı geçersiz: {}", unit.name));
        }
        // A free name: the package's own, or with a counter.
        let original = unit.name.clone();
        let taken = |work: &Project, name: &str| {
            name.eq_ignore_ascii_case("main")
                || work.units.iter().any(|u| u.name.eq_ignore_ascii_case(name))
        };
        let mut name = original.clone();
        let mut counter = 2;
        while taken(&work, &name) {
            name = format!("{original}_{counter}");
            counter += 1;
        }
        if name != original {
            report.notes.push(format!("\"{original}\" adı kullanımda; \"{name}\" olarak alındı"));
        }
        unit.name = name.clone();
        unit.enabled = false;
        // Pointers: a subroutine takes a free P; an interrupt label cannot move.
        match unit.kind {
            UnitKind::Subroutine => {
                let clash = work
                    .units
                    .iter()
                    .any(|u| u.kind == UnitKind::Subroutine && u.number == unit.number)
                    || work.cj_pointer_numbers().contains(&unit.number);
                if clash {
                    let old = unit.number;
                    let probe = work.add_unit(UnitKind::Subroutine)?;
                    let new = work.units[probe - 1].number;
                    work.remove_unit(probe);
                    renumbered.insert(old, new);
                    unit.number = new;
                    report
                        .notes
                        .push(format!("{name}: P{old} kullanımda, P{new} verildi"));
                }
            }
            UnitKind::Interrupt => {
                if let Some(other) = work
                    .units
                    .iter()
                    .find(|u| u.kind == UnitKind::Interrupt && u.number == unit.number)
                {
                    return Err(format!(
                        "{} kesme etiketi \"{}\" programında kullanımda; önce onu silin ya da numarasını değiştirin",
                        unit.kind.pointer(unit.number),
                        other.name
                    ));
                }
            }
            UnitKind::Cyclic => {}
        }
        if work.units.len() >= 64 {
            return Err("En çok 64 ek program olabilir".into());
        }
        work.units.push(unit);
        imported.push(work.units.len());
        report.programs.push(name);
    }
    // CALLs between the imported subroutines follow their new pointers.
    for index in imported {
        let unit = &mut work.units[index - 1];
        if unit.language == SourceLanguage::Il {
            unit.source = renumber_calls(&unit.source, &renumbered);
        }
    }
    for (name, symbol) in package.symbols {
        match work.symbols.get(&name) {
            Some(existing) if existing.device == symbol.device => {}
            Some(_) => report.notes.push(format!(
                "\"{name}\" sembolü projede başka bir adresle var; projedeki korundu"
            )),
            None => match work.set_symbol(&name, symbol) {
                Ok(()) => {}
                Err(e) => report.notes.push(format!("\"{name}\" sembolü alınamadı: {e}")),
            },
        }
    }
    work.validate()?;
    *project = work;
    Ok(report)
}

/// `%LOCALAPPDATA%\OpenDVP\library` (or the XDG/HOME equivalent).
pub fn library_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("OpenDVP").join("library"))
}

/// Package files in `dir` as (display name, path), sorted by name.
pub fn library_list_in(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut list: Vec<(String, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e.eq_ignore_ascii_case("odprog")))
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().into_owned();
            Some((name, path))
        })
        .collect();
    list.sort_by_key(|(name, _)| name.to_lowercase());
    list
}

/// Save a package into the library under `name` (letters, digits, `_`, `-`, space).
pub fn library_save_in(dir: &Path, name: &str, json: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '))
    {
        return Err("Kütüphane adı harf, rakam, boşluk, _ ve - içermeli".into());
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{name}.odprog"));
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Read a package file, refusing oversized ones.
pub fn read_file(path: &Path) -> Result<String, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("Paket 1 MiB sınırını aşıyor".into());
    }
    fs::read_to_string(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with_units() -> Project {
        let mut p = Project {
            source: "LD X0\nCALL Pump\nEND\n".into(),
            ..Project::default()
        };
        let sub = p.add_unit(UnitKind::Subroutine).unwrap();
        p.rename_unit(sub, "Pump").unwrap();
        p.set_program_source(sub, "LD M1000\nOUT Y1\nEND\n".into());
        let int = p.add_unit(UnitKind::Interrupt).unwrap();
        p.set_program_source(int, "LD M1000\nINC D10\nEND\n".into());
        p
    }

    #[test]
    fn a_program_survives_export_and_import_disabled() {
        let source = project_with_units();
        let json = export(&source, &[1]).unwrap();
        let mut target = Project::default();
        let done = import(&mut target, &json).unwrap();
        assert_eq!(done.programs, vec!["Pump"]);
        assert_eq!(target.units.len(), 1);
        assert_eq!(target.units[0].source, "LD M1000\nOUT Y1\nEND\n");
        assert!(!target.units[0].enabled, "imported programs wait to be enabled");
        target.validate().unwrap();
    }
    #[test]
    fn main_is_exported_as_a_cyclic_part_with_its_symbols() {
        use crate::st::DataType::Bool;
        let mut source = Project {
            name: "Motor line".into(),
            source: "LD Start\nOUT Y0\nEND\n".into(),
            ..Project::default()
        };
        let symbol = |d: &str| Symbol {
            ty: Bool,
            device: Some(d.into()),
            auto: false,
            retain: false,
            comment: String::new(),
            init: None,
        };
        source.set_symbol("Start", symbol("X0")).unwrap();
        source.set_symbol("Unused", symbol("X5")).unwrap();
        let json = export(&source, &[0]).unwrap();
        assert!(json.contains("Start") && !json.contains("Unused"), "only mentioned symbols travel");
        let mut target = Project::default();
        let done = import(&mut target, &json).unwrap();
        assert_eq!(done.programs, vec!["Motor_line"]);
        assert_eq!(target.units[0].kind, UnitKind::Cyclic);
        assert!(target.symbols.contains_key("Start"));
        // The symbol the package brought makes the program compile in the new project.
        target.set_unit_enabled(1, true);
        assert!(target.compile().is_ok(), "{:?}", target.compile().err());
    }
    #[test]
    fn clashing_names_and_pointers_are_renamed_and_calls_follow() {
        let mut target = project_with_units();
        // Package: a subroutine P0 named Pump that calls a second one by pointer.
        let mut source = Project::default();
        let a = source.add_unit(UnitKind::Subroutine).unwrap();
        source.rename_unit(a, "Pump").unwrap();
        source.set_program_source(a, "LD M1000\nCALL P1\nEND\n".into());
        let b = source.add_unit(UnitKind::Subroutine).unwrap();
        source.rename_unit(b, "Helper").unwrap();
        source.set_program_source(b, "LD M1000\nINC D20\nEND\n".into());
        let json = export(&source, &[a, b]).unwrap();
        let done = import(&mut target, &json).unwrap();
        assert_eq!(done.programs, vec!["Pump_2", "Helper"]);
        let by = |n: &str| target.units.iter().find(|u| u.name == n).unwrap();
        assert_eq!(by("Pump_2").number, 1, "P0 was taken by the existing Pump");
        assert_eq!(by("Helper").number, 2, "its own P1 is now Pump_2's, so it moves on");
        // The package's `CALL P1` meant Helper, so it follows Helper to P2.
        assert!(by("Pump_2").source.contains("CALL P2"), "{}", by("Pump_2").source);
        assert!(done.notes.iter().any(|n| n.contains("Pump_2")));
        target.validate().unwrap();
    }
    #[test]
    fn a_taken_interrupt_label_stops_the_import_and_changes_nothing() {
        let mut target = project_with_units();
        let source = project_with_units();
        let json = export(&source, &[2]).unwrap();
        let before = target.units.clone();
        let error = import(&mut target, &json).unwrap_err();
        assert!(error.contains("I001"), "{error}");
        assert_eq!(target.units, before);
    }
    #[test]
    fn bad_packages_are_refused() {
        let mut p = Project::default();
        assert!(import(&mut p, "not json").is_err());
        assert!(import(&mut p, r#"{"format":"other","version":1,"units":[]}"#).is_err());
        assert!(import(&mut p, r#"{"format":"opendvp-programs","version":1,"units":[]}"#).is_err());
        let bad = r#"{"format":"opendvp-programs","version":1,"units":[{"name":"bad name","kind":"cyclic","number":0,"source":"END\n"}]}"#;
        assert!(import(&mut p, bad).is_err());
        assert!(export(&p, &[]).is_err());
        assert!(export(&p, &[3]).is_err());
    }
    #[test]
    fn the_library_lists_and_saves_packages() {
        let dir = std::env::temp_dir().join(format!("opendvp-library-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert!(library_list_in(&dir).is_empty());
        let json = export(&project_with_units(), &[1]).unwrap();
        let path = library_save_in(&dir, "Pump control", &json).unwrap();
        library_save_in(&dir, "Alarm", &json).unwrap();
        assert!(library_save_in(&dir, "../evil", &json).is_err());
        let names: Vec<String> = library_list_in(&dir).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["Alarm", "Pump control"]);
        assert_eq!(read_file(&path).unwrap(), json);
        fs::remove_dir_all(&dir).unwrap();
    }
}
