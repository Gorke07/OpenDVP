//! DCNT. DocumentedEvents is an explicit ES2/SS2 virtual input backend;
//! InstalledLegacy follows the shipped interpreter's distinct C<235 coil path.
use crate::plc::{Area, Device, Memory};
use crate::timing_io::ResourceOwner;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn dev(area: Area, index: u16) -> Device {
    Device { area, index }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CounterModel {
    #[default]
    Es2,
    Ss2,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CounterProfile {
    #[default]
    DocumentedEvents,
    InstalledLegacy,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HardwareResetPolicy {
    /// PDF C243/244 ON-level default. Other external resets use selected edge.
    #[default]
    DefaultLevel,
    /// Explicit alternative for the PDF's R/F reset-edge description.
    SelectedEdge,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterPreset {
    Constant(i32),
    Data(Device),
}
impl CounterPreset {
    pub fn read(self, m: &Memory) -> i32 {
        match self {
            Self::Constant(v) => v,
            Self::Data(d) => m.double_word(d),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterAction {
    pub counter: Device,
    pub preset: CounterPreset,
}
impl CounterAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if name != "DCNT" || args.len() != 2 {
            return Err("DCNT C200..C254 K<signed32>/D<pair> bekleniyor".into());
        }
        let counter: Device = args[0].parse()?;
        if counter.area != Area::C || !(200..=254).contains(&counter.index) {
            return Err("DCNT hedefi C200..C254 olmalı".into());
        }
        let raw = &args[1];
        let preset = if let Some(v) = raw.strip_prefix('K') {
            CounterPreset::Constant(
                v.parse::<i32>()
                    .map_err(|_| "DCNT K değeri signed32 aralığında olmalı")?,
            )
        } else {
            let d: Device = raw.parse()?;
            if d.area != Area::D || d.index >= 9999 {
                return Err("DCNT ayarı tamamlanmış bir D çifti olmalı".into());
            }
            CounterPreset::Data(d)
        };
        Ok(Self { counter, preset })
    }
    pub fn name(&self) -> &'static str {
        "DCNT"
    }
    pub fn header(&self) -> u16 {
        0x0600
    }
    pub fn dest(&self) -> Device {
        self.counter
    }
    pub fn destination(&self) -> Device {
        self.counter
    }
    pub fn label(&self) -> String {
        match self.preset {
            CounterPreset::Constant(v) => format!("DCNT {} K{v}", self.counter),
            CounterPreset::Data(d) => format!("DCNT {} {d}", self.counter),
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        m.double_word(self.counter).to_string()
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.insert(self.counter);
        if let CounterPreset::Data(d) = self.preset {
            out.insert(d);
            out.insert(dev(Area::D, d.index + 1));
        }
        out.insert(dev(Area::M, 1000 + self.counter.index));
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if m.counters.profile == CounterProfile::DocumentedEvents
            && let Some(kind) = kind(m.counters.model, self.counter.index)
        {
            for pin in kind.pins(m, self.counter.index) {
                out.insert(dev(Area::X, u16::from(pin)));
            }
            out.extend([dev(Area::D, 1022), dev(Area::M, 1260)]);
            for i in 1270..=1277 {
                out.insert(dev(Area::M, i));
            }
            if let Some(r) = kind.reset() {
                let base = if r == 1 || r == 4 { 1240 } else { 1242 };
                out.extend([dev(Area::D, base), dev(Area::D, base + 1)]);
            }
        }
    }
    pub fn validate_preset_span(&self, m: &Memory) -> Result<(), String> {
        if let CounterPreset::Data(d) = self.preset
            && (d.area != Area::D
                || u32::from(d.index) + 1 > u32::from(m.data_register_limit.unwrap_or(9999)))
        {
            return Err("DCNT ayar D çifti model belleğinin dışında; sayaç tutuldu".into());
        }
        Ok(())
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let mut state = std::mem::take(&mut m.counters);
        state.visit(m, self, power, pc);
        m.counters = state;
    }
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    Single(u8),
    Up(u8, Option<u8>),
    Dir(u8, u8, Option<u8>),
    Dual(u8, u8, Option<u8>),
    Ab(u8, u8, Option<u8>),
}
impl Kind {
    fn reset(self) -> Option<u8> {
        match self {
            Self::Up(_, r) | Self::Dir(_, _, r) | Self::Dual(_, _, r) | Self::Ab(_, _, r) => r,
            Self::Single(_) => None,
        }
    }
    fn pins(self, m: &Memory, c: u16) -> BTreeSet<u8> {
        let mut s = BTreeSet::new();
        match self {
            Self::Single(a) => {
                s.insert(a);
                if c <= 241 && m.system_bit(1260) {
                    s.insert(7);
                }
            }
            Self::Up(a, _) => {
                s.insert(a);
            }
            Self::Dir(a, b, _) | Self::Dual(a, b, _) | Self::Ab(a, b, _) => {
                s.extend([a, b]);
            }
        }
        if let Some(r) = self.reset()
            && !(c == 243 && m.system_bit(1243) || c == 244 && m.system_bit(1244))
        {
            s.insert(r);
        }
        s
    }
}
fn kind(model: CounterModel, c: u16) -> Option<Kind> {
    Some(match c {
        232 if model == CounterModel::Es2 => Kind::Ab(0, 2, None),
        233 => Kind::Ab(4, 5, None),
        234 => Kind::Ab(6, 7, None),
        235..=242 => Kind::Single((c - 235) as u8),
        243 => Kind::Up(0, Some(1)),
        244 => Kind::Up(2, Some(3)),
        245 => Kind::Dir(0, 1, None),
        246 => Kind::Dir(0, 1, Some(4)),
        247 => Kind::Dual(0, 1, None),
        248 => Kind::Dual(0, 1, Some(4)),
        249 => Kind::Dir(2, 3, None),
        250 => Kind::Dir(2, 3, Some(5)),
        251 => Kind::Ab(0, 1, None),
        252 => Kind::Ab(0, 1, Some(4)),
        253 => Kind::Ab(2, 3, None),
        254 => Kind::Ab(2, 3, Some(5)),
        _ => return None,
    })
}
#[derive(Debug, Clone)]
struct Active {
    pc: u16,
    preset: i32,
    kind: Kind,
    pins: BTreeSet<u8>,
    reset_disabled: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterEvent {
    pub at_us: u64,
    pub counter: u16,
    pub before: i32,
    pub after: i32,
    pub reset: bool,
}
#[derive(Debug, Clone)]
pub struct CounterState {
    pub model: CounterModel,
    pub profile: CounterProfile,
    pub hardware_reset: HardwareResetPolicy,
    /// Virtual backend assumes modern firmware (ES2/SS2 >=2.80 for K1).
    pub single_frequency_supported: bool,
    running: bool,
    physical: [bool; 8],
    seeded: bool,
    last_event_us: Option<u64>,
    edges: BTreeMap<u16, bool>,
    active: BTreeMap<u16, Active>,
    // Bare RST owns this state; ordinary value resets (including ZRST) do not.
    // General software counters share the native-derived instruction policy.
    // Documented physical event counters do not consume this bank.
    reset_inhibit: BTreeSet<u16>,
    installed_enabled: BTreeSet<u16>,
    /// External IRQ ownership explicitly injected by the IRQ integration.
    interrupt_edges: BTreeMap<u8, u8>,
    direction_monitors: BTreeMap<u16, bool>,
    blocked: BTreeSet<u16>,
    pub trace: VecDeque<CounterEvent>,
}
impl Default for CounterState {
    fn default() -> Self {
        Self {
            model: CounterModel::Es2,
            profile: CounterProfile::DocumentedEvents,
            hardware_reset: HardwareResetPolicy::DefaultLevel,
            single_frequency_supported: true,
            running: true,
            physical: [false; 8],
            seeded: false,
            last_event_us: None,
            edges: BTreeMap::new(),
            active: BTreeMap::new(),
            reset_inhibit: BTreeSet::new(),
            installed_enabled: BTreeSet::new(),
            interrupt_edges: BTreeMap::new(),
            direction_monitors: BTreeMap::new(),
            blocked: BTreeSet::new(),
            trace: VecDeque::new(),
        }
    }
}
impl CounterState {
    pub fn begin_scan(m: &mut Memory) {
        Self::service(m);
    }
    /// The installed interpreter's enable bank is separate from C contacts
    /// and from physical input ownership. It does not imply hardware counting.
    pub fn installed_coil_enabled(&self, counter: u16) -> bool {
        self.installed_enabled.contains(&counter)
    }
    pub fn visit_reset(m: &mut Memory, counter: Device, power: bool) {
        if counter.area != Area::C || !(200..=254).contains(&counter.index) {
            return;
        }
        if power {
            m.counters.reset_inhibit.insert(counter.index);
            m.reset(counter);
        } else {
            m.counters.reset_inhibit.remove(&counter.index);
        }
    }
    /// No synthetic edges. Call once with physical levels, before enabling.
    pub fn seed_inputs(m: &mut Memory, levels: [bool; 8]) {
        m.counters.physical = levels;
        m.counters.seeded = true;
    }
    pub fn set_running(m: &mut Memory, running: bool) {
        m.counters.running = running;
    }
    /// Unconfigured standalone-memory seam; compiled IRQ maps take precedence.
    /// `falling=None` releases IRQ priority; Some selects its actual edge.
    pub fn set_interrupt_reset(m: &mut Memory, input: u8, falling: Option<bool>) {
        Self::set_interrupt_reset_edges(
            m,
            input,
            falling.map(|f| if f { 2 } else { 1 }).unwrap_or(0),
        );
    }
    /// bit0 rising, bit1 falling. This does not register/dispatch an ISR.
    pub fn set_interrupt_reset_edges(m: &mut Memory, input: u8, edges: u8) {
        if input < 8 {
            if edges & 3 == 0 {
                m.counters.interrupt_edges.remove(&input);
            } else {
                m.counters.interrupt_edges.insert(input, edges & 3);
            }
        }
    }
    fn synchronize_interrupt_resets(&mut self, m: &Memory) {
        if m.interrupts.is_configured() {
            self.interrupt_edges.clear();
            for (input, edges) in m.interrupts.reset_capture_edges().into_iter().enumerate() {
                if edges & 3 != 0 {
                    self.interrupt_edges.insert(input as u8, edges & 3);
                }
            }
        }
    }
    pub fn note_reset(m: &mut Memory, counter: Device) {
        if counter.area == Area::C && (200..=254).contains(&counter.index) {
            let before = m.double_word(counter);
            m.set_double_word_raw(counter, 0);
            m.set_bit(counter, false);
            let active = m
                .counters
                .active_counters()
                .map(|index| dev(Area::C, index))
                .collect::<Vec<_>>();
            crate::high_speed::HighSpeedState::counter_changed(
                m,
                counter,
                before,
                0,
                m.timing_io.now_us,
                false,
                crate::high_speed::CounterChange::Reset,
                &active,
            );
        }
    }
    pub fn active_counters(&self) -> impl Iterator<Item = u16> + '_ {
        self.active.keys().copied()
    }
    fn limitation(m: &mut Memory, text: impl Into<String>) {
        m.execution_limitations.insert(text.into());
    }
    fn visit(&mut self, m: &mut Memory, a: &CounterAction, power: bool, pc: u16) {
        let c = a.counter.index;
        if self.profile == CounterProfile::InstalledLegacy && c >= 235 {
            // The native branch does not read preset/history, claim physical
            // inputs or run the DocumentedEvents scheduler.
            if power {
                self.installed_enabled.insert(c);
            } else {
                self.installed_enabled.remove(&c);
            }
            return;
        }
        let general =
            self.profile == CounterProfile::InstalledLegacy || kind(self.model, c).is_none();
        if !power {
            if general {
                self.edges.insert(pc.wrapping_add(5), false);
            }
            self.installed_enabled.remove(&c);
            if self.active.get(&c).is_some_and(|a| a.pc == pc) {
                self.active.remove(&c);
            }
            self.sync_claims(m);
            return;
        }
        if let Err(reason) = a.validate_preset_span(m) {
            Self::limitation(m, reason);
            return;
        }
        let preset = a.preset.read(m);
        if self.profile == CounterProfile::InstalledLegacy {
            self.installed_enabled.insert(c);
        }
        if general {
            let was = self.edges.insert(pc.wrapping_add(5), true).unwrap_or(false);
            if !was && !self.reset_inhibit.contains(&c) {
                let down = m.system_bit(1000 + c);
                let at_us = m.timing_io.now_us;
                self.count(m, c, preset, down, at_us);
            }
            return;
        }
        let Some(k) = kind(self.model, c) else {
            return;
        };
        // M1243/44 must be set before activation. Keep their captured value
        // until a false visit disarms this instance.
        let disabled = self
            .active
            .get(&c)
            .map(|a| a.reset_disabled)
            .unwrap_or(c == 243 && m.system_bit(1243) || c == 244 && m.system_bit(1244));
        let mut pins = k.pins(m, c);
        if let Some(r) = k.reset() {
            if disabled {
                pins.remove(&r);
            } else {
                pins.insert(r);
            }
        }
        if pins.iter().any(|&p| {
            m.timing_io
                .input_owners
                .get(&dev(Area::X, u16::from(p)))
                .is_some_and(|owner| owner != &ResourceOwner::External("DCNT".into()))
        }) {
            Self::limitation(
                m,
                format!(
                    "DCNT C{c}: fiziksel giriş başka bir darbe komutu tarafından kullanılıyor; etkinleştirme tutuldu"
                ),
            );
            return;
        }
        for (&other, active) in &self.active {
            let shared_reset = |pin: &u8| {
                *pin == 7
                    && c <= 241
                    && other <= 241
                    && c >= 235
                    && other >= 235
                    && m.system_bit(1260)
            };
            if (other == c && active.pc != pc)
                || (other != c && pins.intersection(&active.pins).any(|p| !shared_reset(p)))
            {
                Self::limitation(
                    m,
                    format!(
                        "DCNT C{c}: C{other} ile hızlı sayaç/giriş sahipliği çakışıyor; yeni etkinleştirme tutuldu"
                    ),
                );
                return;
            }
        }
        let software = matches!(k, Kind::Single(_) | Kind::Ab(_, _, None)) && c < 243;
        if software
            && !self.active.contains_key(&c)
            && self.active.keys().filter(|&&v| v < 243).count() >= 8
        {
            Self::limitation(
                m,
                "DCNT: aynı anda en fazla sekiz yazılım hızlı sayacı kullanılabilir",
            );
            return;
        }
        if matches!(k, Kind::Ab(..))
            && m.word(dev(Area::D, 1022)) == 1
            && !self.single_frequency_supported
        {
            Self::limitation(
                m,
                "DCNT K1 AB modu bu seçilmiş firmware profilinde desteklenmiyor",
            );
            return;
        }
        if !self.seeded {
            for (i, v) in self.physical.iter_mut().enumerate() {
                *v = m.bit(dev(Area::X, i as u16));
            }
            self.seeded = true;
        }
        self.active.insert(
            c,
            Active {
                pc,
                preset,
                kind: k,
                pins,
                reset_disabled: disabled,
            },
        );
        self.sync_claims(m);
        if self.profile == CounterProfile::DocumentedEvents {
            self.service_inner(m);
        }
    }
    fn sync_claims(&self, m: &mut Memory) {
        m.timing_io
            .input_owners
            .retain(|_, owner| owner != &ResourceOwner::External("DCNT".into()));
        for (&c, active) in &self.active {
            if self.blocked.contains(&c) {
                continue;
            }
            for &p in &active.pins {
                m.timing_io
                    .input_owners
                    .entry(dev(Area::X, u16::from(p)))
                    .or_insert_with(|| ResourceOwner::External("DCNT".into()));
            }
        }
    }
    fn event_pins(&self, m: &Memory, c: u16, a: &Active) -> BTreeSet<u8> {
        let mut pins = a.pins.clone();
        if (235..=241).contains(&c) {
            if m.system_bit(1260) {
                pins.insert(7);
            } else {
                pins.remove(&7);
            }
        }
        pins
    }
    fn conflicted(&self, m: &mut Memory, c: u16, a: &Active) -> bool {
        let pins = self.event_pins(m, c, a);
        for (&other, b) in &self.active {
            if other == c {
                continue;
            }
            let shared_reset = |p: &u8| {
                *p == 7
                    && (235..=241).contains(&c)
                    && (235..=241).contains(&other)
                    && m.system_bit(1260)
            };
            if pins
                .intersection(&self.event_pins(m, other, b))
                .any(|p| !shared_reset(p))
            {
                Self::limitation(
                    m,
                    format!(
                        "DCNT C{c}/C{other}: çalışma içi giriş ayarı çakışıyor; fiziksel sayım tutuldu"
                    ),
                );
                return true;
            }
        }
        false
    }
    fn record(&mut self, e: CounterEvent) {
        if self.trace.len() == 256 {
            self.trace.pop_front();
        }
        self.trace.push_back(e);
    }
    fn clear(&mut self, m: &mut Memory, c: u16, at_us: u64) {
        let d = dev(Area::C, c);
        let before = m.double_word(d);
        m.set_double_word_raw(d, 0);
        m.set_bit(d, false);
        self.record(CounterEvent {
            at_us,
            counter: c,
            before,
            after: 0,
            reset: true,
        });
        let active = self
            .active_counters()
            .map(|index| dev(Area::C, index))
            .collect::<Vec<_>>();
        crate::high_speed::HighSpeedState::counter_changed(
            m,
            d,
            before,
            0,
            at_us,
            false,
            crate::high_speed::CounterChange::Reset,
            &active,
        );
    }
    fn count(&mut self, m: &mut Memory, c: u16, preset: i32, down: bool, at_us: u64) {
        let d = dev(Area::C, c);
        let before = m.double_word(d);
        if (c == 243 || c == 244) && before < 0 {
            Self::limitation(
                m,
                "DCNT C243/C244 negatif yüklenmiş değerin davranışı doğrulanmadı; olay tutuldu",
            );
            return;
        }
        let after = if c == 243 || c == 244 {
            if before == i32::MAX {
                0
            } else {
                before.wrapping_add(1)
            }
        } else if down {
            before.wrapping_sub(1)
        } else {
            before.wrapping_add(1)
        };
        m.set_double_word_raw(d, after);
        if self.profile == CounterProfile::InstalledLegacy {
            if !down && after == preset {
                m.set_bit(d, true);
            } else if down && after == preset.wrapping_sub(1) {
                m.set_bit(d, false);
            }
        } else {
            m.set_bit(d, after >= preset);
        }
        if self.profile == CounterProfile::DocumentedEvents
            && (c == 232 && self.model == CounterModel::Es2
                || c == 233
                || c == 234
                || (245..=254).contains(&c))
        {
            self.direction_monitors.insert(c, down);
            m.set_system(1000 + c, down);
        }
        self.record(CounterEvent {
            at_us,
            counter: c,
            before,
            after,
            reset: false,
        });
        let active = self
            .active_counters()
            .map(|index| dev(Area::C, index))
            .collect::<Vec<_>>();
        crate::high_speed::HighSpeedState::counter_changed(
            m,
            d,
            before,
            after,
            at_us,
            down,
            crate::high_speed::CounterChange::Count,
            &active,
        );
    }
    pub fn service(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.counters);
        s.service_inner(m);
        m.counters = s;
    }
    fn service_inner(&mut self, m: &mut Memory) {
        self.synchronize_interrupt_resets(m);
        if !self.running || self.profile != CounterProfile::DocumentedEvents {
            return;
        }
        self.blocked.clear();
        let refreshed: Vec<_> = self
            .active
            .iter()
            .map(|(&c, a)| (c, self.event_pins(m, c, a)))
            .collect();
        for (c, pins) in refreshed {
            if pins.iter().any(|&p| {
                m.timing_io
                    .input_owners
                    .get(&dev(Area::X, u16::from(p)))
                    .is_some_and(|o| o != &ResourceOwner::External("DCNT".into()))
            }) {
                self.blocked.insert(c);
                Self::limitation(
                    m,
                    format!(
                        "DCNT C{c}: değişen giriş ayarı başka darbe komutuyla çakışıyor; sayım tutuldu"
                    ),
                );
            }
            if let Some(a) = self.active.get_mut(&c) {
                a.pins = pins;
            }
        }
        self.sync_claims(m);
        for c in 232..=254 {
            if c == 232 && self.model == CounterModel::Es2
                || c == 233
                || c == 234
                || (245..=254).contains(&c)
            {
                m.set_system(
                    1000 + c,
                    self.direction_monitors.get(&c).copied().unwrap_or(false),
                );
            }
        }
        let resets: Vec<u16> = self
            .active
            .iter()
            .filter_map(|(&c, a)| {
                if self.blocked.contains(&c) {
                    return None;
                }
                let level = (c == 243 || c == 244)
                    && self.hardware_reset == HardwareResetPolicy::DefaultLevel
                    && !a.reset_disabled
                    && a.kind.reset().is_some_and(|r| {
                        self.physical[r as usize] && !self.interrupt_edges.contains_key(&r)
                    });
                level.then_some(c)
            })
            .collect();
        for c in resets {
            self.clear(m, c, self.last_event_us.unwrap_or(0));
        }
    }
    /// Process physical X0..X7 changes in timestamp order. Ordinary X image is
    /// latched separately by TimingState/REF. Paired events at the same time
    /// follow caller order; no fabricated simultaneous quadrature transition.
    pub fn input_edge(m: &mut Memory, input: Device, old: bool, new: bool, at_us: u64) {
        if input.area != Area::X || input.index > 7 || old == new {
            return;
        }
        let mut s = std::mem::take(&mut m.counters);
        s.edge(m, input.index as u8, old, new, at_us);
        m.counters = s;
    }
    fn edge(&mut self, m: &mut Memory, pin: u8, old: bool, new: bool, at_us: u64) {
        self.synchronize_interrupt_resets(m);
        if self.last_event_us.is_some_and(|last| at_us < last) {
            Self::limitation(m, "DCNT fiziksel olay zamanı geriye gidiyor; olay tutuldu");
            return;
        }
        if self.seeded && self.physical[pin as usize] != old {
            Self::limitation(
                m,
                "DCNT fiziksel giriş başlangıç seviyesi olayla uyuşmuyor; olay tutuldu",
            );
            return;
        }
        self.seeded = true;
        let previous = self.physical;
        self.physical[pin as usize] = new;
        self.last_event_us = Some(at_us);
        if !self.running || self.profile != CounterProfile::DocumentedEvents {
            return;
        }
        let active: Vec<_> = self.active.iter().map(|(&c, a)| (c, a.clone())).collect();
        for (c, a) in active {
            if self.blocked.contains(&c) || self.conflicted(m, c, &a) {
                continue;
            }
            let reset = a.kind.reset().filter(|_| !a.reset_disabled);
            let global_reset = (235..=241).contains(&c) && m.system_bit(1260) && pin == 7;
            if reset == Some(pin) || global_reset {
                let irq = self.interrupt_edges.get(&pin).copied();
                let falling = m.system_bit(1270 + u16::from(pin));
                let qualified = if global_reset {
                    !old && new
                } else if let Some(mask) = irq {
                    (mask & (if new { 1 } else { 2 })) != 0
                } else if (c == 243 || c == 244)
                    && self.hardware_reset == HardwareResetPolicy::DefaultLevel
                {
                    new
                } else {
                    if falling { old && !new } else { !old && new }
                };
                if qualified {
                    if self.interrupt_edges.contains_key(&pin) && !global_reset {
                        let base = if pin == 1 || pin == 4 { 1240 } else { 1242 };
                        m.set_double_word(dev(Area::D, base), m.double_word(dev(Area::C, c)));
                    }
                    self.clear(m, c, at_us);
                }
                continue;
            }
            if (c == 243 || c == 244)
                && self.hardware_reset == HardwareResetPolicy::DefaultLevel
                && !a.reset_disabled
                && reset.is_some_and(|r| {
                    self.physical[r as usize] && !self.interrupt_edges.contains_key(&r)
                })
            {
                continue;
            }
            let down = match a.kind {
                Kind::Single(p) if pin == p => {
                    let fall = m.system_bit(1270 + u16::from(p));
                    if new == fall {
                        continue;
                    }
                    m.system_bit(1000 + c)
                }
                Kind::Up(p, _) if pin == p && new => false,
                Kind::Dir(p, d, _) if pin == p && new => self.physical[d as usize],
                Kind::Dual(up, down, _) if new && (pin == up || pin == down) => pin == down,
                Kind::Ab(a, b, _) if pin == a || pin == b => {
                    let mode = m.word(dev(Area::D, 1022));
                    if mode == 1 && (!self.single_frequency_supported || pin != b || !new) {
                        continue;
                    }
                    if mode == 2 && pin != b {
                        continue;
                    }
                    let prior =
                        (u8::from(previous[a as usize]) << 1) | u8::from(previous[b as usize]);
                    let next = (u8::from(self.physical[a as usize]) << 1)
                        | u8::from(self.physical[b as usize]);
                    match (prior, next) {
                        (0, 2) | (2, 3) | (3, 1) | (1, 0) => false,
                        (2, 0) | (3, 2) | (1, 3) | (0, 1) => true,
                        _ => continue,
                    }
                }
                _ => continue,
            };
            self.count(m, c, a.preset, down, at_us);
        }
    }
}

#[cfg(test)]
#[path = "counter_tests.rs"]
mod tests;
