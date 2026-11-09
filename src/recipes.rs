//! Project-owned parameter sets. Preparing a recipe validates every row before
//! any local memory is changed. This module has no PLC transport or force API.
use crate::{
    plc::{Area, Device, Memory},
    plc_monitor,
    project::{self, Project},
    st::DataType,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_RECIPES: usize = 32;
pub const MAX_ENTRIES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    /// A D/M address or a project-global symbol with an assigned address.
    pub target: String,
    pub ty: DataType,
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub name: String,
    #[serde(default)]
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone)]
pub struct Prepared {
    pub device: Device,
    pub ty: DataType,
    pub words: Vec<u16>,
}

fn text(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.chars().count() <= max && !text.chars().any(char::is_control)
}

/// Validate the stored document independently of address resolution. Deleting a
/// symbol must not make it impossible to save a project and repair its recipe.
pub fn validate(recipes: &[Recipe]) -> Result<(), String> {
    if recipes.len() > MAX_RECIPES {
        return Err("En çok 32 reçete saklanabilir.".into());
    }
    let mut names = BTreeSet::new();
    for recipe in recipes {
        if !text(&recipe.name, 64) || !names.insert(recipe.name.to_lowercase()) {
            return Err("Reçete adı boş, yinelenmiş veya 64 karakterden uzun.".into());
        }
        recipe.validate()?;
    }
    Ok(())
}

fn scalar(ty: DataType, words: &[u16]) -> f64 {
    match plc_monitor::decode(ty, words) {
        plc_monitor::Value::Bool(v) => f64::from(u8::from(v)),
        plc_monitor::Value::Int(v) => f64::from(v),
        plc_monitor::Value::Dint(v) => f64::from(v),
        plc_monitor::Value::Word(v) => f64::from(v),
        plc_monitor::Value::Dword(v) => f64::from(v),
        plc_monitor::Value::Real(v) => f64::from(v),
    }
}

impl Parameter {
    fn words(&self) -> Result<Vec<u16>, String> {
        let words = project::parse_initial(self.ty, &self.value)?;
        let bound = |value: &Option<String>| -> Result<Option<f64>, String> {
            value
                .as_ref()
                .map(|v| project::parse_initial(self.ty, v).map(|w| scalar(self.ty, &w)))
                .transpose()
        };
        let (min, max) = (bound(&self.min)?, bound(&self.max)?);
        if let (Some(min), Some(max)) = (min, max)
            && min > max
        {
            return Err("Alt sınır üst sınırdan büyük olamaz.".into());
        }
        let value = scalar(self.ty, &words);
        if min.is_some_and(|min| value < min) || max.is_some_and(|max| value > max) {
            return Err("Değer tanımlanan alt/üst sınırın dışında.".into());
        }
        Ok(words)
    }

    pub fn resolve(&self, project: &Project) -> Result<Prepared, String> {
        let target = self.target.trim();
        let device = if let Ok(device) = target.parse::<Device>() {
            device
        } else {
            let symbol = project
                .symbols
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(target))
                .map(|(_, s)| s)
                .ok_or_else(|| format!("Adres veya proje sembolü bulunamadı: {target}"))?;
            if symbol.ty != self.ty {
                return Err(format!(
                    "{target}: sembol tipi {}, reçete tipi {}.",
                    symbol.ty, self.ty
                ));
            }
            symbol
                .device
                .as_deref()
                .ok_or_else(|| format!("{target} sembolüne henüz adres atanmadı."))?
                .parse()?
        };
        validate_target(device, self.ty)?;
        Ok(Prepared {
            device,
            ty: self.ty,
            words: self.words()?,
        })
    }
}

/// Parameter storage uses general-purpose M bits and D words. I/O, timer/counter
/// state and special registers are deliberately outside parameter storage.
pub fn validate_target(device: Device, ty: DataType) -> Result<(), String> {
    let bit = ty == DataType::Bool;
    if device.area != if bit { Area::M } else { Area::D } {
        return Err(format!(
            "{ty} parametresi {} alanında olmalı.",
            if bit { "M" } else { "D" }
        ));
    }
    let width = if bit { 1 } else { ty.words() as u16 };
    let end = device
        .index
        .checked_add(width - 1)
        .ok_or("Adres aralığı taşıyor.")?;
    let limit = if bit { 4095 } else { 9999 };
    if end > limit || device.index <= 1999 && end >= 1000 {
        return Err(format!(
            "{device}: parametre alanı sınırı veya özel sistem alanı aşılıyor."
        ));
    }
    Ok(())
}

impl Recipe {
    pub fn validate(&self) -> Result<(), String> {
        if !text(&self.name, 64) || self.parameters.len() > MAX_ENTRIES {
            return Err("Reçete adı geçersiz veya parametre sayısı 256 sınırını aşıyor.".into());
        }
        let mut names = BTreeSet::new();
        for p in &self.parameters {
            if !text(&p.name, 64)
                || !names.insert(p.name.to_lowercase())
                || !text(&p.target, 64)
                || p.unit.chars().count() > 24
                || p.unit.chars().any(char::is_control)
                || [
                    &p.value,
                    p.min.as_ref().unwrap_or(&p.value),
                    p.max.as_ref().unwrap_or(&p.value),
                ]
                .iter()
                .any(|s| s.len() > 64)
            {
                return Err(
                    "Parametre adı/hedefi geçersiz, yinelenmiş veya metin sınırı aşılmış.".into(),
                );
            }
            p.words().map_err(|e| format!("{}: {e}", p.name))?;
        }
        Ok(())
    }

    pub fn prepare(&self, project: &Project) -> Result<Vec<Prepared>, String> {
        self.validate()?;
        let mut occupied = BTreeSet::new();
        let mut out = Vec::new();
        for p in &self.parameters {
            let prepared = p.resolve(project).map_err(|e| format!("{}: {e}", p.name))?;
            for offset in 0..prepared.words.len() as u16 {
                if !occupied.insert((prepared.device.area, prepared.device.index + offset)) {
                    return Err(format!(
                        "{}: {} hedefi başka parametreyle çakışıyor.",
                        p.name, prepared.device
                    ));
                }
            }
            out.push(prepared);
        }
        Ok(out)
    }

    pub fn apply(&self, project: &Project, memory: &mut Memory) -> Result<usize, String> {
        let prepared = self.prepare(project)?;
        for p in &prepared {
            if p.ty == DataType::Bool {
                memory.write_bit(p.device, p.words[0] != 0);
            } else {
                for (offset, word) in p.words.iter().enumerate() {
                    memory.set_word(
                        Device {
                            index: p.device.index + offset as u16,
                            ..p.device
                        },
                        *word as i16,
                    );
                }
            }
        }
        Ok(prepared.len())
    }

    pub fn capture(&self, project: &Project, memory: &Memory) -> Result<Self, String> {
        let prepared = self.prepare(project)?;
        let mut next = self.clone();
        for (p, resolved) in next.parameters.iter_mut().zip(prepared) {
            p.value = plc_monitor::read_memory(memory, &[(resolved.device, p.ty)])[0].to_string();
        }
        next.validate()?;
        Ok(next)
    }

    pub fn export(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn import(json: &str) -> Result<Self, String> {
        if json.len() > 256 * 1024 {
            return Err("Reçete dosyası 256 KiB sınırını aşıyor.".into());
        }
        let recipe: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        recipe.validate()?;
        Ok(recipe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn param(target: &str, ty: DataType, value: &str) -> Parameter {
        Parameter {
            name: target.into(),
            target: target.into(),
            ty,
            value: value.into(),
            unit: String::new(),
            min: None,
            max: None,
        }
    }
    fn recipe(parameters: Vec<Parameter>) -> Recipe {
        Recipe {
            name: "Ürün A".into(),
            parameters,
        }
    }

    #[test]
    fn typed_values_roundtrip_through_memory_and_project() {
        let r = recipe(vec![
            param("D0", DataType::Dint, "-100000"),
            param("D2", DataType::Real, "12,5"),
            param("M0", DataType::Bool, "TRUE"),
            param("D4", DataType::Dword, "4294967295"),
        ]);
        let mut p = Project::default();
        let mut m = Memory::default();
        r.apply(&p, &mut m).unwrap();
        let captured = r.capture(&p, &m).unwrap();
        assert_eq!(captured.parameters[0].value, "-100000");
        assert_eq!(captured.parameters[1].value, "12.5");
        assert_eq!(captured.parameters[3].value, "16#FFFFFFFF");
        p.recipes.push(captured.clone());
        let loaded: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(loaded.recipes[0], captured);
        assert_eq!(Recipe::import(&r.export().unwrap()).unwrap(), r);
    }
    #[test]
    fn bad_late_row_does_not_partially_write_memory() {
        let p = Project::default();
        let mut m = Memory::default();
        m.set_word("D0".parse().unwrap(), 42);
        let r = recipe(vec![
            param("D0", DataType::Int, "10"),
            param("D2", DataType::Int, "32768"),
        ]);
        assert!(r.apply(&p, &mut m).is_err());
        assert_eq!(m.word("D0".parse().unwrap()), 42);
    }
    #[test]
    fn rejects_overlap_special_areas_and_wide_boundary() {
        let p = Project::default();
        for parameters in [
            vec![
                param("D0", DataType::Dint, "1"),
                param("D1", DataType::Int, "2"),
            ],
            vec![param("D9999", DataType::Real, "1")],
            vec![param("D999", DataType::Dint, "1")],
            vec![param("Y0", DataType::Bool, "1")],
            vec![param("M1000", DataType::Bool, "1")],
        ] {
            assert!(recipe(parameters).prepare(&p).is_err());
        }
    }
    #[test]
    fn limits_and_capture_are_validated_atomically() {
        let mut a = param("D0", DataType::Int, "50");
        a.min = Some("0".into());
        a.max = Some("100".into());
        let r = recipe(vec![a]);
        let p = Project::default();
        let mut m = Memory::default();
        m.set_word("D0".parse().unwrap(), 101);
        assert!(r.capture(&p, &m).is_err());
        assert_eq!(r.parameters[0].value, "50");
        let mut invalid = r.clone();
        invalid.parameters[0].value = "-1".into();
        assert!(invalid.prepare(&p).is_err());
    }
    #[test]
    fn symbol_resolution_tracks_new_address_and_missing_symbol_does_not_block_save() {
        let mut p = Project::default();
        let r = recipe(vec![param("Speed", DataType::Int, "50")]);
        p.recipes.push(r.clone());
        assert!(p.validate().is_ok());
        assert!(r.prepare(&p).is_err());
        p.set_symbol(
            "Speed",
            project::Symbol {
                ty: DataType::Int,
                device: Some("D20".into()),
                auto: false,
                retain: false,
                comment: String::new(),
                init: None,
            },
        )
        .unwrap();
        assert_eq!(r.prepare(&p).unwrap()[0].device.to_string(), "D20");
    }

    #[test]
    fn a_large_collection_cannot_save_an_unloadable_project() {
        let mut p = Project::default();
        let parameters: Vec<_> = (0..MAX_ENTRIES)
            .map(|i| {
                let mut entry = param(&format!("D{i}"), DataType::Int, "10");
                entry.name = format!("{i:03} {}", "p".repeat(60));
                entry.unit = "u".repeat(24);
                entry.min = Some("0".into());
                entry.max = Some("32767".into());
                entry
            })
            .collect();
        p.recipes = (0..MAX_RECIPES)
            .map(|i| Recipe {
                name: format!("Recipe {i}"),
                parameters: parameters.clone(),
            })
            .collect();
        p.validate().unwrap();
        assert!(serde_json::to_string_pretty(&p).unwrap().len() > 1024 * 1024);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("opendvp-recipes-{unique}.odvp"));
        assert!(p.save(&path).unwrap_err().contains("1 MiB"));
        assert!(!path.exists());
    }

    #[test]
    fn example_and_legacy_projects_load_with_recipe_compatibility() {
        let p: Project = serde_json::from_str(include_str!("../examples/recipes.odvp")).unwrap();
        p.validate().unwrap();
        p.compile().unwrap();
        assert_eq!(p.recipes.len(), 2);
        for recipe in &p.recipes {
            recipe.prepare(&p).unwrap();
        }
        let mut json = serde_json::to_value(Project::default()).unwrap();
        json.as_object_mut().unwrap().remove("recipes");
        assert!(
            serde_json::from_value::<Project>(json)
                .unwrap()
                .recipes
                .is_empty()
        );
    }
}
