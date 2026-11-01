//! Explicit RTC seeds and progression policies; never samples the host clock.
use crate::plc::{Area, Device, Memory, word_range};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcProfile {
    /// TWR itself supplies a seed, without choosing a passage-of-time backend.
    FrozenSeed,
    /// Explicit monotonic elapsed time with Gregorian month/day progression.
    VirtualGregorian,
    /// Old installed simulator: sampled second modulo60, no date carry.
    InstalledShadow,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RtcEvents {
    /// Forward only to the installed HOUR pending bitmap, never scan counters.
    pub native_second_changed: bool,
    pub virtual_elapsed_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcAdvance {
    /// Deliberately frozen instruction/replay boundary.
    Hold,
    ElapsedMicros(u64),
    /// An explicitly observed/injected host-second sample, NOT obtained here.
    InstalledSampledSecond(u8),
}

/// Scheduling is separate from the numerical clock backend. Explicit service
/// callers supply every boundary input; the source VM never samples wall time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RtcServiceSchedule {
    #[default]
    EndOfScan,
    ExplicitBoundaries,
}

pub(crate) fn is_instruction(action: &crate::plc::Action) -> bool {
    match action {
        crate::plc::Action::Pulse(inner) => is_instruction(inner),
        crate::plc::Action::Rtc(_) | crate::plc::Action::IndexedRtc(_) => true,
        _ => false,
    }
}

#[derive(Debug, Clone, Default)]
pub struct RtcClockState {
    profile: Option<RtcProfile>,
    // Native wire order: year0..99,weekday1..7,month,day,hour,minute,second.
    // Replay seeds may intentionally contain other raw bytes.
    shadow: Option<[u8; 7]>,
    fraction_us: u64,
    previous_native_second: Option<u8>,
    correction_was_on: bool,
}

fn data(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn offset(base: Device, amount: u16) -> Device {
    Device {
        area: base.area,
        index: base.index + amount,
    }
}
pub(crate) fn word_limit(base: Device, m: &Memory) -> u16 {
    match base.area {
        Area::T => 255,
        Area::C => 199,
        Area::D => m.data_register_limit.unwrap_or(9999),
        _ => 0,
    }
}
fn leap(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}
fn month_days(year: u16, month: u8) -> u8 {
    match month {
        2 => {
            if leap(year) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
fn valid_calendar(fields: [u8; 7]) -> Result<(), String> {
    let [year, weekday, month, day, hour, minute, second] = fields;
    if year > 99
        || !(1..=7).contains(&weekday)
        || !(1..=12).contains(&month)
        || day == 0
        || day > month_days(2000 + u16::from(year), month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err("RTC sanal Gregoryen takvimi için geçersiz tarih/saat".into());
    }
    Ok(())
}
fn ordinal(fields: [u8; 7]) -> u64 {
    let mut result = 0;
    for y in 2000..2000 + u16::from(fields[0]) {
        result += if leap(y) { 366 } else { 365 };
    }
    for month in 1..fields[2] {
        result += u64::from(month_days(2000 + u16::from(fields[0]), month));
    }
    result + u64::from(fields[3] - 1)
}
fn add_gregorian(fields: [u8; 7], seconds: u64) -> Result<[u8; 7], String> {
    valid_calendar(fields)?;
    let old_day = ordinal(fields);
    let time = u64::from(fields[4]) * 3600 + u64::from(fields[5]) * 60 + u64::from(fields[6]);
    let total = (old_day * 86400 + time)
        .checked_add(seconds)
        .ok_or("RTC sanal zaman ilerlemesi taştı")?;
    // Century rollover is not defined by the selected manual. Hold explicitly.
    if total >= 36525 * 86400 {
        return Err("RTC 2099 sonrası yüzyıl geçişi henüz tanımlanmadı".into());
    }
    let new_day = total / 86400;
    let mut remaining = new_day;
    let mut year = 2000u16;
    loop {
        let length = if leap(year) { 366 } else { 365 };
        if remaining < length {
            break;
        }
        remaining -= length;
        year += 1;
    }
    let mut month = 1;
    while remaining >= u64::from(month_days(year, month)) {
        remaining -= u64::from(month_days(year, month));
        month += 1;
    }
    let time = total % 86400;
    Ok([
        (year - 2000) as u8,
        ((u64::from(fields[1] - 1) + new_day - old_day) % 7 + 1) as u8,
        month,
        remaining as u8 + 1,
        (time / 3600) as u8,
        (time / 60 % 60) as u8,
        (time % 60) as u8,
    ])
}

impl RtcClockState {
    pub fn profile(&self) -> Option<RtcProfile> {
        self.profile
    }
    pub fn shadow(&self) -> Option<[u8; 7]> {
        self.shadow
    }
    pub fn fraction_us(&self) -> u64 {
        self.fraction_us
    }
    pub fn previous_native_second(&self) -> Option<u8> {
        self.previous_native_second
    }

    /// Absolute year2000..2099. Weekday is an explicit independent1..7 counter;
    /// it is not silently recomputed from the date supplied by the user.
    pub fn configure_virtual(&mut self, fields: [u16; 7]) -> Result<(), String> {
        if !(2000..=2099).contains(&fields[0]) || fields[1..].iter().any(|v| *v > 255) {
            return Err("RTC başlangıç yılı 2000..2099 olmalı".into());
        }
        let mut raw = fields.map(|v| v as u8);
        raw[0] = (fields[0] - 2000) as u8;
        valid_calendar(raw)?;
        *self = Self {
            profile: Some(RtcProfile::VirtualGregorian),
            shadow: Some(raw),
            ..Self::default()
        };
        Ok(())
    }
    /// Captures raw PRIVATE native state, not a possibly stale D1313..D1319
    /// display. Invalid Gregorian dates such as February31 remain representable.
    pub fn configure_installed_shadow(
        &mut self,
        shadow: [u8; 7],
        previous_second: u8,
    ) -> Result<(), String> {
        if previous_second > 59 {
            return Err("Yerel RTC örnek saniyesi 0..59 olmalı".into());
        }
        *self = Self {
            profile: Some(RtcProfile::InstalledShadow),
            shadow: Some(shadow),
            previous_native_second: Some(previous_second),
            ..Self::default()
        };
        Ok(())
    }
    pub fn configure_frozen(&mut self, shadow: [u8; 7]) {
        *self = Self {
            profile: Some(RtcProfile::FrozenSeed),
            shadow: Some(shadow),
            ..Self::default()
        };
    }
    /// An explicit replay boundary can replace the private state without
    /// resetting elapsed/tick phase or guessing a preceding host second.
    pub fn replace_installed_shadow(&mut self, shadow: [u8; 7]) -> Result<(), String> {
        if self.profile != Some(RtcProfile::InstalledShadow) {
            return Err("Yerel RTC yeniden oynatma profili seçilmedi".into());
        }
        self.shadow = Some(shadow);
        Ok(())
    }
    fn commit_twr(&mut self, shadow: [u8; 7]) {
        self.shadow = Some(shadow);
        if self.profile.is_none() {
            self.profile = Some(RtcProfile::FrozenSeed);
        }
        // Explicit virtual policy: setting its calendar starts a whole second.
        // Old installed mode retains its independent sampled-second phase.
        if self.profile == Some(RtcProfile::VirtualGregorian) {
            self.fraction_us = 0;
        }
    }
    pub fn advance_monotonic_us(&mut self, elapsed_us: u64) -> Result<RtcEvents, String> {
        if self.profile != Some(RtcProfile::VirtualGregorian) {
            return Err(
                "RTC monoton ilerleme yalnız açık sanal Gregoryen profilde kullanılabilir".into(),
            );
        }
        let phase = self
            .fraction_us
            .checked_add(elapsed_us)
            .ok_or("RTC mikrosaniye sayacı taştı")?;
        let seconds = phase / 1_000_000;
        let before = self.shadow.ok_or("RTC başlangıç zamanı yapılandırılmadı")?;
        let after = add_gregorian(before, seconds)?;
        self.shadow = Some(after);
        self.fraction_us = phase % 1_000_000;
        Ok(RtcEvents {
            virtual_elapsed_seconds: seconds,
            ..RtcEvents::default()
        })
    }
    pub fn sample_installed_second(&mut self, sampled: u8) -> Result<RtcEvents, String> {
        if self.profile != Some(RtcProfile::InstalledShadow) || sampled > 59 {
            return Err("RTC yerel örnek için InstalledShadow ve 0..59 saniye gerekli".into());
        }
        let previous = self
            .previous_native_second
            .ok_or("RTC önceki yerel örnek saniyesi belirtilmedi")?;
        let mut shadow = self.shadow.ok_or("RTC başlangıç zamanı yapılandırılmadı")?;
        if sampled == previous {
            return Ok(RtcEvents::default());
        }
        let delta = if sampled > previous {
            sampled - previous
        } else {
            sampled + 60 - previous
        };
        shadow[6] = shadow[6].wrapping_add(delta);
        if shadow[6] > 59 {
            shadow[6] = shadow[6].wrapping_sub(60);
            shadow[5] = shadow[5].wrapping_add(1);
            if shadow[5] > 59 {
                shadow[5] = 0;
                shadow[4] = shadow[4].wrapping_add(1);
                if shadow[4] > 23 {
                    shadow[4] = 0;
                }
            }
        }
        self.shadow = Some(shadow);
        self.previous_native_second = Some(sampled);
        Ok(RtcEvents {
            native_second_changed: true,
            ..RtcEvents::default()
        })
    }
    fn correct_minute(&mut self) -> Result<(), String> {
        let mut shadow = self
            .shadow
            .ok_or("RTC düzeltmesi için başlangıç zamanı gerekli")?;
        if self.profile == Some(RtcProfile::VirtualGregorian) {
            valid_calendar(shadow)?;
            shadow = if shadow[6] >= 30 {
                add_gregorian(shadow, u64::from(60 - shadow[6]))?
            } else {
                shadow[6] = 0;
                shadow
            };
            self.fraction_us = 0;
        } else {
            if shadow[6] >= 30 {
                shadow[5] = shadow[5].wrapping_add(1);
                if shadow[5] > 59 {
                    shadow[5] = 0;
                    shadow[4] = shadow[4].wrapping_add(1);
                    if shadow[4] > 23 {
                        shadow[4] = 0;
                    }
                }
            }
            shadow[6] = 0;
        }
        self.shadow = Some(shadow);
        Ok(())
    }
}

/// Native scan start does not publish/advance RTC. TRD publishes on demand;
/// end_scan below follows the old engine's public-refresh-before-advance order.
pub fn begin_scan(_m: &mut Memory) {}

pub fn refresh_public(m: &mut Memory) -> Result<(), String> {
    let shadow = m
        .rtc_clock
        .shadow()
        .ok_or("RTC başlangıç zamanı yapılandırılmadı; açık seed veya TWR gerekli")?;
    for (n, raw) in shadow.into_iter().enumerate() {
        let value = u16::from(raw)
            + if n == 0 && m.system_bit(1016) {
                2000
            } else {
                0
            };
        m.set_word(data(1319 - n as u16), value as i16);
    }
    Ok(())
}

/// No elapsed input is inferred. The caller chooses Hold, monotonic microseconds
/// or an observed native-second sample. Installed events can feed HourState.
pub fn end_scan(m: &mut Memory, advance: RtcAdvance) -> RtcEvents {
    if m.rtc_clock.profile().is_none() {
        return RtcEvents::default();
    }
    if let Err(reason) = refresh_public(m) {
        m.execution_limitations.insert(reason);
        return RtcEvents::default();
    }
    let result = match advance {
        RtcAdvance::Hold => Ok(RtcEvents::default()),
        RtcAdvance::ElapsedMicros(us) => m.rtc_clock.advance_monotonic_us(us),
        RtcAdvance::InstalledSampledSecond(second) => m.rtc_clock.sample_installed_second(second),
    };
    let events = match result {
        Ok(events) => events,
        Err(reason) => {
            m.execution_limitations.insert(reason);
            RtcEvents::default()
        }
    };
    let on = m.system_bit(1017);
    let correct = on
        && (m.rtc_clock.profile() != Some(RtcProfile::VirtualGregorian)
            || !m.rtc_clock.correction_was_on);
    m.rtc_clock.correction_was_on = on;
    if correct {
        match m.rtc_clock.correct_minute() {
            Ok(()) => m.set_system(1082, true),
            Err(reason) => {
                m.execution_limitations.insert(reason);
            }
        }
    }
    events
}

/// Settle the installed publish/tick/level-correction service at an explicitly
/// supplied second. Two services reach its fixed point, including M1017 ON.
/// This models an endpoint, not the number of native worker iterations, and
/// neither observes nor accepts native RTC results as new local clock state.
pub fn settle_installed_service(m: &mut Memory, sampled_second: u8) -> Result<RtcEvents, String> {
    if m.rtc_service_schedule != RtcServiceSchedule::ExplicitBoundaries {
        return Err("RTC settled service requires explicit boundary scheduling".into());
    }
    if sampled_second > 59
        || m.rtc_clock.profile() != Some(RtcProfile::InstalledShadow)
        || m.rtc_clock.shadow().is_none()
        || m.rtc_clock.previous_native_second().is_none_or(|s| s > 59)
    {
        return Err(
            "RTC settled service requires an initial installed seed and valid sample phase".into(),
        );
    }
    let first = end_scan(m, RtcAdvance::InstalledSampledSecond(sampled_second));
    let second = end_scan(m, RtcAdvance::InstalledSampledSecond(sampled_second));
    Ok(RtcEvents {
        native_second_changed: first.native_second_changed || second.native_second_changed,
        virtual_elapsed_seconds: 0,
    })
}

/// Execute one source-VM scan under a declared, constant external RTC sample.
/// Used for independent fixed-input conformance fixtures; no host time or
/// reference output is read. Explicit scheduling must already be selected.
pub fn scan_with_installed_sample(
    simulator: &mut crate::plc::Simulator,
    program: &crate::plc::Program,
    scan_ms: u64,
    sampled_second: u8,
) -> Result<(), String> {
    if simulator.current_plan_pc().is_some() {
        return Err("RTC fixed-input scan cannot replace an active cursor".into());
    }
    settle_installed_service(&mut simulator.memory, sampled_second)?;
    simulator.begin_plan_scan(program, scan_ms, true)?;
    while simulator.current_plan_pc().is_some() {
        let step = simulator.execute_plan_step(program)?;
        settle_installed_service(&mut simulator.memory, sampled_second)?;
        if step.scan_boundary {
            break;
        }
    }
    Ok(())
}

pub(crate) fn normalize_twr_public(m: &mut Memory) -> bool {
    let mut bad = false;
    let year = m.word(data(1319));
    if (0..=99).contains(&year) {
    } else if (2000..=2099).contains(&year) {
        m.set_word(data(1319), year - 2000);
    } else {
        m.set_word(data(1319), 0);
        bad = true;
    }
    for (index, low, high, minimum) in [
        (1318, 1, 7, 1),
        (1317, 1, 12, 1),
        (1316, 1, 31, 1),
        (1315, 0, 23, 0),
        (1314, 0, 59, 0),
        (1313, 0, 59, 0),
    ] {
        if !(low..=high).contains(&m.word(data(index))) {
            m.set_word(data(index), minimum);
            bad = true;
        }
    }
    !bad
}

pub(crate) fn finish_twr(m: &mut Memory, valid: bool) -> RtcOutcome {
    if !valid {
        m.operation_error(0x0e1a, 3);
    } else {
        let shadow = std::array::from_fn(|n| m.word(data(1319 - n as u16)) as u8);
        m.rtc_clock.commit_twr(shadow);
        if m.rtc_clock.profile() == Some(RtcProfile::FrozenSeed) {
            m.timing_io.notices.insert("RTC: TWR açık başlangıç değerini kaydetti; zaman ilerlemesi seçilmediği için saat sabit. Sanal Gregoryen veya kurulu saat tekrar profili ayrıca yapılandırılmalı.".into());
        }
        if m.system_bit(1016) {
            let year = m.word(data(1319));
            m.set_word(data(1319), year + 2000);
        }
    }
    m.set_system(1082, true);
    if valid {
        RtcOutcome::Complete
    } else {
        RtcOutcome::PlcError { offset: 3 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum RtcOutcome {
    Nominal,
    Complete,
    PartialRead { written: u8 },
    PlcError { offset: u16 },
    Limitation(String),
}

pub(crate) fn copy_live_word(m: &mut Memory, source: Device, dest: Device) {
    let value = m.word(source);
    m.set_word(dest, value);
}

#[derive(Debug, Clone, PartialEq)]
pub enum RtcAction {
    Read { dest: Device },
    Write { source: Device },
}
impl RtcAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !matches!(name, "TRD" | "TWR") {
            return Err("Bilinmeyen RTC komutu".into());
        }
        if args.len() != 1 {
            return Err(format!("{name}: bir T/C/D operandı gerekli"));
        }
        let device: Device = args[0].parse()?;
        word_range(device, 1)?;
        Ok(if name == "TRD" {
            Self::Read { dest: device }
        } else {
            Self::Write { source: device }
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Read { .. } => "TRD",
            Self::Write { .. } => "TWR",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Read { .. } => 0x6a01,
            Self::Write { .. } => 0x6c01,
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Read { dest } => *dest,
            Self::Write { source } => *source,
        }
    }
    pub fn label(&self) -> String {
        format!("{} {}", self.name(), self.dest())
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.insert(self.dest());
        out.extend((1313..=1319).map(data));
        out.extend([1016, 1017, 1082].map(Memory::system_device));
        Memory::error_devices(out);
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        let base = self.dest();
        let last = word_limit(base, m);
        for n in 0..7 {
            if let Some(index) = base.index.checked_add(n).filter(|index| *index <= last) {
                out.insert(Device {
                    area: base.area,
                    index,
                });
            }
        }
    }
    pub fn execute(&self, m: &mut Memory) -> RtcOutcome {
        match *self {
            Self::Read { dest } => {
                if let Err(reason) = refresh_public(m) {
                    m.execution_limitations.insert(reason.clone());
                    return RtcOutcome::Limitation(reason);
                }
                for n in 0..7 {
                    let Some(index) = dest
                        .index
                        .checked_add(n)
                        .filter(|i| *i <= word_limit(dest, m))
                    else {
                        return RtcOutcome::PartialRead { written: n as u8 };
                    };
                    copy_live_word(
                        m,
                        data(1319 - n),
                        Device {
                            area: dest.area,
                            index,
                        },
                    );
                }
                RtcOutcome::Complete
            }
            Self::Write { source } => {
                if source
                    .index
                    .checked_add(6)
                    .is_none_or(|end| end > word_limit(source, m))
                {
                    m.operation_error(0xe1a, 3);
                    return RtcOutcome::PlcError { offset: 3 };
                }
                for n in 0..7 {
                    copy_live_word(m, offset(source, n), data(1319 - n));
                }
                let valid = normalize_twr_public(m);
                finish_twr(m, valid)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn read(dest: u16) -> RtcAction {
        RtcAction::Read { dest: data(dest) }
    }
    fn write(source: u16) -> RtcAction {
        RtcAction::Write {
            source: data(source),
        }
    }
    fn put(m: &mut Memory, base: u16, fields: [i16; 7]) {
        for (n, v) in fields.into_iter().enumerate() {
            m.set_word(data(base + n as u16), v);
        }
    }
    fn words(m: &Memory, base: u16) -> Vec<i16> {
        (0..7).map(|n| m.word(data(base + n))).collect()
    }
    #[test]
    fn unseeded_read_holds_and_twr_is_an_explicit_frozen_seed() {
        let mut m = Memory::default();
        m.set_word(data(100), 123);
        read(100).execute(&mut m);
        assert_eq!(m.word(data(100)), 123);
        assert!(!m.execution_limitations.is_empty());
        m.execution_limitations.clear();
        put(&mut m, 10, [2024, 4, 2, 29, 23, 59, 58]);
        write(10).execute(&mut m);
        assert_eq!(m.rtc_clock.profile(), Some(RtcProfile::FrozenSeed));
        read(100).execute(&mut m);
        assert_eq!(words(&m, 100), [24, 4, 2, 29, 23, 59, 58]);
        assert!(m.system_bit(1082));
        m.set_system(1016, true);
        read(100).execute(&mut m);
        assert_eq!(m.word(data(100)), 2024);
    }
    #[test]
    fn twr_corrects_each_public_field_but_holds_private_clock_on_error() {
        let mut m = Memory::default();
        m.rtc_clock.configure_frozen([24, 3, 7, 4, 5, 6, 7]);
        put(&mut m, 10, [100, 0, 13, 0, 24, -1, 60]);
        write(10).execute(&mut m);
        assert_eq!(
            (1313..=1319)
                .rev()
                .map(|n| m.word(data(n)))
                .collect::<Vec<_>>(),
            [0, 1, 1, 1, 0, 0, 0]
        );
        assert_eq!(m.rtc_clock.shadow(), Some([24, 3, 7, 4, 5, 6, 7]));
        assert!(m.system_bit(1067));
        assert!(m.system_bit(1068));
        assert_eq!(m.word(data(1067)), 0x0e1a);
        assert_eq!(m.word(data(1068)), 3);
        assert!(m.system_bit(1082));
    }
    #[test]
    fn native_field_contract_accepts_february31_and_independent_weekday() {
        let mut m = Memory::default();
        put(&mut m, 10, [24, 7, 2, 31, 12, 13, 14]);
        write(10).execute(&mut m);
        assert_eq!(m.rtc_clock.shadow(), Some([24, 7, 2, 31, 12, 13, 14]));
        assert!(!m.system_bit(1067));
    }
    #[test]
    fn trd_is_sequential_through_special_register_aliases() {
        let mut m = Memory::default();
        m.rtc_clock.configure_frozen([24, 3, 7, 4, 5, 6, 7]);
        read(1316).execute(&mut m);
        // D1316 overwrites the live day source before the fourth copy reads it.
        assert_eq!(words(&m, 1316), [24, 3, 3, 24, 5, 6, 7]);
        assert_eq!(m.rtc_clock.shadow(), Some([24, 3, 7, 4, 5, 6, 7]));
    }
    #[test]
    fn area_end_read_is_partial_without_plc_error_write_is_upfront_error() {
        for (area, index) in [(Area::T, 254), (Area::C, 198), (Area::D, 4998)] {
            let mut m = Memory::default();
            m.data_register_limit = Some(4999);
            m.rtc_clock.configure_frozen([24, 3, 7, 4, 5, 6, 7]);
            let base = Device { area, index };
            RtcAction::Read { dest: base }.execute(&mut m);
            assert_eq!(m.word(base), 24);
            assert_eq!(m.word(offset(base, 1)), 3);
            assert!(!m.system_bit(1067));
            m.set_system(1082, false);
            RtcAction::Write { source: base }.execute(&mut m);
            assert_eq!(m.word(data(1067)), 0x0e1a);
            assert!(!m.system_bit(1082));
        }
    }
    #[test]
    fn virtual_calendar_and_installed_midnight_are_deliberately_different() {
        let mut virtual_clock = RtcClockState::default();
        virtual_clock
            .configure_virtual([2024, 3, 2, 28, 23, 59, 59])
            .unwrap();
        virtual_clock.advance_monotonic_us(500_000).unwrap();
        assert_eq!(virtual_clock.shadow().unwrap()[6], 59);
        virtual_clock.advance_monotonic_us(500_000).unwrap();
        assert_eq!(virtual_clock.shadow(), Some([24, 4, 2, 29, 0, 0, 0]));
        let mut installed = RtcClockState::default();
        installed
            .configure_installed_shadow([24, 3, 2, 28, 23, 59, 59], 59)
            .unwrap();
        assert!(
            installed
                .sample_installed_second(0)
                .unwrap()
                .native_second_changed
        );
        assert_eq!(installed.shadow(), Some([24, 3, 2, 28, 0, 0, 0]));
        assert_eq!(
            installed.sample_installed_second(0).unwrap(),
            RtcEvents::default()
        );
    }
    #[test]
    fn scan_end_publishes_before_advance_and_profiles_choose_correction_policy() {
        let mut m = Memory::default();
        m.rtc_clock
            .configure_installed_shadow([24, 3, 7, 4, 12, 59, 30], 10)
            .unwrap();
        m.set_system(1017, true);
        let event = end_scan(&mut m, RtcAdvance::InstalledSampledSecond(11));
        assert!(event.native_second_changed);
        assert_eq!(m.word(data(1313)), 30);
        assert_eq!(m.rtc_clock.shadow(), Some([24, 3, 7, 4, 13, 0, 0]));
        assert!(m.system_bit(1082));
        end_scan(&mut m, RtcAdvance::InstalledSampledSecond(12));
        assert_eq!(m.rtc_clock.shadow().unwrap()[6], 0);
        let mut m = Memory::default();
        m.rtc_clock
            .configure_virtual([2024, 3, 7, 4, 12, 59, 30])
            .unwrap();
        m.set_system(1017, true);
        end_scan(&mut m, RtcAdvance::Hold);
        end_scan(&mut m, RtcAdvance::ElapsedMicros(1_000_000));
        assert_eq!(m.rtc_clock.shadow().unwrap()[6], 1);
    }
    #[test]
    fn native_second_samples_coalesce_and_repeat_does_not_imply_elapsed_minutes() {
        let mut clock = RtcClockState::default();
        clock
            .configure_installed_shadow([24, 3, 7, 4, 5, 6, 7], 10)
            .unwrap();
        let event = clock.sample_installed_second(15).unwrap();
        assert!(event.native_second_changed);
        assert_eq!(event.virtual_elapsed_seconds, 0);
        assert_eq!(clock.shadow().unwrap()[6], 12);
        assert_eq!(
            clock.sample_installed_second(15).unwrap(),
            RtcEvents::default()
        );
        // Backward host samples become a positive modulo60 delta in the old runtime.
        clock.sample_installed_second(14).unwrap();
        assert_eq!(clock.shadow().unwrap()[5..], [7, 11]);
    }
    #[test]
    fn unproved_century_rollover_and_wrong_clock_backend_hold_state() {
        let mut clock = RtcClockState::default();
        clock
            .configure_virtual([2099, 4, 12, 31, 23, 59, 59])
            .unwrap();
        let original = clock.shadow();
        assert!(clock.advance_monotonic_us(1_000_000).is_err());
        assert_eq!(clock.shadow(), original);
        assert_eq!(clock.fraction_us(), 0);
        assert!(clock.sample_installed_second(1).is_err());
        assert_eq!(clock.shadow(), original);
    }
    #[test]
    fn twr_source_alias_reads_live_words_not_a_snapshot() {
        let mut m = Memory::default();
        put(&mut m, 1313, [24, 3, 7, 4, 5, 6, 7]);
        write(1313).execute(&mut m);
        assert_eq!(m.rtc_clock.shadow(), Some([24, 3, 7, 4, 7, 3, 24]));
    }
}
