//! UI language: Turkish texts are the source strings, English comes from `i18n_en::EN`.
//!
//! `tr("Kaydet")` returns the text for the active language; `tfmt!("tr", "en", args..)`
//! is `format!` with a Turkish and an English format string.
use crate::i18n_en::EN;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Tr,
    En,
}

impl Lang {
    /// Locale code understood by the component library (`rust-i18n`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::Tr => "tr",
            Lang::En => "en",
        }
    }
    pub fn other(self) -> Lang {
        match self {
            Lang::Tr => Lang::En,
            Lang::En => Lang::Tr,
        }
    }
}

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn lang() -> Lang {
    if is_en() { Lang::En } else { Lang::Tr }
}

pub fn is_en() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn set_lang(lang: Lang) {
    ENGLISH.store(lang == Lang::En, Ordering::Relaxed);
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| EN.iter().copied().collect())
}

/// The text for the active language; texts without a translation stay as written.
pub fn tr(text: &'static str) -> &'static str {
    if is_en() {
        table().get(text).copied().unwrap_or(text)
    } else {
        text
    }
}

/// `format!` with a Turkish and an English format string (same arguments for both).
#[macro_export]
macro_rules! tfmt {
    ($tr:literal, $en:literal $(, $($args:tt)*)?) => {
        if $crate::i18n::is_en() {
            format!($en $(, $($args)*)?)
        } else {
            format!($tr $(, $($args)*)?)
        }
    };
}

fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("OpenDVP").join("language.txt"))
}

/// The language saved by a previous run, if any.
pub fn saved() -> Option<Lang> {
    match std::fs::read_to_string(settings_path()?).ok()?.trim() {
        "en" => Some(Lang::En),
        "tr" => Some(Lang::Tr),
        _ => None,
    }
}

/// Remember the language for the next run; failures are ignored (it is only a preference).
pub fn save(lang: Lang) {
    if let Some(path) = settings_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, lang.code());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_only_in_english() {
        set_lang(Lang::Tr);
        assert_eq!(tr("Kaydet"), "Kaydet");
        set_lang(Lang::En);
        assert_eq!(tr("Kaydet"), "Save");
        assert_eq!(tr("henüz çevrilmemiş metin"), "henüz çevrilmemiş metin");
        let n = 3;
        assert_eq!(tfmt!("{n} ağ", "{n} networks"), "3 networks");
        set_lang(Lang::Tr);
        assert_eq!(tfmt!("{n} ağ", "{n} networks"), "3 ağ");
    }

    #[test]
    fn every_translation_keeps_its_placeholders() {
        let count = |s: &str| s.matches('{').count();
        for (tr, en) in EN {
            assert_eq!(count(tr), count(en), "placeholder count differs: {tr}");
        }
    }
}
