//! API145 interface and explicit external calculation boundary.
//!
//! The supplied manual does not specify Delta's fuzzy rules or five state words.
//! No numerical FTC implementation is bundled. An external engine receives raw
//! state/time and owns sampling, filtering, rounding and ordered state commits.
use crate::file_memory::FirmwareVersion;
use crate::plc::{Area, Device, Memory, Operand};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const MISSING: &str = "FTC: Delta bulanık sıcaklık algoritması henüz çözülemedi; MV ve durum korunuyor. FTC sayısal desteği eksik.";
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FtcModel {
    #[default]
    Disabled,
    Es2,
    Ex2,
    Sa2,
    Sx2,
    Se,
    Se2,
    Ss2,
}
#[derive(Debug, Clone, Copy)]
pub struct FtcProfile {
    pub model: FtcModel,
    pub firmware: FirmwareVersion,
}
impl Default for FtcProfile {
    fn default() -> Self {
        Self {
            model: FtcModel::Disabled,
            firmware: FirmwareVersion::new(1, 0),
        }
    }
}
impl FtcProfile {
    fn validate(self) -> Result<(), String> {
        let minimum = match self.model {
            FtcModel::Es2 | FtcModel::Ex2 => FirmwareVersion::new(3, 22),
            FtcModel::Sa2 | FtcModel::Sx2 => FirmwareVersion::new(2, 66),
            FtcModel::Se => FirmwareVersion::new(1, 60),
            FtcModel::Se2 => FirmwareVersion::new(1, 0),
            FtcModel::Ss2 => return Err(
                "FTC: SS2 özet tablosu destek göstermiyor; sayfa başlığı destek kanıtı değildir"
                    .into(),
            ),
            FtcModel::Disabled => return Err("FTC: açık CPU ve firmware profili gerekli".into()),
        };
        if self.firmware < minimum || self.firmware.minor > 99 {
            Err("FTC: seçilen CPU firmware sürümü belgelenmiş minimumun altında".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeatingEnvironment {
    Slow,
    General,
    Fast,
    HighSpeed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FtcConfig {
    pub fahrenheit: bool,
    pub filter: bool,
    pub environment: HeatingEnvironment,
    pub reserved: u16,
}
impl FtcConfig {
    /// PDF table interpretation; does not guess the undocumented numeric gains.
    pub fn decode(word: u16) -> Self {
        let environment = if word & 4 != 0 {
            HeatingEnvironment::Slow
        } else if word & 8 != 0 {
            HeatingEnvironment::General
        } else if word & 16 != 0 {
            HeatingEnvironment::Fast
        } else if word & 32 != 0 {
            HeatingEnvironment::HighSpeed
        } else {
            HeatingEnvironment::General
        };
        Self {
            fahrenheit: word & 1 != 0,
            filter: word & 2 != 0,
            environment,
            reserved: word & 0xffc0,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct FtcAction {
    pub setpoint: Operand,
    pub process: Operand,
    pub parameters: Device,
    pub output: Device,
}
impl FtcAction {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        if args.len() != 4 {
            return Err("FTC dört operand gerektirir: SV PV parametreD çıktıD".into());
        }
        let scalar = |raw: &str| -> Result<Operand, String> {
            let value = Operand::parse(raw)?;
            if matches!(value, Operand::Register(v) if v.area != Area::D)
                || !raw.starts_with(['K', 'H', 'D'])
            {
                return Err("FTC SV/PV matrisi yalnız K/H/D içerir".into());
            }
            Ok(value)
        };
        let result = Self {
            setpoint: scalar(&args[0])?,
            process: scalar(&args[1])?,
            parameters: args[2].parse()?,
            output: args[3].parse()?,
        };
        if result.parameters.area != Area::D || result.output.area != Area::D {
            return Err("FTC parametre ve MV hedefi D olmalı".into());
        }
        result.validate_span(9999)?;
        Ok(result)
    }
    fn validate_span(&self, limit: u16) -> Result<(), String> {
        if self
            .parameters
            .index
            .checked_add(6)
            .is_none_or(|end| end > limit)
            || self.output.index > limit
            || [&self.setpoint, &self.process]
                .into_iter()
                .any(|v| matches!(v, Operand::Register(r) if r.index > limit))
        {
            Err("FTC: yedi kelimelik parametre bloğu veya operand model sınırını aşıyor".into())
        } else {
            Ok(())
        }
    }
    pub fn dest(&self) -> Device {
        self.output
    }
    pub fn label(&self) -> String {
        format!(
            "FTC {} {} {} {}",
            self.setpoint, self.process, self.parameters, self.output
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for v in [&self.setpoint, &self.process] {
            if let Operand::Register(v) = v {
                out.insert(*v);
            }
        }
        out.extend((0..7).map(|i| d(self.parameters.index + i)));
        out.insert(self.output);
    }
    pub fn display_value(&self, m: &Memory) -> String {
        format!(
            "{}={} · FTC hesabı harici",
            self.output,
            m.word(self.output)
        )
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let mut state = std::mem::take(&mut m.ftc);
        state.visit(m, self, power, pc);
        m.ftc = state;
    }
}
#[derive(Debug, Clone)]
pub struct FtcInput {
    pub profile: FtcProfile,
    pub pc: u16,
    pub now_us: u64,
    pub previous_visit_us: Option<u64>,
    pub was_enabled: bool,
    pub sv: i16,
    pub pv: i16,
    pub parameters: [i16; 7],
    pub mv: i16,
    pub context: Vec<u8>,
}
#[derive(Debug, Clone, Copy)]
pub enum FtcTarget {
    Parameter(u8),
    Output,
}
#[derive(Debug, Clone, Default)]
pub struct FtcStep {
    pub writes: Vec<(FtcTarget, i16)>,
    pub context: Vec<u8>,
}
/// Identity is supplied provenance, not proof of Delta compatibility. Test
/// fixtures implementing this trait verify the adapter only, never fuzzy math.
pub trait FtcEngine: std::fmt::Debug + Send + Sync {
    fn identity(&self) -> &str;
    fn evaluate(&self, input: &FtcInput) -> Result<FtcStep, String>;
}
#[derive(Debug, Clone, Default)]
struct LoopContext {
    context: Vec<u8>,
    previous_visit_us: Option<u64>,
    enabled: bool,
}
#[derive(Debug, Clone, Default)]
pub struct FtcState {
    pub profile: FtcProfile,
    pub notices: BTreeSet<String>,
    engine: Option<Arc<dyn FtcEngine>>,
    loops: BTreeMap<u16, LoopContext>,
    owners: BTreeMap<u16, u16>,
}
impl FtcState {
    pub fn set_engine(&mut self, engine: Arc<dyn FtcEngine>) -> Result<(), String> {
        if engine.identity().trim().is_empty() {
            return Err("FTC harici motor kimliği boş olamaz".into());
        }
        self.reset_instruction_state();
        self.engine = Some(engine);
        Ok(())
    }
    pub fn reset_instruction_state(&mut self) {
        self.loops.clear();
        self.owners.clear();
    }
    pub fn abort(m: &mut Memory) {
        m.ftc.reset_instruction_state();
    }
    fn visit(&mut self, m: &mut Memory, a: &FtcAction, power: bool, pc: u16) {
        let now = m.timing_io.now_us;
        if !power {
            let c = self.loops.entry(pc).or_default();
            c.enabled = false;
            c.previous_visit_us = Some(now);
            return;
        }
        if let Err(e) = a.validate_span(m.data_register_limit.unwrap_or(9999)) {
            m.execution_limitations.insert(e);
            return;
        }
        let parameters = std::array::from_fn(|i| m.word(d(a.parameters.index + i as u16)));
        if parameters[0] == 0 {
            let c = self.loops.entry(pc).or_default();
            c.enabled = false;
            c.previous_visit_us = Some(now);
            return;
        }
        if let Err(e) = self.profile.validate() {
            m.execution_limitations.insert(e);
            return;
        }
        if !(1..=200).contains(&parameters[0]) {
            m.execution_limitations.insert(
                "FTC: TS 1..200 dışında; belgelenmemiş hata/kelepçeleme davranışı uygulanmadı"
                    .into(),
            );
            return;
        }
        let sv = a.setpoint.read(m);
        let config = FtcConfig::decode(parameters[1] as u16);
        if !config.fahrenheit && !(1..=5000).contains(&sv) {
            m.execution_limitations
                .insert("FTC: Celsius SV 1..5000 dışında".into());
            return;
        }
        let Some(engine) = self.engine.as_ref() else {
            m.execution_limitations.insert(MISSING.into());
            self.notices.insert(MISSING.into());
            return;
        };
        if (a.parameters.index..=a.parameters.index + 6)
            .any(|i| self.owners.get(&i).is_some_and(|owner| *owner != pc))
        {
            m.execution_limitations
                .insert("FTC: iki komut aynı parametre/durum kelimelerini sahiplenemez".into());
            return;
        }
        let old = self.loops.get(&pc).cloned().unwrap_or_default();
        let input = FtcInput {
            profile: self.profile,
            pc,
            now_us: now,
            previous_visit_us: old.previous_visit_us,
            was_enabled: old.enabled,
            sv,
            pv: a.process.read(m),
            parameters,
            mv: m.word(a.output),
            context: old.context,
        };
        let result = match engine.evaluate(&input) {
            Ok(result) => result,
            Err(e) => {
                m.execution_limitations
                    .insert(format!("FTC harici motor: {e}"));
                return;
            }
        };
        // Validate all proposed writes before changing either RAM or opaque state.
        if result.writes.iter().any(|(target, value)| match target {
            FtcTarget::Parameter(i) => *i >= 7,
            FtcTarget::Output => *value < 0 || i32::from(*value) > i32::from(parameters[0]) * 100,
        }) {
            m.execution_limitations.insert(
                "FTC harici motoru geçersiz yazma üretti; işlem atomik olarak reddedildi".into(),
            );
            return;
        }
        for (target, value) in result.writes {
            m.set_word(
                match target {
                    FtcTarget::Parameter(i) => d(a.parameters.index + u16::from(i)),
                    FtcTarget::Output => a.output,
                },
                value,
            );
        }
        self.owners
            .extend((a.parameters.index..=a.parameters.index + 6).map(|i| (i, pc)));
        self.loops.insert(
            pc,
            LoopContext {
                context: result.context,
                previous_visit_us: Some(now),
                enabled: true,
            },
        );
        self.notices.insert(format!(
            "FTC harici motor: {}; algoritma kimliği sağlayıcı beyanıdır",
            engine.identity()
        ));
    }
}

#[cfg(test)]
#[path = "ftc_tests.rs"]
mod tests;
