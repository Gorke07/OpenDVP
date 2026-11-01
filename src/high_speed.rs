//! PDF API53..56 deterministic virtual high-speed operations. Installed
//! ES2/SS2 handlers are skip stubs, not a mathematical/timing oracle.
use crate::calendar_operand::CalendarValue;
use crate::counter::CounterModel;
use crate::interrupt::InterruptId;
use crate::plc::{Area, Device, Memory};
use crate::shift::bit_span;
use crate::timing_io::ResourceOwner;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn d(area: Area, index: u16) -> Device {
    Device { area, index }
}
fn limit(m: &mut Memory, s: impl Into<String>) {
    m.execution_limitations.insert(s.into());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighSpeedDest {
    Bit(Device),
    Counter(Device),
    Interrupt(InterruptId),
}
impl std::fmt::Display for HighSpeedDest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bit(v) | Self::Counter(v) => write!(f, "{v}"),
            Self::Interrupt(v) => write!(f, "{v}"),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum HighSpeedAction {
    Compare {
        threshold: CalendarValue,
        counter: Device,
        dest: HighSpeedDest,
        reset: bool,
    },
    Zone {
        lower: CalendarValue,
        upper: CalendarValue,
        counter: Device,
        dest: Device,
    },
    Speed {
        input: Device,
        period: CalendarValue,
        dest: Device,
    },
}
fn threshold(raw: &str) -> Result<CalendarValue, String> {
    CalendarValue::parse(raw, true)
}
fn counter(raw: &str) -> Result<Device, String> {
    let c: Device = raw.parse()?;
    if c.area != Area::C || !(232..=254).contains(&c.index) {
        return Err("Hızlı karşılaştırma sayacı C232..C254 olmalı".into());
    }
    Ok(c)
}
impl HighSpeedAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        match (name, args.len()) {
            ("DHSCS" | "DHSCR", 3) => {
                let reset = name == "DHSCR";
                let dest = if args[2].starts_with('I') {
                    let id: InterruptId = args[2].parse()?;
                    if reset || !matches!(id, InterruptId::Counter { .. }) {
                        return Err("Yalnız DHSCS hedefi I010..I080 olabilir".into());
                    }
                    HighSpeedDest::Interrupt(id)
                } else {
                    let v: Device = args[2].parse()?;
                    if reset && v.area == Area::C {
                        HighSpeedDest::Counter(counter(&args[2])?)
                    } else {
                        if !v.writable_bit() {
                            return Err(
                                "DHSCS/DHSCR hedefi Y/M/S; DHSCR ayrıca hızlı C olabilir".into()
                            );
                        }
                        HighSpeedDest::Bit(v)
                    }
                };
                Ok(Self::Compare {
                    threshold: threshold(&args[0])?,
                    counter: counter(&args[1])?,
                    dest,
                    reset,
                })
            }
            ("DHSZ", 4) => {
                let dest: Device = args[3].parse()?;
                bit_span(dest, 3, true)?;
                Ok(Self::Zone {
                    lower: threshold(&args[0])?,
                    upper: threshold(&args[1])?,
                    counter: counter(&args[2])?,
                    dest,
                })
            }
            ("SPD", 3) => {
                let input: Device = args[0].parse()?;
                if input.area != Area::X || input.index > 7 {
                    return Err("SPD girişi X0..X7 olmalı".into());
                }
                let dest: Device = args[2].parse()?;
                if !matches!(dest.area, Area::D | Area::T | Area::C) {
                    return Err("SPD hedefi beş T/C/D kelimesi olmalı".into());
                }
                let max = if dest.area == Area::D { 9999 } else { 255 };
                if dest.index.checked_add(4).is_none_or(|v| v > max) {
                    return Err("SPD beş kelimelik sonuç aralığı alanı aşıyor".into());
                }
                Ok(Self::Speed {
                    input,
                    period: CalendarValue::parse(&args[1], false)?,
                    dest,
                })
            }
            _ => Err(
                "DHSCS/DHSCR/DHSZ/SPD operandları geçersiz; P veya ek D formu belgelenmedi".into(),
            ),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Compare { reset: false, .. } => "DHSCS",
            Self::Compare { reset: true, .. } => "DHSCR",
            Self::Zone { .. } => "DHSZ",
            Self::Speed { .. } => "SPD",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Compare { reset: false, .. } => 0x7B00,
            Self::Compare { reset: true, .. } => 0x7D00,
            Self::Zone { .. } => 0x7F00,
            Self::Speed { .. } => 0x8000,
        }
    }
    pub fn words(&self) -> u16 {
        match self {
            Self::Compare { .. } => 13,
            Self::Zone { .. } => 17,
            Self::Speed { .. } => 7,
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Compare {
                dest: HighSpeedDest::Bit(v) | HighSpeedDest::Counter(v),
                ..
            }
            | Self::Zone { dest: v, .. }
            | Self::Speed { dest: v, .. } => *v,
            Self::Compare { counter, .. } => *counter,
        }
    }
    pub fn destination(&self) -> Device {
        self.dest()
    }
    pub fn wide(&self) -> bool {
        !matches!(self, Self::Speed { .. })
    }
    pub fn display_value(&self, m: &Memory) -> String {
        match self {
            Self::Speed { dest, .. } => m.double_word(*dest).to_string(),
            Self::Compare {
                dest: HighSpeedDest::Bit(v),
                ..
            } => u8::from(m.bit(*v)).to_string(),
            Self::Zone { dest, .. } => (0..3)
                .map(|i| {
                    if m.bit(d(dest.area, dest.index + i)) {
                        '1'
                    } else {
                        '0'
                    }
                })
                .collect(),
            _ => m.double_word(self.dest()).to_string(),
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Compare {
                threshold,
                counter,
                dest,
                ..
            } => format!("{} {threshold} {counter} {dest}", self.name()),
            Self::Zone {
                lower,
                upper,
                counter,
                dest,
            } => format!("DHSZ {lower} {upper} {counter} {dest}"),
            Self::Speed {
                input,
                period,
                dest,
            } => format!("SPD {input} {period} {dest}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Compare {
                threshold,
                counter,
                dest,
                ..
            } => {
                threshold.devices(out);
                out.insert(*counter);
                if let HighSpeedDest::Bit(v) | HighSpeedDest::Counter(v) = dest {
                    out.insert(*v);
                }
            }
            Self::Zone {
                lower,
                upper,
                counter,
                dest,
            } => {
                lower.devices(out);
                upper.devices(out);
                out.insert(*counter);
                out.extend((0..3).map(|n| d(dest.area, dest.index + n)));
            }
            Self::Speed {
                input,
                period,
                dest,
            } => {
                period.devices(out);
                out.insert(*input);
                if input.index % 2 == 1 {
                    out.insert(d(Area::X, input.index - 1));
                }
                out.extend((0..5).map(|n| d(dest.area, dest.index + n)));
            }
        }
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let at_us = m.timing_io.now_us;
        let active: Vec<_> = m
            .counters
            .active_counters()
            .map(|i| d(Area::C, i))
            .collect();
        let mut state = std::mem::take(&mut m.high_speed);
        state.model = m.counters.model;
        state.visit(m, self, power, pc, at_us, &active);
        m.high_speed = state;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ZoneBoundary {
    #[default]
    InclusiveZone,
    LowerSideInclusive,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ArmComparison {
    #[default]
    EvaluateCurrent,
    TransitionOnly,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CompareChanges {
    #[default]
    CountsOnly,
    AllValueChanges,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HardwareTiming {
    #[default]
    Equality,
    LegacyDirectionalOffset,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpeedWindows {
    #[default]
    ContinuousWhileEnabled,
    OneShotUntilRearm,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpeedDisabled {
    #[default]
    HoldWorking,
    ClearWorking,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CounterChange {
    #[default]
    Count,
    Reset,
    Load,
    ComparatorReset,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CounterResetTarget {
    #[default]
    SameComparedCounter,
    AnyHighSpeedCounter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterTransition {
    pub counter: Device,
    pub before: i32,
    pub after: i32,
    pub at_us: u64,
    pub down: bool,
    pub reason: CounterChange,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompareMode {
    Equal {
        value: i32,
        dest: HighSpeedDest,
        reset: bool,
    },
    Zone {
        lower: i32,
        upper: i32,
        dest: Device,
    },
}
#[derive(Debug, Clone)]
struct Comparator {
    counter: Device,
    mode: CompareMode,
    group: Option<u8>,
    slots: BTreeSet<u8>,
}
#[derive(Debug, Clone)]
struct SpeedRun {
    input: Device,
    dest: Device,
    period_us: u64,
    next_period_us: u64,
    deadline: u64,
    count: i32,
    done: bool,
    pins: BTreeSet<Device>,
}
#[derive(Debug, Clone)]
pub struct HighSpeedState {
    pub model: CounterModel,
    pub zone_boundary: ZoneBoundary,
    pub arm_comparison: ArmComparison,
    pub compare_changes: CompareChanges,
    pub counter_reset_target: CounterResetTarget,
    pub hardware_timing: HardwareTiming,
    /// Modern ES2>=3.20 / SS2>=3.00; older firmware latches until re-enable.
    pub live_hardware_thresholds: bool,
    pub speed_windows: SpeedWindows,
    pub speed_disabled: SpeedDisabled,
    pub notices: BTreeSet<String>,
    pub now_us: u64,
    running: bool,
    comparators: BTreeMap<u16, Comparator>,
    speeds: BTreeMap<u16, SpeedRun>,
    physical: [bool; 8],
    seeded: bool,
    pub trace: VecDeque<CounterTransition>,
}
impl Default for HighSpeedState {
    fn default() -> Self {
        Self {
            model: CounterModel::Es2,
            zone_boundary: ZoneBoundary::InclusiveZone,
            arm_comparison: ArmComparison::EvaluateCurrent,
            compare_changes: CompareChanges::CountsOnly,
            counter_reset_target: CounterResetTarget::SameComparedCounter,
            hardware_timing: HardwareTiming::Equality,
            live_hardware_thresholds: true,
            speed_windows: SpeedWindows::ContinuousWhileEnabled,
            speed_disabled: SpeedDisabled::HoldWorking,
            notices: BTreeSet::new(),
            now_us: 0,
            running: true,
            comparators: BTreeMap::new(),
            speeds: BTreeMap::new(),
            physical: [false; 8],
            seeded: false,
            trace: VecDeque::new(),
        }
    }
}
fn group(c: Device) -> Option<u8> {
    match c.index {
        243 | 245..=248 | 251 | 252 => Some(0),
        244 | 249 | 250 | 253 | 254 => Some(1),
        _ => None,
    }
}
fn valid_counter(model: CounterModel, c: Device) -> bool {
    c.area == Area::C
        && (232..=254).contains(&c.index)
        && !(model == CounterModel::Ss2 && c.index == 232)
}
fn software_irq(c: Device) -> u8 {
    match c.index {
        232 | 235 => 1,
        233 | 239 => 5,
        234 | 241 => 7,
        236 => 2,
        237 => 3,
        238 => 4,
        240 => 6,
        242 => 8,
        _ => 0,
    }
}
fn supported(v: &CalendarValue, m: &Memory, wide: bool) -> Result<i32, String> {
    if wide
        && matches!(
            v,
            CalendarValue::Index {
                high_bank: true,
                ..
            }
        )
    {
        return Err(
            "F kaynağı 32 bit hızlı karşılaştırma matrisinde yok; karşılaştırıcı tutuldu".into(),
        );
    }
    let mut devices = BTreeSet::new();
    v.devices(&mut devices);
    if devices.into_iter().any(|value| !in_model(m, value)) {
        return Err("Hızlı komut operandı seçilen modelin tam bellek aralığını aşıyor".into());
    }
    Ok(v.read(m))
}
fn in_model(m: &Memory, value: Device) -> bool {
    value.index
        <= match value.area {
            Area::D => m.data_register_limit.unwrap_or(9999),
            Area::M => m.relay_limit.unwrap_or(4095),
            Area::S => 1023,
            _ => 255,
        }
}
impl HighSpeedState {
    pub fn comparator_count(&self) -> usize {
        self.comparators.len()
    }
    pub fn speed_count(&self) -> usize {
        self.speeds.len()
    }
    pub fn set_running(m: &mut Memory, running: bool) {
        m.high_speed.running = running;
    }
    pub fn seed_inputs(m: &mut Memory, levels: [bool; 8]) {
        m.high_speed.physical = levels;
        m.high_speed.seeded = true;
    }
    fn resolve(&self, m: &mut Memory, c: Device, active: &[Device]) -> Option<Device> {
        let Some(g) = group(c) else { return Some(c) };
        let candidates: Vec<_> = active
            .iter()
            .copied()
            .filter(|v| group(*v) == Some(g))
            .collect();
        if candidates.len() > 1 {
            limit(
                m,
                "Hızlı karşılaştırma donanım grubunda birden çok aktif kaynak var; kaynak seçimi tutuldu",
            );
            None
        } else {
            Some(candidates.first().copied().unwrap_or(c))
        }
    }
    fn allocate(
        &self,
        m: &mut Memory,
        pc: u16,
        c: Device,
        mode: &CompareMode,
    ) -> Option<BTreeSet<u8>> {
        let g = group(c);
        let zone = matches!(mode, CompareMode::Zone { .. });
        let others: Vec<_> = self
            .comparators
            .iter()
            .filter(|(id, _)| **id != pc)
            .map(|(_, v)| v)
            .collect();
        if g.is_none() {
            if others
                .iter()
                .filter(|v| v.group.is_none() && matches!(v.mode, CompareMode::Zone { .. }) == zone)
                .count()
                >= 6
            {
                limit(
                    m,
                    if zone {
                        "DHSZ yazılım bölge karşılaştırıcı sınırı altıdır"
                    } else {
                        "DHSCS/DHSCR ortak yazılım karşılaştırıcı sınırı altıdır"
                    },
                );
                return None;
            }
            return Some(BTreeSet::new());
        }
        let used: BTreeSet<_> = others
            .iter()
            .filter(|v| v.group == g)
            .flat_map(|v| v.slots.iter().copied())
            .collect();
        let slots = if zone {
            BTreeSet::from([3, 4])
        } else if let CompareMode::Equal {
            dest: HighSpeedDest::Interrupt(InterruptId::Counter { slot }),
            ..
        } = mode
        {
            let local = if g == Some(0) {
                *slot
            } else {
                slot.saturating_sub(4)
            };
            if !(1..=4).contains(&local) || (g == Some(0) && *slot > 4) {
                limit(m, "DHSCS kesme numarası donanım sayaç grubuna ait değil");
                return None;
            }
            BTreeSet::from([local])
        } else {
            let Some(slot) = (1..=4).find(|v| !used.contains(v)) else {
                limit(m, "DHSCS/DHSCR donanım grubunda boş karşılaştırıcı yok");
                return None;
            };
            BTreeSet::from([slot])
        };
        if !slots.is_disjoint(&used) {
            limit(
                m,
                "DHSZ/DHSCS/DHSCR aynı donanım karşılaştırıcısını yeniden kullanıyor",
            );
            None
        } else {
            Some(slots)
        }
    }
    fn visit(
        &mut self,
        m: &mut Memory,
        a: &HighSpeedAction,
        power: bool,
        pc: u16,
        at_us: u64,
        active: &[Device],
    ) {
        if !power {
            self.comparators.remove(&pc);
            if let Some(run) = self.speeds.remove(&pc) {
                if self.speed_disabled == SpeedDisabled::ClearWorking {
                    write_speed(m, run.dest, 2, 0);
                    write_word(m, run.dest, 4, 0);
                }
                self.release_speed(m, pc);
            }
            return;
        }
        if let HighSpeedAction::Speed {
            input,
            period,
            dest,
        } = a
        {
            self.arm_speed(m, pc, *input, period, *dest, at_us);
            return;
        }
        let (c, mut mode) = match a {
            HighSpeedAction::Compare {
                threshold,
                counter,
                dest,
                reset,
            } => {
                let value = match supported(threshold, m, true) {
                    Ok(v) => v,
                    Err(e) => {
                        limit(m, e);
                        return;
                    }
                };
                if let HighSpeedDest::Interrupt(id) = dest {
                    if !m.interrupts.definitions().contains_key(id) {
                        limit(m, "DHSCS hedef kesme yordamı programda kayıtlı değil");
                        return;
                    }
                    if group(*counter).is_none()
                        && *id
                            != (InterruptId::Counter {
                                slot: software_irq(*counter),
                            })
                    {
                        limit(m, "DHSCS yazılım sayacı için yanlış kesme numarası");
                        return;
                    }
                }
                (
                    *counter,
                    CompareMode::Equal {
                        value,
                        dest: *dest,
                        reset: *reset,
                    },
                )
            }
            HighSpeedAction::Zone {
                lower,
                upper,
                counter,
                dest,
            } => {
                let (lo, hi) = match (supported(lower, m, true), supported(upper, m, true)) {
                    (Ok(a), Ok(b)) => (a, b),
                    (Err(e), _) | (_, Err(e)) => {
                        limit(m, e);
                        return;
                    }
                };
                if lo > hi {
                    limit(
                        m,
                        "DHSZ alt sınır üst sınırdan büyük; karşılaştırıcı tutuldu",
                    );
                    return;
                }
                (
                    *counter,
                    CompareMode::Zone {
                        lower: lo,
                        upper: hi,
                        dest: *dest,
                    },
                )
            }
            _ => unreachable!(),
        };
        if !valid_counter(self.model, c) {
            limit(
                m,
                "Hızlı karşılaştırma sayacı seçilen ES2/SS2 modelinde desteklenmiyor",
            );
            return;
        }
        let outputs: Vec<_> = match mode {
            CompareMode::Equal {
                dest: HighSpeedDest::Bit(v),
                ..
            } => vec![v],
            CompareMode::Zone { dest, .. } => {
                (0..3).map(|i| d(dest.area, dest.index + i)).collect()
            }
            _ => vec![],
        };
        if outputs.iter().any(|v| !in_model(m, *v)) {
            limit(
                m,
                "Hızlı karşılaştırmanın tam bit sonucu model belleğini aşıyor; sonuç tutuldu",
            );
            return;
        }
        if let CompareMode::Equal {
            dest: HighSpeedDest::Counter(target),
            ..
        } = mode
        {
            if !valid_counter(self.model, target) {
                limit(m, "DHSCR reset hedefi seçilen modelde hızlı sayaç değil");
                return;
            }
            if target != c && self.counter_reset_target == CounterResetTarget::SameComparedCounter {
                limit(
                    m,
                    "DHSCR sayaç hedefi karşılaştırılan sayaçla aynı olmalı; çapraz sayaç reseti yalnız açık yerel uzantı profilidir",
                );
                return;
            }
        }
        if group(c).is_some()
            && !self.live_hardware_thresholds
            && let Some(previous) = self.comparators.get(&pc)
        {
            mode = previous.mode;
            self.notices.insert("Eski firmware profilinde donanım karşılaştırma ayarı yeniden etkinleştirilene kadar tutulur".into());
        }
        let Some(slots) = self.allocate(m, pc, c, &mode) else {
            return;
        };
        let changed = self
            .comparators
            .get(&pc)
            .is_none_or(|old| old.mode != mode || old.counter != c);
        let cmp = Comparator {
            counter: c,
            mode,
            group: group(c),
            slots,
        };
        self.comparators.insert(pc, cmp.clone());
        if ((changed && self.arm_comparison == ArmComparison::EvaluateCurrent)
            || matches!(cmp.mode, CompareMode::Zone { .. }))
            && let Some(source) = self.resolve(m, c, active)
        {
            let value = m.double_word(source);
            let down =
                source.index != 243 && source.index != 244 && m.system_bit(1000 + source.index);
            self.evaluate(m, &cmp, value, down, at_us);
        }
    }
    fn write_bit(&self, m: &mut Memory, target: Device, value: bool) {
        m.set_internal(target, value);
        if target.area == Area::Y
            && target.index
                <= if self.model == CounterModel::Ss2 {
                    5
                } else {
                    3
                }
        {
            m.set_physical_output(target, value);
        }
    }
    fn evaluate(&mut self, m: &mut Memory, cmp: &Comparator, value: i32, down: bool, at_us: u64) {
        match cmp.mode {
            CompareMode::Equal {
                value: threshold,
                dest,
                reset,
            } => {
                let threshold = if cmp.group.is_some()
                    && self.hardware_timing == HardwareTiming::LegacyDirectionalOffset
                {
                    if down {
                        threshold.wrapping_sub(1)
                    } else {
                        threshold.wrapping_add(1)
                    }
                } else {
                    threshold
                };
                if value != threshold {
                    return;
                }
                match dest {
                    HighSpeedDest::Bit(target) => self.write_bit(m, target, !reset),
                    HighSpeedDest::Counter(target) => {
                        m.set_double_word_raw(target, 0);
                        m.set_bit(target, false);
                    }
                    HighSpeedDest::Interrupt(id) => {
                        if let Err(e) = m.interrupts.request(id, at_us) {
                            limit(m, e);
                        }
                    }
                }
            }
            CompareMode::Zone { lower, upper, dest } => {
                let region = if value < lower
                    || self.zone_boundary == ZoneBoundary::LowerSideInclusive && value == lower
                {
                    0
                } else if value > upper {
                    2
                } else {
                    1
                };
                for n in 0..3 {
                    self.write_bit(m, d(dest.area, dest.index + n), n == region);
                }
            }
        }
    }
    /// Invoke after the counter value/contact commit. The active list must be
    /// captured from the real CounterState before it is temporarily taken out.
    #[allow(clippy::too_many_arguments)] // Event data and ownership cannot be read from temporarily taken backends.
    pub fn counter_changed(
        m: &mut Memory,
        counter: Device,
        before: i32,
        after: i32,
        at_us: u64,
        down: bool,
        reason: CounterChange,
        active: &[Device],
    ) {
        let mut state = std::mem::take(&mut m.high_speed);
        if state.running {
            if state.trace.len() == 256 {
                state.trace.pop_front();
            }
            state.trace.push_back(CounterTransition {
                counter,
                before,
                after,
                at_us,
                down,
                reason,
            });
            let comparisons: Vec<_> = state.comparators.values().cloned().collect();
            for cmp in comparisons {
                if !matches!(cmp.mode, CompareMode::Zone { .. })
                    && reason != CounterChange::Count
                    && state.compare_changes != CompareChanges::AllValueChanges
                {
                    continue;
                }
                if state.resolve(m, cmp.counter, active) == Some(counter) {
                    state.evaluate(m, &cmp, after, down, at_us);
                }
            }
        }
        m.high_speed = state;
    }
    fn release_speed(&self, m: &mut Memory, pc: u16) {
        let owner = ResourceOwner::External(format!("SPD:{pc}"));
        m.timing_io.input_owners.retain(|_, v| v != &owner);
    }
    fn arm_speed(
        &mut self,
        m: &mut Memory,
        pc: u16,
        input: Device,
        period: &CalendarValue,
        dest: Device,
        at_us: u64,
    ) {
        if !speed_span(m, dest) {
            return;
        }
        let period = match supported(period, m, false) {
            Ok(v) if (1..=32767).contains(&v) => v as u64 * 1000,
            Ok(_) => {
                limit(m, "SPD ölçüm süresi 1..32767 ms olmalı; ölçüm tutuldu");
                return;
            }
            Err(e) => {
                limit(m, e);
                return;
            }
        };
        let pins = if input.index.is_multiple_of(2) {
            BTreeSet::from([input])
        } else {
            BTreeSet::from([d(Area::X, input.index - 1), input])
        };
        let owner = ResourceOwner::External(format!("SPD:{pc}"));
        if pins
            .iter()
            .any(|p| m.timing_io.input_owners.get(p).is_some_and(|v| v != &owner))
            || pins.iter().any(|p| {
                m.interrupts.definitions().keys().any(
                    |id| matches!(id,InterruptId::External{input,..} if u16::from(*input)==p.index),
                )
            })
        {
            limit(
                m,
                "SPD girişi hızlı sayaç/darbe komutu/kesme tarafından kullanılıyor; ölçüm başlatılmadı",
            );
            return;
        }
        for &p in &pins {
            m.timing_io.input_owners.insert(p, owner.clone());
        }
        if !self.seeded {
            for (i, v) in self.physical.iter_mut().enumerate() {
                *v = m
                    .timing_io
                    .physical_inputs
                    .get(&d(Area::X, i as u16))
                    .copied()
                    .unwrap_or_else(|| m.bit(d(Area::X, i as u16)));
            }
            self.seeded = true;
        }
        if let Some(run) = self.speeds.get_mut(&pc) {
            run.next_period_us = period;
            return;
        }
        let Some(deadline) = at_us.checked_add(period) else {
            limit(m, "SPD örnek son tarihi u64 saat sınırını aşıyor");
            self.release_speed(m, pc);
            return;
        };
        self.speeds.insert(
            pc,
            SpeedRun {
                input,
                dest,
                period_us: period,
                next_period_us: period,
                deadline,
                count: 0,
                done: false,
                pins,
            },
        );
        write_speed(m, dest, 2, 0);
        write_word(m, dest, 4, (period / 1000) as i16);
        self.notices.insert("SPD: varsayılan sürekli pencere modu PDF zaman diyagramını izler; metindeki tek-atım yorumu ayrı profil seçeneğidir".into());
    }
    /// Must run before a physical event at the same timestamp. Windows are
    /// [start,deadline); an event on the deadline belongs to the next window.
    pub fn advance_to(m: &mut Memory, at_us: u64) {
        let mut state = std::mem::take(&mut m.high_speed);
        if at_us < state.now_us {
            limit(m, "Hızlı ölçüm zamanı geriye gidiyor; ilerleme tutuldu");
            m.high_speed = state;
            return;
        }
        state.now_us = at_us;
        if state.running {
            for run in state.speeds.values_mut() {
                if run.done {
                    continue;
                }
                if at_us >= run.deadline {
                    write_speed(m, run.dest, 0, run.count);
                    run.count = 0;
                    write_speed(m, run.dest, 2, 0);
                    if state.speed_windows == SpeedWindows::OneShotUntilRearm {
                        run.done = true;
                        write_word(m, run.dest, 4, 0);
                        continue;
                    }
                    run.period_us = run.next_period_us;
                    let remaining = at_us - run.deadline;
                    let skipped = remaining / run.period_us;
                    if skipped > 0 {
                        write_speed(m, run.dest, 0, 0);
                    }
                    let next = remaining / run.period_us + 1;
                    let Some(deadline) = next
                        .checked_mul(run.period_us)
                        .and_then(|v| run.deadline.checked_add(v))
                    else {
                        run.done = true;
                        limit(m, "SPD son tarihi taşması; ölçüm durduruldu");
                        continue;
                    };
                    run.deadline = deadline;
                }
                let remaining = run.deadline.saturating_sub(at_us).div_ceil(1000).min(32767);
                write_word(m, run.dest, 4, remaining as i16);
            }
        }
        m.high_speed = state;
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        if self.running {
            self.speeds
                .values()
                .filter(|r| !r.done)
                .map(|r| r.deadline)
                .min()
        } else {
            None
        }
    }
    /// Physical edge only; caller advances deadlines first. Never reads the
    /// temporarily absent TimingState/CounterState from this callback.
    pub fn input_edge(m: &mut Memory, input: Device, old: bool, new: bool, at_us: u64) {
        if input.area != Area::X || input.index > 7 || old == new {
            return;
        }
        let mut state = std::mem::take(&mut m.high_speed);
        if at_us < state.now_us {
            limit(m, "SPD fiziksel olay zamanı geriye gidiyor");
            m.high_speed = state;
            return;
        }
        if state.seeded && state.physical[input.index as usize] != old {
            limit(m, "SPD fiziksel başlangıç seviyesi uyuşmuyor; olay tutuldu");
            m.high_speed = state;
            return;
        }
        let previous = state.physical;
        state.physical[input.index as usize] = new;
        state.seeded = true;
        state.now_us = at_us;
        if state.running {
            for run in state.speeds.values_mut() {
                if run.done || !run.pins.contains(&input) {
                    continue;
                }
                let delta = if run.input.index % 2 == 0 {
                    if new { 1 } else { 0 }
                } else {
                    let a = (run.input.index - 1) as usize;
                    let b = run.input.index as usize;
                    let before = (u8::from(previous[a]) << 1) | u8::from(previous[b]);
                    let after = (u8::from(state.physical[a]) << 1) | u8::from(state.physical[b]);
                    match (before, after) {
                        (0, 2) | (2, 3) | (3, 1) | (1, 0) => 1,
                        (2, 0) | (3, 2) | (1, 3) | (0, 1) => -1,
                        _ => 0,
                    }
                };
                run.count = run.count.wrapping_add(delta);
                write_speed(m, run.dest, 2, run.count);
            }
        }
        m.high_speed = state;
    }
}
fn speed_span(m: &mut Memory, base: Device) -> bool {
    if base.area == Area::D && base.index + 4 > m.data_register_limit.unwrap_or(9999) {
        limit(m, "SPD beş kelimelik sonucu modelin D sınırını aşıyor");
        return false;
    }
    if base.area == Area::C && base.index + 4 >= 200 {
        limit(
            m,
            "SPD C199/32 bit doğal sayaç sınırındaki beş kelime eşlemesi doğrulanmadı; sonuç tutuldu",
        );
        return false;
    }
    true
}
fn write_word(m: &mut Memory, base: Device, offset: u16, value: i16) {
    m.set_word(d(base.area, base.index + offset), value);
}
fn write_speed(m: &mut Memory, base: Device, offset: u16, value: i32) {
    write_word(m, base, offset, value as i16);
    write_word(m, base, offset + 1, (value >> 16) as i16);
}

#[cfg(test)]
#[path = "high_speed_tests.rs"]
mod tests;
