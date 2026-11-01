//! Delta API88 PID/DPID documented-equation candidate.
//!
//! This is a named deterministic local profile, not installed ES2/SS2 parity:
//! those original handlers are skip-only stubs.
//! and pid-design.txt for equations, firmware exclusions and unresolved ABI.
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet};

fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PidProfile {
    #[default]
    DocumentedEquations,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FirstSample {
    #[default]
    Immediate,
    AfterPeriod,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SampleDt {
    #[default]
    ConfiguredPeriod,
    ElapsedSincePreviousSample,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisabledTiming {
    #[default]
    RestartOnEnable,
    KeepDeadline,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MissedSamples {
    #[default]
    OneFromVisit,
    OnePreservingPhase,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IntegralTimeUnit {
    #[default]
    Seconds,
    Milliseconds,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputRounding {
    #[default]
    TowardZero,
    NearestTiesAway,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DerivativeRange {
    #[default]
    SignedParameterTable,
    NonnegativeNote,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ZeroTi {
    #[default]
    DisableIntegralTerm,
    HoldWithLimitation,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThermalPolicy {
    /// Default: selected PDF alone does not resolve the conflicting formula.
    #[default]
    UnverifiedHold,
    /// Word-mode4 math/state candidate recovered from installed EH2, not
    /// ES2/SS2 compatibility or the complete EH2 scheduler/x87 environment.
    RecoveredEh2Formula,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PidPolicies {
    pub first_sample: FirstSample,
    pub sample_dt: SampleDt,
    pub disabled_timing: DisabledTiming,
    pub missed_samples: MissedSamples,
    pub integral_time_unit: IntegralTimeUnit,
    pub output_rounding: OutputRounding,
    pub derivative_range: DerivativeRange,
    pub zero_ti: ZeroTi,
    pub thermal: ThermalPolicy,
    /// Selected local firmware contract for 16-bit manual modes7/8.
    pub manual_modes_firmware_2_plus: bool,
}
impl Default for PidPolicies {
    fn default() -> Self {
        Self {
            first_sample: FirstSample::Immediate,
            sample_dt: SampleDt::ConfiguredPeriod,
            disabled_timing: DisabledTiming::RestartOnEnable,
            missed_samples: MissedSamples::OneFromVisit,
            integral_time_unit: IntegralTimeUnit::Seconds,
            output_rounding: OutputRounding::TowardZero,
            derivative_range: DerivativeRange::SignedParameterTable,
            zero_ti: ZeroTi::DisableIntegralTerm,
            thermal: ThermalPolicy::UnverifiedHold,
            manual_modes_firmware_2_plus: true,
        }
    }
}
#[derive(Debug, Clone, Default)]
struct LoopState {
    next_due_us: Option<u64>,
    last_sample_us: Option<u64>,
    period_us: u64,
    previous_mode: Option<i16>,
    previous_sv: Option<i32>,
}
#[derive(Debug, Clone)]
pub struct PidState {
    pub profile: PidProfile,
    pub policies: PidPolicies,
    pub notices: BTreeSet<String>,
    loops: BTreeMap<u16, LoopState>,
    owners: BTreeMap<u16, (u16, u16)>,
    scan_period_us: u64,
    previous_scan_start_us: Option<u64>,
}
impl Default for PidState {
    fn default() -> Self {
        Self {
            profile: PidProfile::DocumentedEquations,
            policies: PidPolicies::default(),
            notices: BTreeSet::new(),
            loops: BTreeMap::new(),
            owners: BTreeMap::new(),
            scan_period_us: 0,
            previous_scan_start_us: None,
        }
    }
}
impl PidState {
    /// Call once immediately after timing_io::begin_scan. The first cycle only
    /// knows its nominal base; subsequent cycles include the observed previous
    /// actual start interval (including DELAY). This is a local sampling policy.
    pub fn begin_scan(m: &mut Memory, nominal_scan_us: u64) {
        let now = m.timing_io.now_us;
        let state = &mut m.pid;
        state.scan_period_us = state.previous_scan_start_us.map_or(nominal_scan_us, |old| {
            now.saturating_sub(old).max(nominal_scan_us)
        });
        state.previous_scan_start_us = Some(now);
    }
    /// New source/execution epoch, not false power or a skipped CJ visit.
    pub fn clear_execution_epoch(&mut self) {
        self.loops.clear();
        self.owners.clear();
        self.scan_period_us = 0;
        self.previous_scan_start_us = None;
    }
    pub fn owner_count(&self) -> usize {
        self.owners.len()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PidAction {
    pub setpoint: Device,
    pub process: Device,
    pub parameters: Device,
    pub output: Device,
    pub wide: bool,
}
impl PidAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let wide = match name.to_ascii_uppercase().as_str() {
            "PID" => false,
            "DPID" => true,
            "PIDP" | "DPIDP" => {
                return Err("PIDP/DPIDP seçilen PDF'de yok; PID/DPID kullanın".into());
            }
            _ => return Err("PID veya DPID bekleniyor".into()),
        };
        if args.len() != 4 {
            return Err(
                "PID/DPID: SV, PV, parametre bloğu ve MV için dört D adresi gerekli".into(),
            );
        }
        let mut a = [d(0); 4];
        for (i, raw) in args.iter().enumerate() {
            a[i] = raw.parse()?;
            if a[i].area != Area::D {
                return Err("PID/DPID operand matrisi yalnız D adreslerine izin verir".into());
            }
        }
        let out = Self {
            setpoint: a[0],
            process: a[1],
            parameters: a[2],
            output: a[3],
            wide,
        };
        out.validate_span(9999)?;
        Ok(out)
    }
    pub fn name(&self) -> &'static str {
        if self.wide { "DPID" } else { "PID" }
    }
    pub fn header(&self) -> u16 {
        if self.wide { 0xc100 } else { 0xc000 }
    }
    pub fn parameter_words(&self) -> u16 {
        if self.wide { 21 } else { 20 }
    }
    pub fn dest(&self) -> Device {
        self.output
    }
    pub fn label(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.name(),
            self.setpoint,
            self.process,
            self.parameters,
            self.output
        )
    }
    pub fn display_value(&self, m: &Memory) -> String {
        if self.wide {
            m.double_word(self.output).to_string()
        } else {
            m.word(self.output).to_string()
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for base in [self.setpoint, self.process, self.output] {
            for n in 0..if self.wide { 2 } else { 1 } {
                out.insert(d(base.index + n));
            }
        }
        for n in 0..self.parameter_words() {
            out.insert(d(self.parameters.index + n));
        }
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    pub fn validate_blocks(actions: &[Self]) -> Result<(), String> {
        for (i, a) in actions.iter().enumerate() {
            a.validate_span(9999)?;
            for b in &actions[..i] {
                if overlap(a.span(), b.span()) {
                    return Err("PID/DPID parametre blokları başka PID örneğiyle çakışamaz".into());
                }
            }
        }
        Ok(())
    }
    fn span(&self) -> (u16, u16) {
        (
            self.parameters.index,
            self.parameters.index + self.parameter_words() - 1,
        )
    }
    fn validate_span(&self, last: u16) -> Result<(), String> {
        for base in [self.setpoint, self.process, self.output] {
            if base.area != Area::D
                || u32::from(base.index) + u32::from(self.wide) > u32::from(last)
            {
                return Err("PID/DPID sayısal operandı modelin D alanını aşıyor".into());
            }
        }
        if self.parameters.area != Area::D
            || u32::from(self.parameters.index) + u32::from(self.parameter_words())
                > u32::from(last) + 1
        {
            return Err("PID/DPID 20/21 sözcüklü parametre bloğu modelin D alanını aşıyor".into());
        }
        Ok(())
    }
    fn numeric(&self, m: &Memory, base: Device) -> i32 {
        if self.wide {
            m.double_word(base)
        } else {
            i32::from(m.word(base))
        }
    }
    fn numeric_field(&self, m: &Memory, n: u16) -> i32 {
        self.numeric(m, d(self.parameters.index + n))
    }
    fn write_numeric_field(&self, m: &mut Memory, n: u16, value: i32) {
        let dst = d(self.parameters.index + n);
        if self.wide {
            m.set_double_word(dst, value);
        } else {
            m.set_word(dst, value as i16);
        }
    }
    fn integral_offset(&self) -> u16 {
        if self.wide { 15 } else { 10 }
    }
    fn previous_pv_offset(&self) -> u16 {
        if self.wide { 17 } else { 12 }
    }
    fn read_integral(&self, m: &Memory) -> f64 {
        f64::from(f32::from_bits(
            m.double_word(d(self.parameters.index + self.integral_offset())) as u32,
        ))
    }
    fn write_integral(&self, m: &mut Memory, value: f64) {
        m.set_double_word(
            d(self.parameters.index + self.integral_offset()),
            (value as f32).to_bits() as i32,
        );
    }
    /// Must receive false visits, including repeated loop visits at the actual
    /// header PC. A skipped CJ node has no visit and therefore keeps its deadline.
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let mut state = std::mem::take(&mut m.pid);
        self.execute_state(m, &mut state, power, pc);
        m.pid = state;
    }
    fn execute_state(&self, m: &mut Memory, state: &mut PidState, power: bool, pc: u16) {
        if !power {
            if state.policies.disabled_timing == DisabledTiming::RestartOnEnable
                && let Some(s) = state.loops.get_mut(&pc)
            {
                s.next_due_us = None;
                s.last_sample_us = None;
            }
            return;
        }
        let fail = |m: &mut Memory, e: String| {
            m.execution_limitations
                .insert(format!("{}: {e}", self.name()));
        };
        if let Err(e) = self.validate_span(m.data_register_limit.unwrap_or(9999)) {
            fail(m, e);
            return;
        }
        if state.loops.len() >= 512 && !state.loops.contains_key(&pc) {
            fail(m, "512 yerel PID örneği sınırı aşıldı".into());
            return;
        }
        let span = self.span();
        if state
            .owners
            .iter()
            .any(|(owner, other)| *owner != pc && overlap(span, *other))
        {
            fail(
                m,
                "S3 parametre bloğu başka PID örneğiyle çakışıyor; kaynak topolojisini düzeltin"
                    .into(),
            );
            return;
        }
        if state.owners.get(&pc).is_some_and(|old| *old != span) {
            fail(
                m,
                "Aynı PC için PID yerleşimi değişti; yeni yürütme dönemi gerekli".into(),
            );
            return;
        }
        state.owners.insert(pc, span);
        if [self.setpoint, self.process, self.output]
            .into_iter()
            .any(|base| overlap(span, (base.index, base.index + u16::from(self.wide))))
        {
            state.notices.insert("PID operand/S3 aliası: bu yerel profil önce girdileri okur; normalizasyon, integral, eski PV ve MV sırasıyla yazar. Firmware alias sırası doğrulanmadı".into());
        }
        state.notices.insert(format!("PID yerel DocumentedEquations profili: {:?}; binary32 integral, f64 ara hesap; ES2/SS2 özgün gövdeleri hesaplama doğrulamaz",state.policies));
        let mut cfg = Config::read(self, m);
        if cfg.ts == 0 {
            let s = state.loops.entry(pc).or_default();
            s.next_due_us = None;
            s.last_sample_us = None;
            return;
        }
        if !(1..=2000).contains(&cfg.ts) {
            fail(
                m,
                "Ts 0 veya 1..2000 (10 ms) olmalı; diğer değerlerin Delta davranışı bilinmiyor"
                    .into(),
            );
            return;
        }
        if cfg.mode < 0 || cfg.mode > 10 {
            cfg.mode = 0;
        }
        if cfg.mode == 6 {
            fail(
                m,
                "Mod 6 PDF'de tanımlı değil; MV ve parametreler korundu".into(),
            );
            return;
        }
        if cfg.mode == 3 {
            fail(
                m,
                "Sıcaklık modu 3 auto-tuning algoritması henüz doğrulanmadı; MV/durum korundu"
                    .into(),
            );
            return;
        }
        if cfg.mode == 4 && (self.wide || state.policies.thermal == ThermalPolicy::UnverifiedHold) {
            fail(m,"Mod 4 varsayılan profilde/DPID için doğrulanmadı; ayrı RecoveredEh2Formula yalnız 16-bit yerel adaydır. MV/durum korundu".into());
            return;
        }
        if cfg.mode == 4 {
            state.notices.insert("Mod 4 RecoveredEh2Formula: EH2 word termal matematik/durum adayı; ES2 eşdeğerliği veya tam EH2 x87/scheduler değildir. Ts tabanlı formül, signed Kd ve kesme kullanılır; ilk örnek/deadline ayrı yerel politikadır".into());
        }
        if self.wide && matches!(cfg.mode, 7..=9) {
            fail(
                m,
                "DPID mod 7/8/9 geniş parametre tablosunda tanımlı değil; destek kanıtı gerekli"
                    .into(),
            );
            return;
        }
        if matches!(cfg.mode, 7 | 8) && !state.policies.manual_modes_firmware_2_plus {
            fail(
                m,
                "PID manuel mod 7/8 ES2/SS2 firmware 2.00+ profili gerektirir".into(),
            );
            return;
        }
        if cfg.tolerance < 0 {
            fail(
                m,
                "Negatif tolerans PDF aralığı dışında; işlem yapılmadı".into(),
            );
            return;
        }
        if !cfg.integral.is_finite() {
            fail(
                m,
                "S3 integral binary32 değeri NaN/sonsuz; sayısal PID durumu belirlenemiyor".into(),
            );
            return;
        }
        let minimum_units = state.scan_period_us.div_ceil(10_000).max(1);
        if minimum_units > 2000 {
            fail(
                m,
                "Gözlenen tarama süresi PID Ts üst sınırını aşıyor".into(),
            );
            return;
        }
        cfg.ts = cfg.ts.max(minimum_units as i16);
        cfg.kp = cfg.kp.clamp(0, 30000);
        cfg.ki = cfg.ki.clamp(0, 30000);
        if cfg.mode == 10 && cfg.ki == 0 && state.policies.zero_ti == ZeroTi::HoldWithLimitation {
            fail(
                m,
                "Mod 10 TI=0 davranışı belirtilmemiş; seçili politika işlem yapmıyor".into(),
            );
            return;
        }
        cfg.kd = cfg.kd.clamp(
            if cfg.mode == 4
                || state.policies.derivative_range == DerivativeRange::SignedParameterTable
            {
                -30000
            } else {
                0
            },
            30000,
        );
        if cfg.mv_low > cfg.mv_high {
            std::mem::swap(&mut cfg.mv_low, &mut cfg.mv_high);
        }
        if cfg.i_low > cfg.i_high {
            std::mem::swap(&mut cfg.i_low, &mut cfg.i_high);
        }
        let period = cfg.ts as u64 * 10_000;
        let now = m.timing_io.now_us;
        let s = state.loops.entry(pc).or_default();
        if s.period_us != period {
            s.period_us = period;
            s.next_due_us = s.last_sample_us.map(|last| last.saturating_add(period));
        }
        if s.next_due_us.is_none() {
            s.next_due_us = Some(match state.policies.first_sample {
                FirstSample::Immediate => now,
                FirstSample::AfterPeriod => now.saturating_add(period),
            });
        }
        // Parameter normalization at a powered visit is part of this named
        // local profile; no claim about an undocumented firmware write phase.
        cfg.write_normalized(self, m);
        let due = s.next_due_us.expect("initialized deadline");
        if now < due {
            return;
        }
        if cfg.mode == 4 {
            match recovered_eh2_thermal(&cfg) {
                Ok(value) => {
                    if value.commit_integral {
                        self.write_integral(m, f64::from(value.integral));
                    }
                    self.write_numeric_field(
                        m,
                        self.previous_pv_offset(),
                        i32::from(value.previous_pv),
                    );
                    m.set_word(self.output, value.output);
                    finish_sample(s, now, period, due, &cfg, state.policies.missed_samples);
                }
                Err(error) => fail(m, error),
            }
            return;
        }
        let dt_us = if state.policies.sample_dt == SampleDt::ElapsedSincePreviousSample {
            s.last_sample_us
                .map_or(period, |last| now.saturating_sub(last).max(1))
        } else {
            period
        };
        let unit = match state.policies.integral_time_unit {
            IntegralTimeUnit::Seconds => 1_000_000.0,
            IntegralTimeUnit::Milliseconds => 1_000.0,
        };
        let dt = dt_us as f64 / unit;
        let error = cfg.error();
        let p = f64::from(cfg.kp) / 100.0 * error;
        let derivative = if cfg.mode == 10 {
            let prior_sv = s.previous_sv.unwrap_or(cfg.sv);
            let previous_error = f64::from(prior_sv) - f64::from(cfg.previous_pv);
            (error - previous_error) / dt
        } else {
            (f64::from(cfg.pv) - f64::from(cfg.previous_pv)) / dt
        };
        let (ki, derivative_term) = if cfg.mode == 10 {
            let units_per_ms = 1000.0 / unit;
            let ti = f64::from(cfg.ki) * units_per_ms;
            let td = f64::from(cfg.kd) * units_per_ms;
            (
                if ti == 0.0 {
                    0.0
                } else {
                    f64::from(cfg.kp) / 100.0 / ti
                },
                f64::from(cfg.kp) / 100.0 * td * derivative,
            )
        } else {
            (
                f64::from(cfg.ki) / 100.0,
                f64::from(cfg.kd) / 100.0 * derivative,
            )
        };
        let prior_i = quantized_integral(cfg.integral, &cfg);
        let mut integral = if cfg.mode == 8 {
            cfg.integral
        } else {
            quantized_integral(prior_i + error * dt, &cfg)
        };
        let mut result = p + ki * integral + derivative_term;
        if cfg.mode == 5 && s.previous_mode == Some(8) {
            if ki == 0.0 && (f64::from(cfg.mv) - p - derivative_term).abs() > f64::EPSILON {
                fail(m,"Mod 8->5 Ki=0 ile son MV'ye integral dönüşümü belirlenemiyor; MV/durum korundu".into());
                return;
            }
            integral = if ki == 0.0 {
                prior_i
            } else {
                quantized_integral((f64::from(cfg.mv) - p - derivative_term) / ki, &cfg)
            };
            result = p + ki * integral + derivative_term;
        } else if matches!(cfg.mode, 5 | 10) {
            // Directional conditional integration: allow unwinding at a bound.
            if result >= cfg.mv_high as f64 && integral > prior_i
                || result <= cfg.mv_low as f64 && integral < prior_i
            {
                integral = prior_i;
                result = p + ki * integral + derivative_term;
            }
        }
        if cfg.mode == 9 && (result > cfg.mv_high as f64 || result < cfg.mv_low as f64) {
            let limited = result.clamp(cfg.mv_low as f64, cfg.mv_high as f64);
            if ki == 0.0 {
                fail(
                    m,
                    "Mod 9 sınırda Ki=0: integral geri hesabı için Delta kuralı bilinmiyor".into(),
                );
                return;
            }
            integral = quantized_integral((limited - p - derivative_term) / ki, &cfg);
            result = limited;
        }
        if matches!(cfg.mode, 5 | 9 | 10) {
            result = result.clamp(cfg.mv_low as f64, cfg.mv_high as f64);
        }
        if !result.is_finite() || !integral.is_finite() {
            fail(
                m,
                "PID hesap sonucu sonlu değil; sayısal durum/MV korundu".into(),
            );
            return;
        }
        if cfg.mode != 8 {
            self.write_integral(m, integral);
        }
        self.write_numeric_field(m, self.previous_pv_offset(), cfg.pv);
        if !matches!(cfg.mode, 7 | 8) {
            let rounded = match state.policies.output_rounding {
                OutputRounding::TowardZero => result.trunc(),
                OutputRounding::NearestTiesAway => result.round(),
            };
            let value = rounded.clamp(
                if self.wide {
                    i32::MIN as f64
                } else {
                    i16::MIN as f64
                },
                if self.wide {
                    i32::MAX as f64
                } else {
                    i16::MAX as f64
                },
            ) as i32;
            if self.wide {
                m.set_double_word(self.output, value);
            } else {
                m.set_word(self.output, value as i16);
            }
        }
        finish_sample(s, now, period, due, &cfg, state.policies.missed_samples);
    }
}
fn finish_sample(
    s: &mut LoopState,
    now: u64,
    period: u64,
    due: u64,
    cfg: &Config,
    missed: MissedSamples,
) {
    s.last_sample_us = Some(now);
    s.previous_mode = Some(cfg.mode);
    s.previous_sv = Some(cfg.sv);
    s.next_due_us = Some(match missed {
        MissedSamples::OneFromVisit => now.saturating_add(period),
        MissedSamples::OnePreservingPhase => {
            let intervals = now.saturating_sub(due) / period + 1;
            due.saturating_add(intervals.saturating_mul(period))
        }
    });
}
struct ThermalValue {
    output: i16,
    integral: f32,
    previous_pv: i16,
    commit_integral: bool,
}
/// Models the observed FSTP-f32 boundaries with f64 between stores. x87 uses
/// extended precision and a decimal0.01 extended constant, so boundary ULP
/// identity is deliberately not claimed by this finite arithmetic candidate.
fn recovered_eh2_thermal(cfg: &Config) -> Result<ThermalValue, String> {
    let q = |value: f64| value as f32;
    let sv = cfg.sv as i16;
    let pv = cfg.pv as i16;
    let old_pv = cfg.previous_pv as i16;
    let mut error = sv.wrapping_sub(pv);
    if cfg.tolerance > 0 && i64::from(error).abs() <= cfg.tolerance {
        error = 0;
    }
    let mut integral = cfg.integral as f32;
    let mut previous_pv = old_pv;
    let raw = if i32::from(pv) <= i32::from(sv) - i32::from(cfg.kp) {
        q(f64::from(cfg.ts) * 10.0)
    } else if i32::from(pv) >= i32::from(sv) + i32::from(cfg.kp) {
        0.0
    } else {
        // Kp==0 cannot enter this strict interior interval.
        previous_pv = pv;
        let delta = q(f64::from(i32::from(pv) - i32::from(old_pv)));
        let scaled = q(f64::from(delta) * 100.0);
        let rate = q(f64::from(scaled) / f64::from(cfg.ts));
        let derivative = q(f64::from(cfg.kd) * f64::from(rate));
        let error_f32 = f32::from(error);
        let cent_error = q(f64::from(error_f32) * 0.01);
        integral = q(f64::from(cfg.ts) * f64::from(cent_error) + cfg.integral);
        let i_term = if cfg.ki == 0 {
            0.0
        } else {
            q(f64::from(integral) / f64::from(cfg.ki))
        };
        let sum = q(f64::from(error_f32) + f64::from(i_term) + f64::from(derivative));
        let ratio = q(f64::from(sum) / f64::from(cfg.kp));
        q(f64::from(cfg.ts) * f64::from(ratio) * 10.0)
    };
    if !raw.is_finite() || !integral.is_finite() {
        return Err(
            "EH2 termal aday: sonlu binary32 aritmetik alanı aşıldı; MV/integral/PV korundu".into(),
        );
    }
    let truncated = f64::from(raw).trunc();
    if !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&truncated) {
        return Err("EH2 termal aday: FISTP signed64 taşma/indefinite dönüşümü doğrulanmadı; MV/durum korundu".into());
    }
    // Original conversion returns signed64 inEDX:EAX; caller clamps signedEAX.
    let eax = (truncated as i64) as i32;
    let output = eax.clamp(cfg.mv_low as i32, cfg.mv_high as i32) as i16;
    // Mode4 bypasses ordinary I-limit clamping and suppresses public I write
    // whenever FINAL integer MV equals either bound, including exact equality.
    let commit_integral = i64::from(output) != cfg.mv_low && i64::from(output) != cfg.mv_high;
    Ok(ThermalValue {
        output,
        integral,
        previous_pv,
        commit_integral,
    })
}
fn overlap(a: (u16, u16), b: (u16, u16)) -> bool {
    a.0 <= b.1 && b.0 <= a.1
}
#[derive(Debug, Clone)]
struct Config {
    ts: i16,
    kp: i16,
    ki: i16,
    kd: i16,
    mode: i16,
    tolerance: i64,
    mv_low: i64,
    mv_high: i64,
    i_low: i64,
    i_high: i64,
    integral: f64,
    previous_pv: i32,
    sv: i32,
    pv: i32,
    mv: i32,
}
impl Config {
    fn read(a: &PidAction, m: &Memory) -> Self {
        let word = |n| m.word(d(a.parameters.index + n));
        let field = |n| i64::from(a.numeric_field(m, n));
        let (tol, mh, ml, ih, il) = if a.wide {
            (5, 7, 9, 11, 13)
        } else {
            (5, 6, 7, 8, 9)
        };
        Self {
            ts: word(0),
            kp: word(1),
            ki: word(2),
            kd: word(3),
            mode: word(4),
            tolerance: field(tol),
            mv_high: field(mh),
            mv_low: field(ml),
            i_high: field(ih),
            i_low: field(il),
            integral: a.read_integral(m),
            previous_pv: a.numeric_field(m, a.previous_pv_offset()),
            sv: a.numeric(m, a.setpoint),
            pv: a.numeric(m, a.process),
            mv: a.numeric(m, a.output),
        }
    }
    fn error(&self) -> f64 {
        let mut e = if self.mode == 2 {
            f64::from(self.pv) - f64::from(self.sv)
        } else {
            f64::from(self.sv) - f64::from(self.pv)
        };
        if matches!(self.mode, 1 | 2) {
            e = e.max(0.0);
        }
        if self.tolerance > 0 && e.abs() <= self.tolerance as f64 {
            0.0
        } else {
            e
        }
    }
    fn write_normalized(&self, a: &PidAction, m: &mut Memory) {
        for (n, v) in [self.ts, self.kp, self.ki, self.kd, self.mode]
            .into_iter()
            .enumerate()
        {
            m.set_word(d(a.parameters.index + n as u16), v);
        }
        let (mh, ml, ih, il) = if a.wide { (7, 9, 11, 13) } else { (6, 7, 8, 9) };
        for (n, v) in [
            (mh, self.mv_high),
            (ml, self.mv_low),
            (ih, self.i_high),
            (il, self.i_low),
        ] {
            a.write_numeric_field(m, n, v as i32);
        }
    }
}
fn quantized_integral(value: f64, cfg: &Config) -> f64 {
    f64::from(value.clamp(cfg.i_low as f64, cfg.i_high as f64) as f32)
}

#[cfg(test)]
#[path = "pid_tests.rs"]
mod tests;
