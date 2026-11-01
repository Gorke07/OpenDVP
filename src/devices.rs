//! Delta DVP models offered when a project is created.
//!
//! Program capacities marked `verified` were read from the Delta editor simulators
//! (D1002: ES2/SE 15872, SS2 7920, EH3 30000 steps); the others are the family values.

#[derive(Debug, PartialEq, Eq)]
pub struct PlcModel {
    /// Short id stored in the project (`ES2`).
    pub id: &'static str,
    pub title: &'static str,
    pub family: &'static str,
    pub summary: &'static str,
    pub steps: u32,
    pub steps_verified: bool,
    pub io: &'static str,
    pub comm: &'static str,
    /// Column of Delta's special M/D support table (`special_registers`).
    pub special_column: &'static str,
    /// Delta editor simulator that runs this model's programs, if one is installed with Delta editor.
    pub simulator: Option<&'static str>,
}

// Positional constructor keeps the catalog table below compact and aligned.
#[allow(clippy::too_many_arguments)]
const fn model(
    id: &'static str,
    title: &'static str,
    family: &'static str,
    summary: &'static str,
    steps: u32,
    steps_verified: bool,
    io: &'static str,
    comm: &'static str,
    special_column: &'static str,
    simulator: Option<&'static str>,
) -> PlcModel {
    PlcModel {
        id,
        title,
        family,
        summary,
        steps,
        steps_verified,
        io,
        comm,
        special_column,
        simulator,
    }
}

pub const DEFAULT: &str = "ES2";

pub const FAMILIES: &[&str] = &["Standart tip", "İnce (slim) tip", "Yüksek performans"];

pub const MODELS: &[PlcModel] = &[
    model(
        "ES2",
        "DVP-ES2",
        "Standart tip",
        "Genel amaçlı standart PLC; en yaygın Delta DVP modeli.",
        15872,
        true,
        "Dahili 16–60 G/Ç, genişleme modülleriyle artırılabilir",
        "RS-232 + 2 × RS-485 (Modbus ASCII/RTU)",
        "ES2",
        Some("DVPSimulator_ES2.exe"),
    ),
    model(
        "EX2",
        "DVP-EX2",
        "Standart tip",
        "ES2 tabanlı, dahili analog giriş/çıkışlı model.",
        15872,
        false,
        "Dahili dijital G/Ç + 4 analog giriş / 2 analog çıkış",
        "RS-232 + 2 × RS-485",
        "EX2",
        None,
    ),
    model(
        "ES2-E",
        "DVP-ES2-E",
        "Standart tip",
        "Dahili Ethernet portlu ES2.",
        15872,
        false,
        "Dahili 16–60 G/Ç",
        "Ethernet (Modbus TCP) + RS-232 + RS-485",
        "ES2",
        None,
    ),
    model(
        "SS2",
        "DVP-SS2",
        "İnce (slim) tip",
        "Temel ince PLC; küçük makineler için.",
        7920,
        true,
        "8 dijital giriş / 6 dijital çıkış, sağ ve sol genişleme",
        "RS-232 + RS-485",
        "SS2",
        Some("DVPSimulator_SS2.exe"),
    ),
    model(
        "SA2",
        "DVP-SA2",
        "İnce (slim) tip",
        "Gelişmiş ince PLC; daha büyük program ve hızlı sayıcılar.",
        15872,
        false,
        "8 dijital giriş / 4 dijital çıkış",
        "RS-232 + 2 × RS-485",
        "SA2",
        None,
    ),
    model(
        "SX2",
        "DVP-SX2",
        "İnce (slim) tip",
        "Dahili analog G/Ç'li ince PLC.",
        15872,
        false,
        "8 DI / 6 DO + 4 analog giriş / 2 analog çıkış",
        "RS-232 + RS-485 + USB",
        "SX2",
        None,
    ),
    model(
        "SE",
        "DVP-SE",
        "İnce (slim) tip",
        "Dahili Ethernet portlu ince PLC.",
        15872,
        true,
        "8 dijital giriş / 4 dijital çıkış",
        "Ethernet (Modbus TCP) + 2 × RS-485",
        "SE",
        Some("DVPSimulator_SE.exe"),
    ),
    model(
        "SE2",
        "DVP-SE2",
        "İnce (slim) tip",
        "SE'nin yeni nesli; kullanıcı soketleri (ETHCONF/ETHMSG).",
        15872,
        false,
        "8 dijital giriş / 4 dijital çıkış",
        "Ethernet (Modbus TCP, soket) + 2 × RS-485",
        "SE2",
        None,
    ),
    model(
        "EH3",
        "DVP-EH3",
        "Yüksek performans",
        "Yüksek hızlı, büyük programlı standart tip PLC.",
        30000,
        true,
        "Dahili 16–80 G/Ç, yüksek hızlı darbe/sayıcı",
        "RS-232 + RS-485",
        "EH3",
        Some("DVPSimulator_EH3.exe"),
    ),
    model(
        "SV2",
        "DVP-SV2",
        "Yüksek performans",
        "Yüksek performanslı ince PLC; hareket kontrolü.",
        30000,
        false,
        "16 dijital giriş / 12 dijital çıkış",
        "RS-232 + RS-485",
        "SV2",
        None,
    ),
];

/// Simulator file the verified comparison bridge launches for `id`. The bridge has
/// checked profiles only for the ES2 and SS2 simulators; other models run on ES2.
pub fn bridge_simulator(id: &str) -> &'static str {
    if id.eq_ignore_ascii_case("SS2") {
        "DVPSimulator_SS2.exe"
    } else {
        "DVPSimulator_ES2.exe"
    }
}

/// Models whose instruction set the ES2 simulator stands for exactly.
pub fn runs_natively_on_bridge(id: &str) -> bool {
    matches!(
        id.to_ascii_uppercase().as_str(),
        "ES2" | "SS2" | "EX2" | "ES2-E"
    )
}

pub fn find(id: &str) -> Option<&'static PlcModel> {
    MODELS.iter().find(|m| m.id.eq_ignore_ascii_case(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_runs_ss2_projects_on_ss2_and_everything_else_on_es2() {
        assert_eq!(bridge_simulator("SS2"), "DVPSimulator_SS2.exe");
        for id in ["ES2", "EX2", "SE", "EH3"] {
            assert_eq!(bridge_simulator(id), "DVPSimulator_ES2.exe");
        }
        assert!(runs_natively_on_bridge("es2-e"));
        assert!(!runs_natively_on_bridge("EH3"));
    }
    #[test]
    fn catalog_is_consistent_with_the_special_register_tables() {
        assert!(find(DEFAULT).is_some());
        for m in MODELS {
            assert!(FAMILIES.contains(&m.family), "{}", m.id);
            assert!(
                crate::special_registers::model_index(m.special_column).is_some(),
                "{} → {}",
                m.id,
                m.special_column
            );
        }
        let mut ids = std::collections::BTreeSet::new();
        assert!(MODELS.iter().all(|m| ids.insert(m.id)));
    }
}
