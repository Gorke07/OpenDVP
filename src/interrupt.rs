//! Deterministic interrupt events and watchdog accounting. PDF conflicts are
//! explicit profiles; physical devices and VM context save/restore belong to
//! their callers. This is not a claim about an installed simulator scheduler.
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InterruptId {
    External {
        input: u8,
        rising: bool,
    },
    /// Channels0/1: period ms. Channel2: period in100us units.
    Timer {
        channel: u8,
        period: u8,
    },
    Counter {
        slot: u8,
    },
    Communication {
        port: u8,
    },
}
impl InterruptId {
    pub fn number(self) -> u16 {
        match self {
            Self::External { input, rising } => 100 * u16::from(input) + u16::from(rising),
            Self::Timer { channel, period } => 600 + 100 * u16::from(channel) + u16::from(period),
            Self::Counter { slot } => 10 * u16::from(slot),
            Self::Communication { port } => 130 + 10 * u16::from(port),
        }
    }
    pub fn word_len(self) -> u16 {
        1
    }
    pub fn period_us(self) -> Option<u64> {
        if let Self::Timer { channel, period } = self {
            Some(u64::from(period) * if channel == 2 { 100 } else { 1000 })
        } else {
            None
        }
    }
    pub fn validate(self) -> Result<(), String> {
        let valid = match self {
            Self::External { input, .. } => input < 8,
            Self::Timer {
                channel: 0 | 1,
                period,
            } => (2..=99).contains(&period),
            Self::Timer { channel: 2, period } => (5..=99).contains(&period),
            Self::Counter { slot } => (1..=8).contains(&slot),
            Self::Communication { port } => (1..=3).contains(&port),
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err("Kesme etiketi belgelenmiş I aralığında değil".into())
        }
    }
    pub fn disabled(self, flags: [bool; 10], profile: MaskProfile) -> bool {
        let bit = match self {
            Self::External { input, .. } => match profile {
                MaskProfile::ConceptsTable => usize::from(input),
                MaskProfile::InstructionDetail => usize::from(input.min(5)),
            },
            Self::Timer { channel: 0, .. } => 6,
            Self::Timer { .. } => 7,
            Self::Counter { .. } => 9,
            // M1058 is COM3 monitor request, not a communication IRQ mask.
            Self::Communication { .. } => return false,
        };
        flags.get(bit).copied().unwrap_or(true)
    }
}
impl InterruptId {
    /// The documented interrupt whose label is `I<number:03>`, if there is one.
    pub fn from_str_label(number: u16) -> Option<Self> {
        if number > 999 {
            return None;
        }
        format!("I{number:03}")
            .parse::<Self>()
            .ok()
            .filter(|id| id.validate().is_ok())
    }
}
impl fmt::Display for InterruptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "I{:03}", self.number())
    }
}
impl FromStr for InterruptId {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        let raw = text.trim();
        let digits = raw
            .strip_prefix('I')
            .or_else(|| raw.strip_prefix('i'))
            .ok_or("Kesme etiketi I ile başlamalı")?;
        if digits.len() != 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err("Kesme etiketi tam üç onluk basamak içermeli: I001".into());
        }
        let n = digits.parse::<u16>().map_err(|_| "Geçersiz I etiketi")?;
        let id = if n <= 701 && n % 100 <= 1 {
            Self::External {
                input: (n / 100) as u8,
                rising: n % 100 == 1,
            }
        } else if (602..=699).contains(&n) {
            Self::Timer {
                channel: 0,
                period: (n - 600) as u8,
            }
        } else if (702..=799).contains(&n) {
            Self::Timer {
                channel: 1,
                period: (n - 700) as u8,
            }
        } else if (805..=899).contains(&n) {
            Self::Timer {
                channel: 2,
                period: (n - 800) as u8,
            }
        } else if (10..=80).contains(&n) && n.is_multiple_of(10) {
            Self::Counter {
                slot: (n / 10) as u8,
            }
        } else if matches!(n, 140 | 150 | 160) {
            Self::Communication {
                port: ((n - 130) / 10) as u8,
            }
        } else {
            return Err(format!("Belgelenmemiş kesme etiketi: {raw}"));
        };
        id.validate()?;
        Ok(id)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MaskProfile {
    #[default]
    ConceptsTable,
    InstructionDetail,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PendingPolicy {
    #[default]
    Earliest,
    Latest,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReturnEnable {
    /// Explicit local context policy; DI inside ISR doesn't leak into main.
    #[default]
    RestoreEntry,
    /// Use only with separately established installed runner semantics.
    AlwaysEnable,
    /// Alternative qualified policy: retain the ISR's final EI/DI setting.
    PreserveHandler,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptCapabilities {
    pub external_inputs: u8,
    pub communication_ports: u8,
    pub counter_slots: u8,
    pub minimum_timer_ms: u8,
    pub submillisecond_timer: bool,
}
impl Default for InterruptCapabilities {
    fn default() -> Self {
        Self::es2_v2()
    }
}
impl InterruptCapabilities {
    pub const fn es2_v2() -> Self {
        Self {
            external_inputs: 8,
            communication_ports: 3,
            counter_slots: 8,
            minimum_timer_ms: 2,
            submillisecond_timer: true,
        }
    }
    pub const fn ss2_v2() -> Self {
        Self {
            communication_ports: 2,
            ..Self::es2_v2()
        }
    }
    pub const fn detail_5ms() -> Self {
        Self {
            minimum_timer_ms: 5,
            ..Self::es2_v2()
        }
    }
    pub fn allows(self, id: InterruptId) -> bool {
        id.validate().is_ok()
            && match id {
                InterruptId::External { input, .. } => input < self.external_inputs,
                InterruptId::Communication { port } => port <= self.communication_ports,
                InterruptId::Counter { slot } => slot <= self.counter_slots,
                InterruptId::Timer { channel: 2, .. } => self.submillisecond_timer,
                InterruptId::Timer { period, .. } => period >= self.minimum_timer_ms,
            }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingEvent {
    pub at_us: u64,
    pub coalesced: u64,
}
#[derive(Debug, Clone, Copy)]
struct TimerDeadline {
    id: InterruptId,
    next_us: u64,
    period_us: u64,
}

#[derive(Debug, Clone, Default)]
pub struct InterruptState {
    pub enabled: bool,
    pub active: Option<InterruptId>,
    pub mask_profile: MaskProfile,
    pub pending_policy: PendingPolicy,
    pub return_enable: ReturnEnable,
    pub capabilities: InterruptCapabilities,
    /// Explicit virtual CPU cost;0 does not pretend to know physical runtime.
    pub header_instruction_cost_us: u64,
    pub notices: BTreeSet<String>,
    pub now_us: u64,
    definitions: BTreeMap<InterruptId, u16>,
    pending: BTreeMap<InterruptId, PendingEvent>,
    timers: BTreeMap<u8, TimerDeadline>,
    reverse_latched: [bool; 8],
    physical_levels: [Option<bool>; 8],
    image_levels: [Option<bool>; 8],
    entry_enabled: bool,
    configured: bool,
}
impl InterruptState {
    pub fn definitions(&self) -> &BTreeMap<InterruptId, u16> {
        &self.definitions
    }
    pub fn is_configured(&self) -> bool {
        self.configured
    }
    /// Physical counter reset/capture ownership: bit0 rising, bit1 falling.
    /// Registered handlers retain capture while DI/masks defer dispatch. This
    /// explicit local policy prevents a pending ISR from losing its captured
    /// value; the PDF does not specify the DI/mask capture interaction.
    pub fn reset_capture_edges(&self) -> [u8; 8] {
        let mut edges = [0; 8];
        for &id in self.definitions.keys() {
            if let InterruptId::External { input, rising } = id
                && self.capabilities.allows(id)
            {
                let i = usize::from(input);
                let effective_rising = rising ^ self.reverse_latched[i];
                edges[i] |= if effective_rising { 1 } else { 2 };
            }
        }
        edges
    }
    pub fn pending(&self) -> &BTreeMap<InterruptId, PendingEvent> {
        &self.pending
    }
    /// Repeated scans with the SAME linked map retain phase, pending and EI.
    /// A new execution epoch must explicitly clear state even if layout matches.
    pub fn configure(
        &mut self,
        definitions: BTreeMap<InterruptId, u16>,
        now_us: u64,
    ) -> Result<(), String> {
        let mut channels = BTreeSet::new();
        for &id in definitions.keys() {
            if !self.capabilities.allows(id) {
                return Err(format!(
                    "{id}: seçili model/firmware kesme profili bu etiketi desteklemiyor"
                ));
            }
            if let InterruptId::Timer { channel, .. } = id
                && !channels.insert(channel)
            {
                return Err(
                    "Aynı fiziksel timer kesme kanalına birden fazla dönem atanamaz".into(),
                );
            }
        }
        if self.configured && self.definitions == definitions {
            return self.advance_to(now_us);
        }
        if self.active.is_some() {
            return Err("Aktif kesme çerçevesi sırasında yeni program bağlanamaz".into());
        }
        let mut timers = BTreeMap::new();
        for &id in definitions.keys() {
            if let InterruptId::Timer { channel, .. } = id {
                let period_us = id.period_us().expect("timer period");
                let next_us = now_us
                    .checked_add(period_us)
                    .ok_or("Timer kesme zaman çizelgesi taşıyor")?;
                timers.insert(
                    channel,
                    TimerDeadline {
                        id,
                        next_us,
                        period_us,
                    },
                );
            }
        }
        self.clear_execution_epoch();
        self.now_us = now_us;
        self.definitions = definitions;
        self.configured = true;
        self.timers = timers;
        Ok(())
    }
    pub fn clear_execution_epoch(&mut self) {
        self.enabled = false;
        self.active = None;
        self.entry_enabled = false;
        self.definitions.clear();
        self.pending.clear();
        self.timers.clear();
        self.reverse_latched = [false; 8];
        self.physical_levels = [None; 8];
        self.image_levels = [None; 8];
        self.now_us = 0;
        self.configured = false;
    }
    pub fn on_ei(&mut self, reverse: [bool; 3]) {
        self.reverse_latched[0] = reverse[0];
        self.reverse_latched[4] = reverse[1];
        self.reverse_latched[6] = reverse[2];
        self.enabled = true;
    }
    pub fn on_di(&mut self) {
        self.enabled = false;
    }
    pub fn request(&mut self, id: InterruptId, at_us: u64) -> Result<(), String> {
        if !self.definitions.contains_key(&id) {
            return Err(format!("Bağlı kesme yordamı bulunamadı: {id}"));
        }
        self.latch(id, at_us, 1);
        Ok(())
    }
    fn latch(&mut self, id: InterruptId, at_us: u64, occurrences: u64) {
        let entry = self.pending.entry(id).or_insert(PendingEvent {
            at_us,
            coalesced: 0,
        });
        entry.at_us = match self.pending_policy {
            PendingPolicy::Earliest => entry.at_us.min(at_us),
            PendingPolicy::Latest => entry.at_us.max(at_us),
        };
        entry.coalesced = entry.coalesced.saturating_add(occurrences);
    }
    /// Constant-time catch-up per timer; no unbounded expiration loop.
    /// Timer phase starts at configure(), not at EI: explicitly local policy.
    pub fn advance_to(&mut self, now_us: u64) -> Result<(), String> {
        if now_us < self.now_us {
            return Err("Kesme saati geriye ilerleyemez".into());
        }
        let mut due = Vec::new();
        let mut timers = self.timers.clone();
        for timer in timers.values_mut() {
            if now_us >= timer.next_us {
                let occurrences = (now_us - timer.next_us) / timer.period_us + 1;
                let last = timer.next_us + (occurrences - 1) * timer.period_us;
                let at = match self.pending_policy {
                    PendingPolicy::Earliest => timer.next_us,
                    PendingPolicy::Latest => last,
                };
                due.push((timer.id, at, occurrences));
                timer.next_us = last
                    .checked_add(timer.period_us)
                    .ok_or("Kesme timer son tarihi taşıyor")?;
            }
        }
        self.now_us = now_us;
        self.timers = timers;
        for (id, at, n) in due {
            self.latch(id, at, n);
        }
        Ok(())
    }
    pub fn seed_input_levels(&mut self, levels: [bool; 8]) {
        self.physical_levels = levels.map(Some);
        self.image_levels = levels.map(Some);
    }
    /// Caller supplies a genuine physical transition. Software D1020/D1021
    /// image filters do not delay IRQ edges (PDF111 and269).
    pub fn input_edge(
        &mut self,
        input: Device,
        old: bool,
        new: bool,
        at_us: u64,
    ) -> Result<(), String> {
        if input.area != Area::X {
            return Err("Kesme kenarı X girişinden gelmeli".into());
        }
        // General PLC inputs remain usable, but cannot produce this model's
        // external interrupt. Do not turn ordinary X10+ edits into PLC faults.
        if input.index >= u16::from(self.capabilities.external_inputs.min(8)) {
            return Ok(());
        }
        let i = usize::from(input.index);
        self.physical_levels[i] = Some(new);
        if old == new {
            return Ok(());
        }
        let ids:Vec<_>=self.definitions.keys().copied().filter(|id|matches!(id,InterruptId::External{input,rising} if usize::from(*input)==i && (*rising ^ self.reverse_latched[i])==new)).collect();
        for id in ids {
            self.latch(id, at_us, 1);
        }
        Ok(())
    }
    /// Optional direct-Memory-input seam. Call only after reconciling user input
    /// edits with the physical/image owner. It has an independent image baseline
    /// so a previous physical edge is not overwritten by an unchanged stale image.
    pub fn service_inputs(&mut self, m: &Memory, now_us: u64) -> Result<(), String> {
        for i in 0..self.capabilities.external_inputs.min(8) {
            let new = m.bit(Device {
                area: Area::X,
                index: u16::from(i),
            });
            let old = self.image_levels[usize::from(i)].replace(new);
            if let Some(old) = old
                && old != new
            {
                self.input_edge(
                    Device {
                        area: Area::X,
                        index: u16::from(i),
                    },
                    old,
                    new,
                    now_us,
                )?;
            }
        }
        Ok(())
    }
    pub fn dispatch(&mut self, mask_flags: [bool; 10]) -> Option<(InterruptId, u16)> {
        if !self.enabled || self.active.is_some() {
            return None;
        }
        let id = self
            .pending
            .iter()
            .filter(|(id, event)| {
                event.at_us <= self.now_us && !id.disabled(mask_flags, self.mask_profile)
            })
            .min_by_key(|(id, event)| (event.at_us, id.number()))
            .map(|(&id, _)| id)?;
        let target = *self.definitions.get(&id)?;
        self.pending.remove(&id);
        self.entry_enabled = self.enabled;
        self.active = Some(id);
        Some((id, target))
    }
    pub fn finish_handler(&mut self) -> Result<(), String> {
        if self.active.take().is_none() {
            return Err("IRET etkin kesme çerçevesi dışında kullanıldı".into());
        }
        self.enabled = match self.return_enable {
            ReturnEnable::RestoreEntry => self.entry_enabled,
            ReturnEnable::AlwaysEnable => true,
            ReturnEnable::PreserveHandler => self.enabled,
        };
        Ok(())
    }
}
pub fn mask_flags(m: &Memory) -> [bool; 10] {
    std::array::from_fn(|i| m.system_bit(1050 + i as u16))
}
pub fn reverse_flags(m: &Memory) -> [bool; 3] {
    [m.system_bit(1280), m.system_bit(1284), m.system_bit(1286)]
}
pub fn devices(out: &mut BTreeSet<Device>) {
    out.extend(
        (1050..=1059)
            .chain([1008, 1280, 1284, 1286])
            .map(Memory::system_device),
    );
    out.extend(
        [
            1000, 1008, 1010, 1011, 1012, 1167, 1168, 1169, 1240, 1241, 1242, 1243,
        ]
        .map(|index| Device {
            area: Area::D,
            index,
        }),
    );
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ZeroWatchdog {
    #[default]
    Invalid,
    Disabled,
    Immediate,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WatchdogProfile {
    #[default]
    VirtualElapsed,
    /// Caller supplies independently established installed execution cost,
    /// converted to microseconds. Do not feed desktop latency into this input.
    InstalledCostInput,
    Disabled,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchdogError {
    InvalidLimit(i16),
    Timeout {
        pc: u16,
        elapsed_us: u64,
        limit_us: u64,
    },
}
impl fmt::Display for WatchdogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimit(n) => write!(
                f,
                "D1000={n}: belgelenmiş pozitif watchdog aralığı dışında; sıfır davranışı seçilmedi"
            ),
            Self::Timeout {
                pc,
                elapsed_us,
                limit_us,
            } => write!(
                f,
                "Watchdog zaman aşımı: STEP {pc}, {elapsed_us}us > {limit_us}us"
            ),
        }
    }
}
#[derive(Debug, Clone)]
pub struct WatchdogState {
    pub enabled: bool,
    pub profile: WatchdogProfile,
    pub elapsed_us: u64,
    pub zero_policy: ZeroWatchdog,
    pub fault: Option<WatchdogError>,
    pub refreshes: u64,
}
impl Default for WatchdogState {
    fn default() -> Self {
        Self {
            enabled: true,
            profile: WatchdogProfile::VirtualElapsed,
            elapsed_us: 0,
            zero_policy: ZeroWatchdog::Invalid,
            fault: None,
            refreshes: 0,
        }
    }
}
impl WatchdogState {
    pub fn begin_scan(&mut self) {
        if self.fault.is_none() {
            self.elapsed_us = 0;
        }
    }
    /// Only actual execution-time increments belong here. User pause/STOP,
    /// fixed-scan idle waiting and source instruction budgets are separate.
    pub fn advance(&mut self, delta_us: u64, limit_ms: i16, pc: u16) -> Result<(), WatchdogError> {
        if let Some(fault) = &self.fault {
            return Err(fault.clone());
        }
        if !self.enabled || self.profile == WatchdogProfile::Disabled {
            return Ok(());
        }
        if limit_ms < 0 || (limit_ms == 0 && self.zero_policy == ZeroWatchdog::Invalid) {
            return Err(WatchdogError::InvalidLimit(limit_ms));
        }
        if limit_ms == 0 && self.zero_policy == ZeroWatchdog::Disabled {
            return Ok(());
        }
        self.elapsed_us = self.elapsed_us.saturating_add(delta_us);
        let limit_us = limit_ms as u64 * 1000;
        if self.elapsed_us > limit_us {
            let fault = WatchdogError::Timeout {
                pc,
                elapsed_us: self.elapsed_us,
                limit_us,
            };
            self.fault = Some(fault.clone());
            return Err(fault);
        }
        Ok(())
    }
    /// A refresh cannot clear a latched watchdog trip. The caller's WDTP edge
    /// gate supplies power=true only for its actual rising visit.
    pub fn refresh(&mut self, power: bool) {
        if power && self.fault.is_none() {
            self.elapsed_us = 0;
            self.refreshes = self.refreshes.saturating_add(1);
        }
    }
    pub fn reset(&mut self) {
        self.elapsed_us = 0;
        self.fault = None;
        self.refreshes = 0;
    }
}

#[cfg(test)]
#[path = "interrupt_tests.rs"]
mod tests;
