//! Explicit virtual timing and edge backend. Installed timing handlers are
//! stubs: this module does not claim native or physical timing equivalence.
use crate::plc::{Area, Device, Memory, Operand, word_range};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const MAX_EVENTS: usize = 4096;
const MAX_INSTANCES: usize = 512;
const MAX_TRACE: usize = 256;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DelayProfile {
    Legacy100Us,
    #[default]
    ModernFine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceOwner {
    External(String),
    Csfo(usize),
    Gpwm(usize),
}

#[derive(Debug, Clone)]
struct InputEdge {
    input: Device,
    high: bool,
}

#[derive(Debug, Clone)]
struct PwmRun {
    cycle: Device,
    dest: Device,
    start_us: u64,
}

#[derive(Debug, Clone)]
struct CsfoRun {
    input: Device,
    sample: Device,
    output: Device,
    active: bool,
    sample_count: u16,
    proportion: u16,
    captured_pulses: i32,
    output_pulses: i32,
    last_sample_us: Option<u64>,
    window_edges: u16,
    window_signed_pulses: i32,
    input_hz: i16,
    output_hz: i32,
}

/// A deterministic pulse generator, represented analytically between input
/// events. Phase ticks are millionths of a period; complete periods count as
/// pulses. 50% duty and the initial phase are named local-profile choices.
#[derive(Debug, Clone)]
pub struct PulseOutput {
    pub owner: usize,
    pub pair: u8,
    pub frequency_hz: i32,
    pub phase_ticks: u64,
    pub accumulated_pulses: i32,
    pub directional: bool,
}

#[derive(Debug, Clone)]
pub struct PulseSegment {
    pub start_us: u64,
    pub end_us: u64,
    pub pair: u8,
    pub frequency_hz: i32,
    pub completed_pulses: u64,
}

#[derive(Debug, Clone, Default)]
pub struct TimingState {
    pub now_us: u64,
    pub delay_profile: DelayProfile,
    /// Informational local-profile qualifications, separate from a blocked or
    /// undefined execution in Memory.execution_limitations.
    pub notices: BTreeSet<String>,
    /// Physical virtual-backend levels. Ordinary X process-image reads latch
    /// these at scan start or an explicit refresh, independently of capture.
    pub physical_inputs: BTreeMap<Device, bool>,
    pub input_owners: BTreeMap<Device, ResourceOwner>,
    pub output_owners: BTreeMap<Device, ResourceOwner>,
    pub pulse_outputs: BTreeMap<u8, PulseOutput>,
    pub pulse_trace: VecDeque<PulseSegment>,
    scan_start_us: u64,
    ordered_plc_timers: bool,
    armed_plc_timers: BTreeSet<Device>,
    event_sequence: u64,
    input_events: BTreeMap<(u64, u64), InputEdge>,
    pwm: BTreeMap<usize, PwmRun>,
    csfo: BTreeMap<usize, CsfoRun>,
}

fn x(index: u16) -> Device {
    Device {
        area: Area::X,
        index,
    }
}
fn y(index: u16) -> Device {
    Device {
        area: Area::Y,
        index,
    }
}
fn offset(base: Device, n: u16) -> Device {
    Device {
        area: base.area,
        index: base.index + n,
    }
}
fn limit(m: &mut Memory, text: &str) {
    m.execution_limitations.insert(text.to_string());
}

impl TimingState {
    /// Stop only executing virtual generators/capture engines after a source
    /// abort. Do not erase the input-event queue, filter/clock time, feedback
    /// registers, timer state or reservations owned by an external backend.
    /// Current Y levels are held here; the watchdog timeout layer clears them.
    pub(crate) fn abort_output_engines(&mut self) {
        self.pwm.clear();
        self.csfo.clear();
        self.pulse_outputs.clear();
        self.input_owners
            .retain(|_, owner| matches!(owner, ResourceOwner::External(_)));
        self.output_owners
            .retain(|_, owner| matches!(owner, ResourceOwner::External(_)));
    }
    pub(crate) fn ordered_plc_timers(&self) -> bool {
        self.ordered_plc_timers
    }
    /// DELAY programs use an explicitly virtual instruction-time timer profile.
    /// Programs without DELAY retain the installed scan-end compatibility path.
    pub(crate) fn configure_plc_timers(&mut self, ordered: bool) {
        if !ordered || ordered != self.ordered_plc_timers {
            self.armed_plc_timers.clear();
        }
        self.ordered_plc_timers = ordered;
    }

    pub(crate) fn arm_plc_timer(&mut self, device: Device, armed: bool) {
        if armed && self.ordered_plc_timers {
            self.armed_plc_timers.insert(device);
        } else {
            self.armed_plc_timers.remove(&device);
        }
    }

    pub fn enqueue_input_edge(
        &mut self,
        at_us: u64,
        input: Device,
        high: bool,
    ) -> Result<(), String> {
        if input.area != Area::X || input.index > 255 {
            return Err("Sanal giriş olayı X0..X377 adresine ait olmalı".into());
        }
        if at_us < self.now_us {
            return Err("Sanal giriş olayı geçmiş bir zamana eklenemez".into());
        }
        if self.input_events.len() >= MAX_EVENTS {
            return Err("Sanal giriş olay kuyruğu dolu (4096 olay)".into());
        }
        let sequence = self.event_sequence;
        self.event_sequence = self
            .event_sequence
            .checked_add(1)
            .ok_or("Sanal olay sıra sayacı doldu")?;
        // Cold physical inputs start low. Register the pre-event level now,
        // before any future image refresh, so the first rising edge is real.
        self.physical_inputs.entry(input).or_insert(false);
        self.input_events
            .insert((at_us, sequence), InputEdge { input, high });
        Ok(())
    }
    pub fn pending_input_events(&self) -> usize {
        self.input_events.len()
    }

    /// Reserve both points of a hardware input/output pair for a future
    /// DCNT/pulse backend. A single-phase CSFO itself owns only its used points.
    pub fn reserve_input_pair(&mut self, pair: u8, owner: &str) -> Result<(), String> {
        reserve_pair(&mut self.input_owners, Area::X, pair, owner)
    }
    pub fn reserve_output_pair(&mut self, pair: u8, owner: &str) -> Result<(), String> {
        reserve_pair(&mut self.output_owners, Area::Y, pair, owner)
    }
    pub fn release_external(&mut self, owner: &str) {
        self.input_owners
            .retain(|_, value| !matches!(value, ResourceOwner::External(name) if name == owner));
        self.output_owners
            .retain(|_, value| !matches!(value, ResourceOwner::External(name) if name == owner));
    }

    fn filter_bypass_inputs(&self, m: &Memory) -> BTreeSet<Device> {
        let mut inputs=self.input_owners.iter().filter_map(|(d,owner)| {
            (matches!(owner,ResourceOwner::Csfo(_))
             || matches!(owner,ResourceOwner::External(name) if name=="DCNT" || name.starts_with("SPD:")))
                .then_some(*d)
        }).collect::<BTreeSet<_>>();
        for id in m.interrupts.definitions().keys() {
            if let crate::interrupt::InterruptId::External { input, .. } = id {
                inputs.insert(x(u16::from(*input)));
            }
        }
        inputs
    }

    fn service_virtual_backends(&mut self, m: &mut Memory, now_us: u64) {
        crate::serial_io::SerialBus::advance(m, now_us);
        crate::dmv_comm::DmvState::advance(m, now_us);
        crate::ethernet::EthernetState::advance(m, now_us);
        crate::ethernet_raw::RawEthernetState::advance(m, now_us);
        crate::ethernet_config::EthernetConfigState::advance(m, now_us);
        crate::canopen::CanopenNetwork::advance(m, now_us);
        crate::canopen_exchange::CanExchangeState::advance(m, now_us);
        crate::coordinated_motion::CoordinatedState::advance(m, now_us);
        crate::pu_module::PuState::advance(m, now_us);
        crate::high_speed::HighSpeedState::advance_to(m, now_us);
        crate::pulse_output::PulseState::advance(m, now_us);
        crate::coordinated_motion::CoordinatedState::service_controls(m);
        crate::motion_extended::MotionExtendedState::advance(m, now_us);
        crate::motion_basic::MotionBasicState::advance(m, now_us, &self.physical_inputs);
        // TimingState is taken out of Memory during event servicing. Reconcile
        // claims against this actual state, preserving every other resource owner.
        self.output_owners.retain(|_, owner| {
            !matches!(owner, ResourceOwner::External(name) if name.starts_with("PULSE:"))
        });
        for (device, owner) in m.pulse_output.claimed_outputs() {
            self.output_owners.entry(device).or_insert(owner);
        }
        self.input_owners.retain(|_, owner| {
            !matches!(owner, ResourceOwner::External(name) if name.starts_with("PULSE:"))
        });
        for (device, owner) in m.pulse_output.claimed_inputs() {
            self.input_owners.entry(device).or_insert(owner);
        }
        self.output_owners.retain(|_, owner| {
            !matches!(owner, ResourceOwner::External(name) if name.starts_with("MOTION:"))
        });
        for (device, owner) in m.motion_basic.claimed_outputs() {
            self.output_owners.entry(device).or_insert(owner);
        }
        self.input_owners.retain(|_, owner| {
            !matches!(owner, ResourceOwner::External(name) if name.starts_with("MOTION:"))
        });
        for (device, owner) in m.motion_basic.claimed_inputs() {
            self.input_owners.entry(device).or_insert(owner);
        }
        crate::external_io::ExternalIoState::advance(m, now_us, &self.physical_inputs);
        self.output_owners.retain(|_, owner| {
            !matches!(owner, ResourceOwner::External(name) if name.starts_with("EXTIO:"))
        });
        for (device, owner) in m.external_io.claimed_outputs() {
            self.output_owners.entry(device).or_insert(owner);
        }
        self.input_owners.retain(|_, owner| {
            !matches!(owner, ResourceOwner::External(name) if name.starts_with("EXTIO:"))
        });
        for (device, owner) in m.external_io.claimed_inputs() {
            self.input_owners.entry(device).or_insert(owner);
        }
        let owned = self.output_owners.keys().copied().collect();
        crate::refresh_io::RefreshState::advance(m, now_us, &self.physical_inputs, &owned);
    }

    fn advance_to(&mut self, m: &mut Memory, target_us: u64) {
        if target_us < self.now_us {
            limit(m, "Sanal zaman geriye ilerletilemez.");
            return;
        }
        let mut boundaries = 0u32;
        loop {
            let next_input = self.input_events.first_key_value().map(|(key, _)| key.0);
            let next = self.now_us.max(
                next_input
                    .into_iter()
                    .chain(m.refresh_io.next_deadline_us())
                    .chain(m.high_speed.next_deadline_us())
                    .chain(m.pulse_output.next_deadline_us())
                    .chain(m.motion_basic.next_deadline_us())
                    .chain(m.motion_extended.next_deadline_us())
                    .chain(m.external_io.next_deadline_us())
                    .chain(m.serial_io.next_deadline_us())
                    .chain(m.dmv_comm.next_deadline_us())
                    .chain(m.ethernet.next_deadline_us())
                    .chain(m.ethernet_raw.next_deadline_us())
                    .chain(m.ethernet_config.next_deadline_us())
                    .chain(m.canopen.next_deadline_us())
                    .chain(m.can_exchange.next_deadline_us())
                    .chain(m.coordinated_motion.next_deadline_us())
                    .chain(m.pu_module.next_deadline_us())
                    .filter(|&t| t <= target_us)
                    .min()
                    .unwrap_or(target_us),
            );
            self.advance_outputs(m, next);
            self.now_us = next;
            // Exact deadline events precede physical edges at the same time.
            // A pulse on an SPD deadline starts the following sample window.
            self.service_virtual_backends(m, next);
            let bypass = self.filter_bypass_inputs(m);
            while let Some((&key, _)) = self.input_events.first_key_value() {
                if key.0 != next {
                    break;
                }
                let event = self.input_events.remove(&key).expect("queued edge");
                let previous = self
                    .physical_inputs
                    .insert(event.input, event.high)
                    .unwrap_or(false);
                m.refresh_io.raw_input_edge(
                    event.input,
                    event.high,
                    key.0,
                    bypass.contains(&event.input),
                );
                if !m.motion_basic.suppresses_external_irq(event.input)
                    && let Err(error) =
                        m.interrupts
                            .input_edge(event.input, previous, event.high, key.0)
                {
                    m.execution_limitations.insert(error);
                }
                crate::counter::CounterState::input_edge(
                    m,
                    event.input,
                    previous,
                    event.high,
                    key.0,
                );
                crate::high_speed::HighSpeedState::input_edge(
                    m,
                    event.input,
                    previous,
                    event.high,
                    key.0,
                );
                crate::pulse_output::PulseState::input_edge(
                    m,
                    event.input,
                    previous,
                    event.high,
                    key.0,
                );
                crate::external_io::ExternalIoState::input_edge(
                    m,
                    event.input,
                    previous,
                    event.high,
                    key.0,
                );
                crate::motion_basic::MotionBasicState::input_edge(
                    m,
                    event.input,
                    previous,
                    event.high,
                    key.0,
                );
                crate::motion_extended::MotionExtendedState::input_edge(
                    m,
                    event.input,
                    previous,
                    event.high,
                    key.0,
                );
                if !previous && event.high {
                    self.capture_rising_edge(m, event.input);
                }
            }
            self.service_virtual_backends(m, next);
            if next == target_us {
                break;
            }
            boundaries += 1;
            if boundaries >= 65536 {
                limit(
                    m,
                    "Sanal olay ilerleme bütçesi doldu; kalan zaman atlanmadan yürütme tutuldu.",
                );
                break;
            }
        }
    }

    fn advance_outputs(&mut self, m: &mut Memory, target_us: u64) {
        let dt = target_us - self.now_us;
        if dt == 0 {
            return;
        }
        crate::system_relays::ClockState::advance(m, dt);
        crate::power_monitor::PowerState::advance(m, dt);
        if m.rtc_clock.profile() == Some(crate::rtc_clock::RtcProfile::VirtualGregorian)
            && let Err(reason) = m.rtc_clock.advance_monotonic_us(dt)
        {
            m.execution_limitations.insert(reason);
        }
        // Global prescaler phase is retained across event segments and scans.
        // Only the current-value WORD advances here; TMR evaluates its done BIT.
        for &device in &self.armed_plc_timers {
            if !m.special_timers.timer_runs(m, &device) {
                continue;
            }
            let base = crate::special_timers::timer_base_us(m, device.index);
            let ticks = target_us / base - self.now_us / base;
            m.set_word(device, m.word(device).wrapping_add(ticks as i16));
        }
        for output in self.pulse_outputs.values_mut() {
            let total = u128::from(output.phase_ticks)
                + u128::from(output.frequency_hz.unsigned_abs()) * u128::from(dt);
            let completed = (total / 1_000_000) as u64;
            output.phase_ticks = (total % 1_000_000) as u64;
            let signed = if output.frequency_hz < 0 {
                (completed as i32).wrapping_neg()
            } else {
                completed as i32
            };
            output.accumulated_pulses = output.accumulated_pulses.wrapping_add(signed);
            if let Some(run) = self.csfo.get_mut(&output.owner) {
                run.output_pulses = output.accumulated_pulses;
            }
            m.set_internal(
                y(u16::from(output.pair) * 2),
                output.frequency_hz != 0 && output.phase_ticks < 500_000,
            );
            m.set_physical_output(
                y(u16::from(output.pair) * 2),
                output.frequency_hz != 0 && output.phase_ticks < 500_000,
            );
            if output.directional && output.frequency_hz != 0 {
                m.set_internal(y(u16::from(output.pair) * 2 + 1), output.frequency_hz < 0);
                m.set_physical_output(y(u16::from(output.pair) * 2 + 1), output.frequency_hz < 0);
            }
            if self.pulse_trace.len() == MAX_TRACE {
                self.pulse_trace.pop_front();
            }
            self.pulse_trace.push_back(PulseSegment {
                start_us: self.now_us,
                end_us: target_us,
                pair: output.pair,
                frequency_hz: output.frequency_hz,
                completed_pulses: completed,
            });
        }
    }

    fn capture_rising_edge(&mut self, m: &mut Memory, input: Device) {
        let mut updates = Vec::new();
        for (&id, run) in &mut self.csfo {
            if !run.active || input.index != (run.input.index & !1) {
                continue;
            }
            let directional = run.input.index & 1 != 0;
            let phase_b = self
                .physical_inputs
                .get(&x(input.index + 1))
                .copied()
                .unwrap_or_else(|| m.bit(x(input.index + 1)));
            let sign: i32 = if directional && phase_b { -1 } else { 1 };
            run.captured_pulses = run.captured_pulses.wrapping_add(sign);
            let Some(previous_us) = run.last_sample_us else {
                run.last_sample_us = Some(self.now_us);
                continue;
            };
            run.window_edges += 1;
            run.window_signed_pulses += sign;
            if run.window_edges < run.sample_count {
                continue;
            }
            let dt = self.now_us - previous_us;
            if dt == 0 {
                limit(
                    m,
                    "CSFO: Aynı zaman damgasındaki darbelerden frekans hesaplanamadı; önceki hız korundu.",
                );
            } else {
                let hz = i128::from(run.window_signed_pulses) * 1_000_000 / i128::from(dt);
                let maximum = if directional { 2000 } else { 10000 };
                if hz.abs() > maximum {
                    limit(
                        m,
                        "CSFO: Sanal giriş frekansı belgelenen kanal sınırını aşıyor; önceki hız korundu.",
                    );
                } else {
                    run.input_hz = hz as i16;
                    let output_hz = i32::from(run.input_hz) * i32::from(run.proportion) / 100;
                    if output_hz.unsigned_abs() > 100_000 {
                        limit(
                            m,
                            "CSFO: Hesaplanan çıkış 100 kHz sınırını aşıyor; fiziksel taşma davranışı bilinmediğinden önceki çıkış korundu.",
                        );
                    } else {
                        run.output_hz = output_hz;
                        updates.push((id, run.input.index as u8 / 2, output_hz));
                    }
                }
            }
            run.last_sample_us = Some(self.now_us);
            run.window_edges = 0;
            run.window_signed_pulses = 0;
        }
        for (owner, pair, frequency) in updates {
            self.set_output_rate(m, owner, pair, frequency);
        }
    }

    fn set_output_rate(&mut self, m: &mut Memory, owner: usize, pair: u8, frequency_hz: i32) {
        if let Some(output) = self.pulse_outputs.get_mut(&pair)
            && output.owner == owner
        {
            output.frequency_hz = frequency_hz;
            m.set_internal(
                y(u16::from(pair) * 2),
                frequency_hz != 0 && output.phase_ticks < 500_000,
            );
            m.set_physical_output(
                y(u16::from(pair) * 2),
                frequency_hz != 0 && output.phase_ticks < 500_000,
            );
            if output.directional && frequency_hz != 0 {
                m.set_internal(y(u16::from(pair) * 2 + 1), frequency_hz < 0);
                m.set_physical_output(y(u16::from(pair) * 2 + 1), frequency_hz < 0);
            }
        }
    }
}

fn reserve_pair(
    map: &mut BTreeMap<Device, ResourceOwner>,
    area: Area,
    pair: u8,
    owner: &str,
) -> Result<(), String> {
    if pair > 1 || owner.is_empty() || owner.len() > 128 {
        return Err("Geçersiz sanal kanal/aygıt sahibi".into());
    }
    let expected = ResourceOwner::External(owner.to_string());
    let points = [
        Device {
            area,
            index: u16::from(pair) * 2,
        },
        Device {
            area,
            index: u16::from(pair) * 2 + 1,
        },
    ];
    if points
        .iter()
        .any(|point| map.get(point).is_some_and(|value| value != &expected))
    {
        return Err("Sanal yüksek hızlı kanal başka bir işlem tarafından kullanılıyor".into());
    }
    for point in points {
        map.insert(point, expected.clone());
    }
    Ok(())
}

/// Flush events at the current instruction time. No host clock is consulted.
pub fn begin_scan(m: &mut Memory) {
    let mut state = std::mem::take(&mut m.timing_io);
    state.scan_start_us = state.now_us;
    state.advance_to(m, state.now_us);
    let bypass = state.filter_bypass_inputs(m);
    crate::refresh_io::RefreshState::begin_scan(m, state.now_us, &state.physical_inputs, &bypass);
    m.timing_io = state;
}

/// Explicit process-image refresh boundary for a future REF input operation.
/// Advancing physical time alone never changes ordinary scan-latched X reads.
pub fn refresh_inputs(m: &mut Memory) {
    let mut state = std::mem::take(&mut m.timing_io);
    state.advance_to(m, state.now_us);
    let bypass = state.filter_bypass_inputs(m);
    crate::refresh_io::RefreshState::begin_scan(m, state.now_us, &state.physical_inputs, &bypass);
    m.timing_io = state;
}

pub fn advance_by(m: &mut Memory, delta_us: u64) {
    let mut state = std::mem::take(&mut m.timing_io);
    let target = state.now_us.checked_add(delta_us).unwrap_or_else(|| {
        limit(m, "Sanal mikro saniye saati üst sınırına ulaştı.");
        u64::MAX
    });
    state.advance_to(m, target);
    m.timing_io = state;
}

/// Return the complete elapsed scan duration, including instruction DELAYs.
/// Derive ms/timer boundaries from cumulative now_us, retaining sub-ms carry.
pub fn finish_scan(m: &mut Memory, base_scan_ms: u64) -> u64 {
    let start = m.timing_io.scan_start_us;
    let processing = base_scan_ms.saturating_mul(1000);
    let elapsed_body = m.timing_io.now_us - start;
    let mut remainder = processing;
    if m.system_relays.profile() == crate::system_relays::ClockProfile::Documented
        && m.system_bit(1039)
    {
        let period = m.word(Device {
            area: Area::D,
            index: 1039,
        });
        if period < 0 {
            limit(m, "M1039 sanal sabit tarama için D1039 negatif olamaz.");
        } else {
            // The configured period is a minimum scan duration. Instruction
            // DELAY and base processing already spent are never counted twice.
            let period_us = u64::from(period as u16) * 1000;
            remainder = remainder.max(period_us.saturating_sub(elapsed_body));
        }
    }
    advance_by(m, remainder);
    m.timing_io.now_us - start
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimingAction {
    Delay {
        duration: Operand,
    },
    Pwm {
        width: Device,
        cycle: Device,
        dest: Device,
    },
    CatchSpeed {
        input: Device,
        sample: Device,
        output: Device,
    },
}

fn data(raw: &str, words: u16) -> Result<Device, String> {
    let d: Device = raw.parse()?;
    if d.area != Area::D {
        return Err("Parametre D register adresi olmalı".into());
    }
    word_range(d, words)?;
    Ok(d)
}

impl TimingAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        match name {
            "DELAY" if args.len() == 1 => {
                let duration = Operand::parse(&args[0])?;
                if matches!(&duration, Operand::Register(d) if d.area != Area::D) {
                    return Err("DELAY süresi K/H sabiti veya D register olmalı".into());
                }
                Ok(Self::Delay { duration })
            }
            "GPWM" if args.len() == 3 => {
                let dest: Device = args[2].parse()?;
                if !dest.writable_bit() {
                    return Err("GPWM hedefi Y/M/S biti olmalı".into());
                }
                Ok(Self::Pwm {
                    width: data(&args[0], 1)?,
                    cycle: data(&args[1], 3)?,
                    dest,
                })
            }
            "CSFO" if args.len() == 3 => {
                let input: Device = args[0].parse()?;
                if input.area != Area::X || input.index > 3 {
                    return Err("CSFO girişi X0..X3 olmalı".into());
                }
                Ok(Self::CatchSpeed {
                    input,
                    sample: data(&args[1], 4)?,
                    output: data(&args[2], 3)?,
                })
            }
            _ => Err(format!(
                "{name}: geçersiz zamanlama komutu veya operand sayısı"
            )),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Delay { .. } => "DELAY",
            Self::Pwm { .. } => "GPWM",
            Self::CatchSpeed { .. } => "CSFO",
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Delay { .. } => Memory::system_device(1148),
            Self::Pwm { dest, .. } => *dest,
            Self::CatchSpeed { output, .. } => *output,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Delay { duration } => format!("DELAY {duration}"),
            Self::Pwm { width, cycle, dest } => format!("GPWM {width} {cycle} {dest}"),
            Self::CatchSpeed {
                input,
                sample,
                output,
            } => format!("CSFO {input} {sample} {output}"),
        }
    }
    pub fn local_word_len(&self) -> u16 {
        if matches!(self, Self::Delay { .. }) {
            3
        } else {
            7
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
        match self {
            Self::Delay { duration } => {
                out.insert(Memory::system_device(1148));
                if let Operand::Register(d) = duration {
                    out.insert(*d);
                }
            }
            Self::Pwm { width, cycle, dest } => {
                out.insert(*width);
                out.insert(*dest);
                out.extend(word_range(*cycle, 3).expect("validated PWM block"));
            }
            Self::CatchSpeed {
                input,
                sample,
                output,
            } => {
                let a = input.index & !1;
                out.insert(x(a));
                out.insert(y(a));
                if input.index & 1 != 0 {
                    out.insert(x(a + 1));
                    out.insert(y(a + 1));
                }
                out.extend(word_range(*sample, 4).expect("validated CSFO input block"));
                out.extend(word_range(*output, 3).expect("validated CSFO output block"));
                for index in [1031, 1330, 1336, 1337] {
                    out.insert(Device {
                        area: Area::D,
                        index,
                    });
                }
            }
        }
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }

    /// Pass the stable source-action instance, including on false power. Pulse
    /// gating is applied by the caller only for DELAYP; GPWM/CSFO have no P form.
    pub fn execute(&self, m: &mut Memory, instance: usize, power: bool) {
        let mut state = std::mem::take(&mut m.timing_io);
        match self {
            Self::Delay { duration } if power => {
                let n = duration.read(m);
                if !(1..=1000).contains(&n) {
                    limit(
                        m,
                        "DELAY: 1..1000 dışındaki sürenin aygıt davranışı doğrulanmadı; gecikme uygulanmadı.",
                    );
                } else {
                    let fine =
                        state.delay_profile == DelayProfile::ModernFine && m.system_bit(1148);
                    let unit = if fine { 5 } else { 100 };
                    if state.delay_profile == DelayProfile::ModernFine {
                        m.set_system(1148, false);
                    }
                    let target = state
                        .now_us
                        .checked_add(n as u64 * unit)
                        .unwrap_or_else(|| {
                            limit(m, "DELAY: Sanal mikro saniye saati üst sınırına ulaştı.");
                            u64::MAX
                        });
                    state.advance_to(m, target);
                }
            }
            Self::Pwm { width, cycle, dest } => {
                pwm(&mut state, m, instance, power, *width, *cycle, *dest)
            }
            Self::CatchSpeed {
                input,
                sample,
                output,
            } => csfo(&mut state, m, instance, power, *input, *sample, *output),
            _ => {}
        }
        m.timing_io = state;
    }
}

fn pwm(
    state: &mut TimingState,
    m: &mut Memory,
    id: usize,
    power: bool,
    width: Device,
    cycle: Device,
    dest: Device,
) {
    if !power {
        state.pwm.remove(&id);
        state
            .output_owners
            .retain(|_, owner| owner != &ResourceOwner::Gpwm(id));
        if !state.output_owners.contains_key(&dest) {
            m.set_internal(dest, false);
            m.set_physical_output(dest, false);
        }
        return;
    }
    let limit_index = m.data_register_limit.unwrap_or(9999);
    if width.index > limit_index || cycle.index + 2 > limit_index {
        limit(
            m,
            "GPWM: Parametre bloğu modelin D sınırını aşıyor; çıkış korunuyor.",
        );
        return;
    }
    let (width_ms, cycle_ms) = (m.word(width), m.word(cycle));
    if cycle_ms <= 0 {
        limit(
            m,
            "GPWM: Sıfır/negatif çevrim davranışı doğrulanmadı; çıkış korunuyor.",
        );
        return;
    }
    if state.pwm.len() >= MAX_INSTANCES && !state.pwm.contains_key(&id) {
        limit(m, "GPWM: Sanal komut durumu sınırına ulaşıldı.");
        return;
    }
    let block = cycle.index..=cycle.index + 2;
    if state.pwm.iter().any(|(other, run)| {
        *other != id
            && (run.dest == dest
                || (run.cycle.index..=run.cycle.index + 2).any(|i| block.contains(&i)))
    }) {
        limit(
            m,
            "GPWM: Ortak hedef/ayrılmış parametre bloğu kullanan komutların eşzamanlı davranışı doğrulanmadı.",
        );
        return;
    }
    if state
        .output_owners
        .get(&dest)
        .is_some_and(|owner| owner != &ResourceOwner::Gpwm(id))
    {
        limit(
            m,
            "GPWM: Çıkış başka bir sanal darbe işlemi tarafından kullanılıyor.",
        );
        return;
    }
    state.output_owners.insert(dest, ResourceOwner::Gpwm(id));
    let run = state.pwm.entry(id).or_insert(PwmRun {
        cycle,
        dest,
        start_us: state.now_us,
    });
    let phase = (state.now_us - run.start_us) % (cycle_ms as u64 * 1000);
    let high = width_ms > 0 && (width_ms >= cycle_ms || phase < width_ms as u64 * 1000);
    m.set_internal(dest, high);
    m.set_physical_output(dest, high);
    state.notices.insert("GPWM: Sanal tarama profili kullanılıyor; ilk faz ve S2+1/S2+2 aygıt içi durum biçimi doğrulanmadı, ayrılmış registerlar değiştirilmedi.".into());
}

fn csfo(
    state: &mut TimingState,
    m: &mut Memory,
    id: usize,
    power: bool,
    input: Device,
    sample: Device,
    output: Device,
) {
    let pair = input.index as u8 / 2;
    let a = input.index & !1;
    let directional = input.index & 1 != 0;
    if !power {
        if let Some(run) = state.csfo.get_mut(&id)
            && run.active
        {
            run.active = false;
            run.output_hz = 0;
            run.last_sample_us = None;
            run.window_edges = 0;
            run.window_signed_pulses = 0;
            m.set_double_word(offset(run.output, 1), 0);
            m.set_internal(y(a), false);
            m.set_physical_output(y(a), false);
        }
        if state
            .pulse_outputs
            .get(&pair)
            .is_some_and(|signal| signal.owner == id)
        {
            state.pulse_outputs.remove(&pair);
        }
        state
            .input_owners
            .retain(|_, owner| owner != &ResourceOwner::Csfo(id));
        state
            .output_owners
            .retain(|_, owner| owner != &ResourceOwner::Csfo(id));
        return;
    }
    let data_limit = m.data_register_limit.unwrap_or(9999);
    if sample.index + 3 > data_limit || output.index + 2 > data_limit {
        limit(
            m,
            "CSFO: Parametre bloğu modelin D sınırını aşıyor; komut başlatılmadı.",
        );
        return;
    }
    let wanted = ResourceOwner::Csfo(id);
    let input_points = if directional {
        vec![x(a), x(a + 1)]
    } else {
        vec![x(a)]
    };
    let output_points = if directional {
        vec![y(a), y(a + 1)]
    } else {
        vec![y(a)]
    };
    if input_points.iter().any(|d| {
        state
            .input_owners
            .get(d)
            .is_some_and(|owner| owner != &wanted)
    }) || output_points.iter().any(|d| {
        state
            .output_owners
            .get(d)
            .is_some_and(|owner| owner != &wanted)
    }) {
        limit(
            m,
            "CSFO: Giriş/çıkış kanalı başka bir sanal sayaç veya darbe komutu tarafından kullanılıyor; CSFO etkinleştirilmedi.",
        );
        return;
    }
    if state.csfo.len() >= MAX_INSTANCES && !state.csfo.contains_key(&id) {
        limit(m, "CSFO: Sanal komut durumu sınırına ulaşıldı.");
        return;
    }
    // Snapshot both writable settings before latching any potentially aliased
    // read-only feedback words. Clamps affect effective settings only.
    let sample_count = m.word(sample).clamp(if directional { 2 } else { 1 }, 100) as u16;
    let proportion = m.word(output).clamp(1, 10000) as u16;
    for point in input_points {
        state.input_owners.insert(point, wanted.clone());
    }
    for point in output_points {
        state.output_owners.insert(point, wanted.clone());
    }
    let run = state.csfo.entry(id).or_insert(CsfoRun {
        input,
        sample,
        output,
        active: false,
        sample_count,
        proportion,
        captured_pulses: 0,
        output_pulses: 0,
        last_sample_us: None,
        window_edges: 0,
        window_signed_pulses: 0,
        input_hz: 0,
        output_hz: 0,
    });
    if !run.active {
        run.active = true;
        run.last_sample_us = None;
        run.window_edges = 0;
        run.window_signed_pulses = 0;
    }
    run.sample_count = sample_count;
    run.proportion = proportion;
    let scaled = i32::from(run.input_hz) * i32::from(proportion) / 100;
    if scaled.unsigned_abs() <= 100_000 {
        // Re-enabling waits for a new sampled period rather than emitting
        // pulses from stale speed; live ratio edits apply after a sample exists.
        if run.last_sample_us.is_some() {
            run.output_hz = scaled;
        }
    } else {
        limit(
            m,
            "CSFO: Hesaplanan çıkış 100 kHz sınırını aşıyor; önceki çıkış korundu.",
        );
    }
    m.set_word(offset(run.sample, 1), run.input_hz);
    m.set_double_word(offset(run.sample, 2), run.captured_pulses);
    m.set_double_word(offset(run.output, 1), run.output_hz);
    state.pulse_outputs.entry(pair).or_insert(PulseOutput {
        owner: id,
        pair,
        frequency_hz: run.output_hz,
        phase_ticks: 0,
        accumulated_pulses: run.output_pulses,
        directional,
    });
    let frequency = run.output_hz;
    state.set_output_rate(m, id, pair, frequency);
    state.notices.insert("CSFO: Sanal kenar profili kullanılıyor (A yükselişi, B=0 ileri, %50 görev); ilk örnek, yeniden etkinleşme ve fiziksel zamanlama doğrulanmadı.".into());
    state.notices.insert("CSFO: D1031/D1330/D1336/D1337 aygıt içi eşlemesi ve hareketsiz girişin sıfırlama süresi doğrulanmadı; bu sistem registerları değiştirilmedi.".into());
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(raw: &str) -> Device {
        raw.parse().unwrap()
    }
    fn action(name: &str, args: &[&str]) -> TimingAction {
        TimingAction::parse(
            name,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn pulse(m: &mut Memory, t: u64, input: Device) {
        m.timing_io.enqueue_input_edge(t, input, true).unwrap();
        m.timing_io.enqueue_input_edge(t + 1, input, false).unwrap();
    }
    fn ready_csfo(input: &str, count: i16, ratio: i16) -> (Memory, TimingAction) {
        let mut m = Memory::default();
        m.set_word(d("D0"), count);
        m.set_word(d("D10"), ratio);
        let a = action("CSFO", &[input, "D0", "D10"]);
        a.execute(&mut m, 7, true);
        (m, a)
    }
    #[test]
    fn delay_consumes_fine_mode_once_and_orders_input_events() {
        let mut m = Memory::default();
        m.set_system(1148, true);
        m.timing_io.enqueue_input_edge(500, x(4), true).unwrap();
        begin_scan(&mut m);
        action("DELAY", &["K100"]).execute(&mut m, 0, true);
        assert_eq!(m.timing_io.now_us, 500);
        assert!(m.timing_io.physical_inputs[&x(4)]);
        assert!(!m.bit(x(4))); // Ordinary process image remains scan-latched.
        refresh_inputs(&mut m);
        assert!(m.bit(x(4)));
        assert!(!m.system_bit(1148));
        action("DELAY", &["K1"]).execute(&mut m, 1, true);
        assert_eq!(m.timing_io.now_us, 600);
        assert_eq!(finish_scan(&mut m, 1), 1600);
        assert_eq!(m.timing_io.now_us, 1600);
    }
    #[test]
    fn delay_false_invalid_legacy_and_microsecond_carry() {
        let mut m = Memory::default();
        m.set_system(1148, true);
        let a = action("DELAY", &["K1"]);
        a.execute(&mut m, 0, false);
        assert_eq!(m.timing_io.now_us, 0);
        assert!(m.system_bit(1148));
        action("DELAY", &["K0"]).execute(&mut m, 0, true);
        assert_eq!(m.timing_io.now_us, 0);
        assert!(!m.system_bit(1067));
        assert!(m.system_bit(1148));
        for _ in 0..200 {
            m.set_system(1148, true);
            a.execute(&mut m, 0, true);
        }
        assert_eq!(m.timing_io.now_us, 1000);
        m.timing_io.delay_profile = DelayProfile::Legacy100Us;
        m.set_system(1148, true);
        a.execute(&mut m, 0, true);
        assert_eq!(m.timing_io.now_us, 1100);
        assert!(m.system_bit(1148));
    }
    #[test]
    fn gpwm_scan_sampling_live_edits_false_and_reserved_words() {
        let mut m = Memory::default();
        m.set_word(d("D0"), 2);
        m.set_word(d("D2"), 5);
        m.set_word(d("D3"), 123);
        m.set_word(d("D4"), 456);
        let a = action("GPWM", &["D0", "D2", "M0"]);
        a.execute(&mut m, 1, true);
        assert!(m.bit(d("M0")));
        advance_by(&mut m, 2000);
        a.execute(&mut m, 1, true);
        assert!(!m.bit(d("M0")));
        m.set_word(d("D0"), 4);
        a.execute(&mut m, 1, true);
        assert!(m.bit(d("M0")));
        advance_by(&mut m, 3000);
        a.execute(&mut m, 1, true);
        assert!(m.bit(d("M0")));
        a.execute(&mut m, 1, false);
        assert!(!m.bit(d("M0")));
        advance_by(&mut m, 1000);
        a.execute(&mut m, 1, true);
        assert!(m.bit(d("M0")));
        assert_eq!(m.word(d("D3")), 123);
        assert_eq!(m.word(d("D4")), 456);
        m.set_word(d("D0"), 0);
        a.execute(&mut m, 1, true);
        assert!(!m.bit(d("M0")));
        m.set_word(d("D0"), 10);
        a.execute(&mut m, 1, true);
        assert!(m.bit(d("M0")));
    }
    #[test]
    fn csfo_timestamped_edges_generate_rate_pulses_and_latched_feedback() {
        let (mut m, a) = ready_csfo("X0", 1, 100);
        pulse(&mut m, 1000, x(0));
        pulse(&mut m, 2000, x(0));
        advance_by(&mut m, 2500);
        assert_eq!(m.timing_io.pulse_outputs[&0].frequency_hz, 1000);
        assert!(!m.bit(y(0))); // half a period since frequency was measured
        a.execute(&mut m, 7, true);
        assert_eq!(m.word(d("D1")), 1000);
        assert_eq!(m.double_word(d("D2")), 2);
        assert_eq!(m.double_word(d("D11")), 1000);
        advance_by(&mut m, 500);
        assert_eq!(m.timing_io.pulse_outputs[&0].accumulated_pulses, 1);
        assert!(m.bit(y(0)));
        assert!(!m.timing_io.pulse_trace.is_empty());
        m.set_word(d("D10"), 5);
        a.execute(&mut m, 7, true);
        assert_eq!(m.timing_io.pulse_outputs[&0].frequency_hz, 50);
        a.execute(&mut m, 7, false);
        assert!(!m.bit(y(0)));
        assert!(m.timing_io.output_owners.is_empty());
        a.execute(&mut m, 7, true);
        assert_eq!(m.timing_io.pulse_outputs[&0].frequency_hz, 0);
        assert_eq!(m.double_word(d("D2")), 2);
    }
    #[test]
    fn csfo_quadrature_signed_window_and_direction() {
        let (mut m, a) = ready_csfo("X1", 2, 10);
        m.timing_io.enqueue_input_edge(500, x(1), true).unwrap();
        pulse(&mut m, 1000, x(0));
        pulse(&mut m, 2000, x(0));
        pulse(&mut m, 3000, x(0));
        advance_by(&mut m, 3001);
        a.execute(&mut m, 7, true);
        assert_eq!(m.word(d("D1")), -1000);
        assert_eq!(m.double_word(d("D2")), -3);
        assert_eq!(m.double_word(d("D11")), -100);
        assert!(m.bit(y(1)));
        advance_by(&mut m, 10000);
        assert_eq!(m.timing_io.pulse_outputs[&0].accumulated_pulses, -1);
    }
    #[test]
    fn channel_conflicts_release_and_single_phase_keeps_normal_direction_point() {
        let mut m = Memory::default();
        m.set_word(d("D0"), 1);
        m.set_word(d("D10"), 100);
        m.timing_io.reserve_input_pair(0, "DCNT").unwrap();
        let a = action("CSFO", &["X0", "D0", "D10"]);
        a.execute(&mut m, 7, true);
        assert!(m.timing_io.pulse_outputs.is_empty());
        m.timing_io.release_external("DCNT");
        m.set_internal(y(1), true);
        a.execute(&mut m, 7, true);
        assert!(m.bit(y(1)));
        assert!(!m.timing_io.output_owners.contains_key(&y(1)));
        let b = action("CSFO", &["X1", "D0", "D10"]);
        b.execute(&mut m, 8, true);
        assert_eq!(m.timing_io.pulse_outputs[&0].owner, 7);
        a.execute(&mut m, 7, false);
        b.execute(&mut m, 8, true);
        assert_eq!(m.timing_io.pulse_outputs[&0].owner, 8);
    }
    #[test]
    fn aliases_snapshot_settings_clamps_and_undocumented_ranges_report_limits() {
        let mut m = Memory::default();
        m.set_word(d("D0"), 0);
        m.set_word(d("D1"), 100);
        let a = action("CSFO", &["X0", "D0", "D1"]);
        a.execute(&mut m, 1, true);
        assert_eq!(m.timing_io.csfo[&1].sample_count, 1);
        assert_eq!(m.timing_io.csfo[&1].proportion, 100);
        assert_eq!(m.word(d("D1")), 0); // feedback aliases the ratio, read first
        let b = action("GPWM", &["D10", "D11", "M0"]);
        m.set_word(d("D10"), 5);
        m.set_word(d("D11"), 0);
        b.execute(&mut m, 2, true);
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        m.data_register_limit = Some(4999);
        let c = action("GPWM", &["D4999", "D4998", "M1"]);
        c.execute(&mut m, 3, true);
        assert!(!m.bit(d("M1")));
    }
    #[test]
    fn queue_bounds_order_and_operand_matrices() {
        let mut state = TimingState::default();
        for _ in 0..MAX_EVENTS {
            state.enqueue_input_edge(1, x(0), false).unwrap();
        }
        assert!(state.enqueue_input_edge(1, x(0), true).is_err());
        state.now_us = 2;
        assert!(state.enqueue_input_edge(1, x(0), false).is_err());
        for (name, args) in [
            ("DELAY", vec!["T0"]),
            ("GPWM", vec!["K1", "D0", "Y0"]),
            ("GPWM", vec!["D0", "D9998", "Y0"]),
            ("CSFO", vec!["X4", "D0", "D10"]),
            ("CSFO", vec!["X0", "D9997", "D10"]),
        ] {
            assert!(
                TimingAction::parse(
                    name,
                    &args.into_iter().map(str::to_string).collect::<Vec<_>>()
                )
                .is_err()
            );
        }
    }
}
