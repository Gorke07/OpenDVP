//! HOUR/DHOUR have two explicit clocks: a replay of the installed four-owner
//! tick bitmap, and a deterministic cumulative enabled-time virtual profile.
//! Neither profile reads host time or treats scan_ms as an observed native tick.
use crate::calendar_operand::CalendarValue;
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HourProfile {
    /// Actual installed slot/bitmap mechanism; callers inject native events.
    InstalledTickBitmap,
    /// Named local profile: monotonic ON-time, retained fractional seconds.
    #[default]
    VirtualElapsed,
}

#[derive(Debug, Clone)]
struct VirtualRun {
    hours: Device,
    wide: bool,
    claimed: bool,
    running: bool,
    last_us: u64,
    pending_us: u128,
    expected_hours: i32,
    expected_seconds: i16,
}

#[derive(Debug, Clone, Default)]
pub struct HourState {
    pub profile: HourProfile,
    /// Exact installed representation, including zero sentinel/sign-extension
    /// quirks. Pending bits belong to slot positions, not instruction owners.
    pub native_owners: [u16; 4],
    pub pending_ticks: u8,
    pub notices: BTreeSet<String>,
    virtual_runs: BTreeMap<u16, VirtualRun>,
}

impl HourState {
    /// Switching clock policies resets hidden ownership/timing, never user D
    /// counters or the output coil. Seed replay state explicitly afterward.
    pub fn select_profile(&mut self, profile: HourProfile) {
        if self.profile != profile {
            self.profile = profile;
            self.native_owners = [0; 4];
            self.pending_ticks = 0;
            self.virtual_runs.clear();
            self.notices.clear();
        }
    }
    /// Explicit snapshot/replay seam; no inference from a host/scan clock.
    pub fn set_native_pending_ticks(&mut self, mask: u8) {
        self.pending_ticks = mask;
    }
    pub fn inject_native_tick_bits(&mut self, mask: u8) {
        self.pending_ticks |= mask;
    }
    /// The inspected RTC helper sets FF on any sampled host-second change,
    /// even when several seconds elapsed. It does not queue multiple seconds.
    pub fn signal_native_second_change(&mut self) {
        self.pending_ticks = 0xff;
    }
    pub fn reset_virtual_fraction(&mut self, pc: u16) {
        self.virtual_runs.remove(&pc);
    }

    fn native_claim(&mut self, pc: u16) -> Option<usize> {
        for index in 0..4 {
            if i32::from(self.native_owners[index] as i16) == i32::from(pc) {
                return Some(index);
            }
            if self.native_owners[index] == 0 {
                self.native_owners[index] = pc;
                return Some(index);
            }
        }
        None
    }
    fn native_release(&mut self, pc: u16) {
        if let Some(index) = self
            .native_owners
            .iter()
            .position(|owner| i32::from(*owner as i16) == i32::from(pc))
        {
            self.native_owners.copy_within(index + 1..4, index);
            self.native_owners[3] = 0;
            // Deliberately do not move/clear pending bits when slots compact.
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HourAction {
    pub source: CalendarValue,
    pub hours: Device,
    pub dest: Device,
}

fn at(base: Device, offset: u16) -> Device {
    Device {
        area: Area::D,
        index: base.index + offset,
    }
}

impl HourAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !matches!(name, "HOUR" | "DHOUR") || args.len() != 3 {
            return Err(format!(
                "{name}: S D1 D2 gerekli; HOURP/DHOURP tanımlı değil"
            ));
        }
        let source = CalendarValue::parse(&args[0], name == "DHOUR")?;
        let hours: Device = args[1].parse()?;
        let dest: Device = args[2].parse()?;
        if hours.area != Area::D {
            return Err("HOUR D1 bir D register adresi olmalı".into());
        }
        if !dest.writable_bit() {
            return Err("HOUR D2 hedefi Y/M/S biti olmalı".into());
        }
        // The actual installed body reports a model-dependent E1A for a
        // legal base whose implicit 2/3-word block does not fit. Keep that
        // executable error case instead of rejecting it as parser syntax.
        Ok(Self {
            source,
            hours,
            dest,
        })
    }
    pub fn wide(&self) -> bool {
        self.source.wide()
    }
    pub fn name(&self) -> &'static str {
        if self.wide() { "DHOUR" } else { "HOUR" }
    }
    pub fn header(&self) -> u16 {
        if self.wide() { 0x7101 } else { 0x7001 }
    }
    pub fn destination(&self) -> Device {
        self.dest
    }
    pub fn dest(&self) -> Device {
        self.dest
    }
    pub fn label(&self) -> String {
        format!(
            "{} {} {} {}",
            self.name(),
            self.source,
            self.hours,
            self.dest
        )
    }
    pub fn state_words(&self) -> u16 {
        if self.wide() { 3 } else { 2 }
    }
    fn seconds_device(&self) -> Device {
        at(self.hours, if self.wide() { 2 } else { 1 })
    }
    fn max_hours(&self) -> i32 {
        if self.wide() {
            i32::MAX
        } else {
            i32::from(i16::MAX)
        }
    }
    fn read_hours(&self, m: &Memory) -> i32 {
        if self.wide() {
            m.double_word(self.hours)
        } else {
            i32::from(m.word(self.hours))
        }
    }
    fn write_counter(&self, m: &mut Memory, hours: i32, seconds: i16) {
        if self.wide() {
            m.set_double_word(self.hours, hours);
        } else {
            m.set_word(self.hours, hours as i16);
        }
        m.set_word(self.seconds_device(), seconds);
    }
    fn block_valid(&self, m: &Memory) -> bool {
        self.hours
            .index
            .checked_add(self.state_words() - 1)
            .is_some_and(|end| end <= m.data_register_limit.unwrap_or(9999))
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.source.devices(out);
        out.insert(self.hours);
        out.insert(self.dest);
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
        // Model-dependent implicit words are observed only when reachable.
        // This preserves SS2 base4999's genuine E1A instead of accidentally
        // rejecting an artificial static observation D5000 before upload.
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if self.block_valid(m) {
            out.extend((0..self.state_words()).map(|i| at(self.hours, i)));
        }
    }
    fn write_output(&self, m: &mut Memory, hours: i32, setpoint: i32) {
        if self.dest.area == Area::M && self.dest.index > m.relay_limit.unwrap_or(4095) {
            m.execution_limitations.insert("HOUR: Çıkış rölesi model sınırını aşıyor; sayaç güncellemesinden sonraki çıkış yazımı uygulanmadı.".into());
            return;
        }
        m.set_internal(self.dest, hours >= setpoint);
    }
    /// Stable compiled WORD PC is required; a source action index is not the
    /// installed owner identity. now_us is explicit monotonic virtual time.
    pub fn execute(&self, m: &mut Memory, power: bool, native_action_pc: u16, now_us: u64) {
        let mut state = std::mem::take(&mut m.hour_meter);
        match state.profile {
            HourProfile::InstalledTickBitmap => {
                self.execute_native(m, &mut state, power, native_action_pc)
            }
            HourProfile::VirtualElapsed => {
                self.execute_virtual(m, &mut state, power, native_action_pc, now_us)
            }
        }
        m.hour_meter = state;
    }
    fn execute_native(&self, m: &mut Memory, state: &mut HourState, power: bool, pc: u16) {
        if !power {
            state.native_release(pc);
            return;
        }
        let Some(slot) = state.native_claim(pc) else {
            return;
        };
        // Ownership precedes every source/destination validation in native.
        let setpoint = self.source.read(m);
        if setpoint <= 0 {
            return;
        }
        if !self.wide() {
            m.native_numeric_scratch = Some(setpoint as i16);
        }
        if !self.block_valid(m) {
            m.operation_error(0x0e1a, if self.wide() { 5 } else { 3 });
            return;
        }
        let mut hours = self.read_hours(m);
        if hours < 0 {
            return;
        }
        if state.pending_ticks & (1 << slot) != 0 {
            state.pending_ticks &= !(1 << slot);
            let mut seconds = m.word(self.seconds_device());
            if hours == self.max_hours() && seconds >= 3599 {
                seconds = 3599;
            } else {
                seconds = seconds.wrapping_add(1);
                if seconds >= 3600 && hours < self.max_hours() {
                    seconds = 0;
                    hours += 1;
                }
            }
            self.write_counter(m, hours, seconds);
        }
        self.write_output(m, hours, setpoint);
    }
    fn execute_virtual(
        &self,
        m: &mut Memory,
        state: &mut HourState,
        power: bool,
        pc: u16,
        now_us: u64,
    ) {
        state.notices.insert("HOUR: Belirli sanal ON-zamanı profili kullanılıyor; kesirler ve devre dışı bırakmadan önceki süre korunur, aygıtın tick/kalıcılık davranışıyla eşdeğerlik iddia edilmez.".into());
        let already_claimed = state.virtual_runs.get(&pc).is_some_and(|run| run.claimed);
        if power
            && !already_claimed
            && state
                .virtual_runs
                .values()
                .filter(|run| run.claimed)
                .count()
                >= 4
        {
            m.execution_limitations.insert("HOUR/DHOUR: Dört etkin sanal saat sayacı sınırına ulaşıldı; beşinci komut uygulanmadı.".into());
            return;
        }
        if !state.virtual_runs.contains_key(&pc) {
            if !power {
                return;
            }
            if state.virtual_runs.len() >= 512 {
                m.execution_limitations
                    .insert("HOUR: Sanal komut durumu sınırına ulaşıldı.".into());
                return;
            }
            // An invalid endpoint is not read during state construction.
            let (hours, seconds) = if self.block_valid(m) {
                (self.read_hours(m), m.word(self.seconds_device()))
            } else {
                (0, 0)
            };
            state.virtual_runs.insert(
                pc,
                VirtualRun {
                    hours: self.hours,
                    wide: self.wide(),
                    claimed: false,
                    running: false,
                    last_us: now_us,
                    pending_us: 0,
                    expected_hours: hours,
                    expected_seconds: seconds,
                },
            );
        }
        let run = state
            .virtual_runs
            .get_mut(&pc)
            .expect("known virtual owner");
        if run.hours != self.hours || run.wide != self.wide() {
            m.execution_limitations.insert(
                "HOUR: Aynı komut adresinin sayaç düzeni değişti; sanal durum sıfırlanmalı.".into(),
            );
            return;
        }
        if now_us < run.last_us {
            m.execution_limitations
                .insert("HOUR: Sanal saat geriye ilerletilemez.".into());
            return;
        }
        // Observable preload/reset writes establish a new virtual phase at
        // this visit. No fictional exact timestamp is assigned to that write.
        if self.block_valid(m) {
            let actual = (self.read_hours(m), m.word(self.seconds_device()));
            if actual != (run.expected_hours, run.expected_seconds) {
                run.pending_us = 0;
                run.last_us = now_us;
                (run.expected_hours, run.expected_seconds) = actual;
                state.notices.insert("HOUR: Sayaç registerlarına dış yazım algılandı; sanal kesir fazı bu komut ziyaretinde yeniden başlatıldı.".into());
            }
        }
        if run.running {
            run.pending_us += u128::from(now_us - run.last_us);
        }
        run.last_us = now_us;
        run.claimed = power;
        run.running = false;
        // False execution preserves observable counters/output. Previously
        // earned ON time waits internally until a subsequent enabled visit.
        if !power {
            return;
        }
        let setpoint = self.source.read(m);
        if setpoint <= 0 {
            return;
        }
        if !self.wide() {
            m.native_numeric_scratch = Some(setpoint as i16);
        }
        if !self.block_valid(m) {
            m.operation_error(0x0e1a, if self.wide() { 5 } else { 3 });
            return;
        }
        let hours = self.read_hours(m);
        if hours < 0 {
            return;
        }
        let seconds = m.word(self.seconds_device());
        if !(0..=3599).contains(&seconds) {
            m.execution_limitations.insert("HOUR: Sanal geçen-süre profilinde 0..3599 dışındaki saniye ön yüklemesi tanımlı değil; kurulu tick profili ayrı seçilebilir.".into());
            return;
        }
        run.running = true;
        let ticks = run.pending_us / 1_000_000;
        run.pending_us %= 1_000_000;
        let total = (u128::from(hours as u32) * 3600 + seconds as u128 + ticks)
            .min(u128::from(self.max_hours() as u32) * 3600 + 3599);
        let new_hours = (total / 3600) as i32;
        let new_seconds = (total % 3600) as i16;
        self.write_counter(m, new_hours, new_seconds);
        run.expected_hours = new_hours;
        run.expected_seconds = new_seconds;
        self.write_output(m, new_hours, setpoint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(raw: &str) -> Device {
        raw.parse().unwrap()
    }
    fn action(name: &str, args: &[&str]) -> HourAction {
        HourAction::parse(
            name,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn native() -> Memory {
        let mut m = Memory::default();
        m.hour_meter
            .select_profile(HourProfile::InstalledTickBitmap);
        m
    }
    #[test]
    fn native_ticks_are_explicit_coalescing_and_not_elapsed_scan_guesses() {
        let mut m = native();
        let a = action("HOUR", &["K1", "D0", "Y0"]);
        a.execute(&mut m, true, 3, 0);
        a.execute(&mut m, true, 3, 10_000_000);
        assert_eq!(m.word(d("D1")), 0);
        m.hour_meter.signal_native_second_change();
        m.hour_meter.signal_native_second_change();
        a.execute(&mut m, true, 3, 20_000_000);
        assert_eq!(m.word(d("D1")), 1);
        a.execute(&mut m, true, 3, 100_000_000);
        assert_eq!(m.word(d("D1")), 1);
        assert_eq!(m.hour_meter.pending_ticks, 0xfe);
    }
    #[test]
    fn invalid_setpoints_claim_quota_and_compaction_does_not_move_ticks() {
        let mut m = native();
        let invalid = action("HOUR", &["K0", "D0", "Y0"]);
        for pc in [3, 10, 17, 24] {
            invalid.execute(&mut m, true, pc, 0);
        }
        assert_eq!(m.hour_meter.native_owners, [3, 10, 17, 24]);
        let a = action("DHOUR", &["K1", "D10", "Y1"]);
        m.hour_meter.set_native_pending_ticks(4);
        a.execute(&mut m, true, 31, 0);
        assert_eq!(m.word(d("D12")), 0);
        invalid.execute(&mut m, false, 10, 0);
        assert_eq!(m.hour_meter.native_owners, [3, 17, 24, 0]);
        a.execute(&mut m, true, 17, 0);
        assert_eq!(m.word(d("D12")), 0);
        a.execute(&mut m, true, 24, 0);
        assert_eq!(m.word(d("D12")), 1);
        assert_eq!(m.hour_meter.pending_ticks, 0);
    }
    #[test]
    fn installed_pc_zero_and_signed_high_pc_quirks_are_preserved_only_in_replay() {
        let mut state = HourState::default();
        assert_eq!(state.native_claim(0), Some(0));
        assert_eq!(state.native_owners, [0; 4]);
        assert_eq!(state.native_claim(7), Some(0));
        assert_eq!(state.native_claim(0), Some(1));
        state.native_owners = [0; 4];
        for slot in 0..4 {
            assert_eq!(state.native_claim(0x8000), Some(slot));
        }
        state.native_release(0x8000);
        assert_eq!(state.native_owners, [0x8000; 4]);
        assert_eq!(state.native_claim(0x8000), None);
    }
    #[test]
    fn native_rollover_saturation_and_signed_preloaded_seconds() {
        for (name, initial, max, seconds_offset) in [
            ("HOUR", 32766, 32767, 1),
            ("DHOUR", 2147483646, 2147483647, 2),
        ] {
            let mut m = native();
            let a = action(name, &["K1", "D0", "Y0"]);
            if a.wide() {
                m.set_double_word(d("D0"), initial);
            } else {
                m.set_word(d("D0"), initial as i16);
            }
            m.set_word(at(d("D0"), seconds_offset), 3599);
            m.hour_meter.set_native_pending_ticks(1);
            a.execute(&mut m, true, 3, 0);
            assert_eq!(a.read_hours(&m), max);
            assert_eq!(m.word(a.seconds_device()), 0);
            m.set_word(a.seconds_device(), 3599);
            m.hour_meter.inject_native_tick_bits(1);
            a.execute(&mut m, true, 3, 0);
            assert_eq!(a.read_hours(&m), max);
            assert_eq!(m.word(a.seconds_device()), 3599);
        }
        let mut m = native();
        let a = action("HOUR", &["K1", "D0", "Y0"]);
        m.set_word(d("D1"), -2);
        m.hour_meter.set_native_pending_ticks(1);
        a.execute(&mut m, true, 3, 0);
        assert_eq!(m.word(d("D1")), -1);
        m.set_word(d("D1"), 32767);
        m.hour_meter.inject_native_tick_bits(1);
        a.execute(&mut m, true, 3, 0);
        assert_eq!(m.word(d("D1")), i16::MIN);
        assert_eq!(m.word(d("D0")), 0);
    }
    #[test]
    fn native_source_snapshot_false_hold_negative_hours_and_flags() {
        let mut m = native();
        let a = action("HOUR", &["D0", "D0", "Y0"]);
        m.set_word(d("D0"), 1);
        m.set_word(d("D1"), 3599);
        for index in 1020..=1022 {
            m.set_system(index, true);
        }
        m.hour_meter.set_native_pending_ticks(1);
        a.execute(&mut m, true, 3, 0);
        assert_eq!(m.word(d("D0")), 2);
        assert!(m.bit(d("Y0")));
        assert_eq!(m.native_numeric_scratch, Some(1));
        a.execute(&mut m, false, 3, 20_000_000);
        assert!(m.bit(d("Y0")));
        assert_eq!(m.word(d("D0")), 2);
        let a = action("HOUR", &["K1", "D0", "Y0"]);
        m.set_word(d("D0"), -1);
        m.hour_meter.set_native_pending_ticks(1);
        a.execute(&mut m, true, 3, 0);
        assert_eq!(m.hour_meter.pending_ticks, 1);
        assert!(m.bit(d("Y0")));
        for index in 1020..=1022 {
            assert!(m.system_bit(index));
        }
    }
    #[test]
    fn model_end_error_precedes_tick_and_source_invalid_skips_destination_check() {
        let mut m = native();
        m.data_register_limit = Some(4999);
        let a = action("HOUR", &["K1", "D4999", "Y0"]);
        m.hour_meter.set_native_pending_ticks(1);
        a.execute(&mut m, true, 3, 0);
        assert_eq!(m.word(d("D1067")), 0x0e1a);
        assert_eq!(m.hour_meter.pending_ticks, 1);
        let mut observed = BTreeSet::new();
        a.observed_devices(&m, &mut observed);
        assert!(!observed.contains(&d("D5000")));
        let mut m = native();
        let a = action("DHOUR", &["K0", "D9999", "Y0"]);
        a.execute(&mut m, true, 3, 0);
        assert!(!m.system_bit(1067));
        assert_eq!(m.hour_meter.native_owners[0], 3);
    }
    #[test]
    fn virtual_profile_preserves_earned_fraction_without_counting_disabled_time() {
        let mut m = Memory::default();
        let a = action("HOUR", &["K1", "D0", "Y0"]);
        a.execute(&mut m, true, 0, 0);
        a.execute(&mut m, false, 0, 1_500_000);
        assert_eq!(m.word(d("D1")), 0);
        assert!(!m.bit(d("Y0")));
        a.execute(&mut m, true, 0, 10_000_000);
        assert_eq!(m.word(d("D1")), 1);
        a.execute(&mut m, true, 0, 10_500_000);
        assert_eq!(m.word(d("D1")), 2);
        a.execute(&mut m, true, 0, 3_608_500_000);
        assert_eq!(m.word(d("D0")), 1);
        assert_eq!(m.word(d("D1")), 0);
        assert!(m.bit(d("Y0")));
        a.execute(&mut m, true, 0, 3_609_500_000);
        assert_eq!(m.word(d("D1")), 1);
    }
    #[test]
    fn virtual_external_reset_restarts_phase_and_backward_clock_is_explicit() {
        let mut m = Memory::default();
        let a = action("DHOUR", &["K40000", "D0", "Y0"]);
        a.execute(&mut m, true, 3, 0);
        a.execute(&mut m, true, 3, 1_500_000);
        assert_eq!(m.word(d("D2")), 1);
        m.set_double_word(d("D0"), 0);
        m.set_word(d("D2"), 0);
        a.execute(&mut m, true, 3, 2_000_000);
        a.execute(&mut m, true, 3, 2_500_000);
        assert_eq!(m.word(d("D2")), 0);
        a.execute(&mut m, true, 3, 3_000_000);
        assert_eq!(m.word(d("D2")), 1);
        a.execute(&mut m, true, 3, 2_000_000);
        assert!(!m.execution_limitations.is_empty());
    }
    #[test]
    fn virtual_quota_and_saturation_are_bounded_not_fifth_silent_noops() {
        let mut m = Memory::default();
        let a = action("HOUR", &["K1", "D0", "Y0"]);
        for pc in [0, 7, 14, 21] {
            a.execute(&mut m, true, pc, 0);
        }
        a.execute(&mut m, true, 28, 0);
        assert!(!m.execution_limitations.is_empty());
        a.execute(&mut m, false, 7, 0);
        a.execute(&mut m, true, 28, 0);
        assert!(m.hour_meter.virtual_runs[&28].claimed);
        let mut m = Memory::default();
        m.set_word(d("D0"), 32767);
        m.set_word(d("D1"), 3598);
        a.execute(&mut m, true, 3, 0);
        a.execute(&mut m, true, 3, u64::MAX);
        assert_eq!(m.word(d("D0")), 32767);
        assert_eq!(m.word(d("D1")), 3599);
    }
    #[test]
    fn matrices_grouped_setpoints_and_nonexistent_forms_are_explicit() {
        for source in [
            "K4X0", "K1Y0", "K2M2000", "K3S0", "T0", "C199", "D0", "K1", "H7FFF", "E0", "F7",
        ] {
            assert!(HourAction::parse("HOUR", &[source, "D0", "Y0"].map(str::to_string)).is_ok());
        }
        assert!(HourAction::parse("DHOUR", &["C200", "D0", "M2000"].map(str::to_string)).is_ok());
        for (name, args) in [
            ("HOURP", ["K1", "D0", "Y0"]),
            ("HOUR", ["K1", "T0", "Y0"]),
            ("HOUR", ["K1", "D0", "X0"]),
            ("DHOUR", ["F0", "D0", "Y0"]),
        ] {
            assert!(HourAction::parse(name, &args.map(str::to_string)).is_err());
        }
    }
}
