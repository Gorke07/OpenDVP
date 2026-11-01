//! Documented virtual PLSY/PWM/PLSR engine. It is not the installed
//! simulator's coarse wall-clock approximation or a physical edge oracle.
use crate::calendar_operand::CalendarValue;
use crate::plc::{Area, Device, Memory};
use crate::timing_io::ResourceOwner;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
const SCALE: u128 = 1_000_000_000; // milli-Hz * microseconds per whole pulse.
fn dev(area: Area, index: u16) -> Device {
    Device { area, index }
}
fn lim(m: &mut Memory, s: impl Into<String>) {
    m.execution_limitations.insert(s.into());
}
#[derive(Debug, Clone, PartialEq)]
pub enum PulseAction {
    Train {
        wide: bool,
        frequency: CalendarValue,
        count: CalendarValue,
        dest: Device,
    },
    Pwm {
        width: CalendarValue,
        period: CalendarValue,
        dest: Device,
    },
    Ramp {
        wide: bool,
        frequency: CalendarValue,
        count: CalendarValue,
        ramp: CalendarValue,
        dest: Device,
    },
}
impl PulseAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let n = if matches!(name, "PLSR" | "DPLSR") {
            4
        } else {
            3
        };
        if args.len() != n || !matches!(name, "PLSY" | "DPLSY" | "PWM" | "PLSR" | "DPLSR") {
            return Err(
                "PLSY/DPLSY/PWM/PLSR/DPLSR operandları gerekli; P/DPWM belgelenmedi".into(),
            );
        }
        let dest: Device = args[n - 1].parse()?;
        if dest.area != Area::Y || ![0, 1, 2, 3, 4, 6].contains(&dest.index) {
            return Err(
                "Darbe hedefi Y0/Y1/Y2/Y3/Y4/Y6 olmalı; model koşulları yürütmede denetlenir"
                    .into(),
            );
        }
        let wide = name.starts_with('D');
        let a = CalendarValue::parse(&args[0], wide)?;
        let b = CalendarValue::parse(&args[1], wide)?;
        Ok(match name {
            "PWM" => Self::Pwm {
                width: a,
                period: b,
                dest,
            },
            "PLSR" | "DPLSR" => Self::Ramp {
                wide,
                frequency: a,
                count: b,
                ramp: CalendarValue::parse(&args[2], wide)?,
                dest,
            },
            _ => Self::Train {
                wide,
                frequency: a,
                count: b,
                dest,
            },
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Train { wide: false, .. } => "PLSY",
            Self::Train { wide: true, .. } => "DPLSY",
            Self::Pwm { .. } => "PWM",
            Self::Ramp { wide: false, .. } => "PLSR",
            Self::Ramp { wide: true, .. } => "DPLSR",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Train { wide: false, .. } => 0x8200,
            Self::Train { wide: true, .. } => 0x8300,
            Self::Pwm { .. } => 0x8400,
            Self::Ramp { wide: false, .. } => 0x8600,
            Self::Ramp { wide: true, .. } => 0x8700,
        }
    }
    pub fn words(&self) -> u16 {
        match self {
            Self::Train { wide: false, .. } | Self::Pwm { .. } => 7,
            Self::Train { wide: true, .. } => 13,
            Self::Ramp { wide: false, .. } => 9,
            Self::Ramp { wide: true, .. } => 17,
        }
    }
    pub fn wide(&self) -> bool {
        matches!(
            self,
            Self::Train { wide: true, .. } | Self::Ramp { wide: true, .. }
        )
    }
    pub fn destination(&self) -> Device {
        match self {
            Self::Train { dest, .. } | Self::Pwm { dest, .. } | Self::Ramp { dest, .. } => *dest,
        }
    }
    pub fn dest(&self) -> Device {
        self.destination()
    }
    pub fn label(&self) -> String {
        match self {
            Self::Train {
                frequency,
                count,
                dest,
                ..
            } => format!("{} {frequency} {count} {dest}", self.name()),
            Self::Pwm {
                width,
                period,
                dest,
            } => format!("PWM {width} {period} {dest}"),
            Self::Ramp {
                frequency,
                count,
                ramp,
                dest,
                ..
            } => format!("{} {frequency} {count} {ramp} {dest}", self.name()),
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        let p = self.dest().index as u8;
        format!(
            "{} pulses · {}",
            m.pulse_output.positions.get(&p).copied().unwrap_or(0),
            if m.pulse_output.is_busy(p) {
                "RUN"
            } else {
                "STOP"
            }
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Train {
                frequency, count, ..
            } => {
                frequency.devices(out);
                count.devices(out);
            }
            Self::Pwm { width, period, .. } => {
                width.devices(out);
                period.devices(out);
            }
            Self::Ramp {
                frequency,
                count,
                ramp,
                ..
            } => {
                frequency.devices(out);
                count.devices(out);
                ramp.devices(out);
            }
        }
        let p = self.dest().index as u8;
        out.insert(self.dest());
        for y in 0..=7 {
            out.insert(dev(Area::Y, y));
        }
        if let Some(a) = port(p) {
            for n in [a.complete, a.pause, a.ramp_pause, a.status, a.auto, a.low] {
                out.insert(dev(Area::M, n));
            }
            out.extend([dev(Area::D, a.pv), dev(Area::D, a.pv + 1)]);
        }
        for n in [
            1070, 1071, 1112, 1113, 1116, 1117, 1257, 1334, 1335, 1520, 1521, 1534, 1535, 1536,
            1537, 1604, 1605, 1606, 1607, 1610, 1611, 1680, 1681, 1682, 1683, 1684, 1685,
        ] {
            out.insert(dev(Area::M, n));
        }
        for n in [
            1220, 1221, 1340, 1341, 1342, 1343, 1348, 1349, 1350, 1351, 1352, 1353, 1379, 1380,
            1381, 1382, 1410, 1411, 1412, 1413, 1970, 1971, 1972, 1973, 1988, 1989, 1990, 1991,
        ] {
            out.insert(dev(Area::D, n));
        }
        if matches!(self, Self::Ramp { .. }) {
            for y in [0, 1, 2, 3, 4, 6] {
                let a = mark(y, false);
                out.insert(dev(Area::X, u16::from(a.input)));
                out.insert(dev(Area::M, a.enable));
                for base in [a.after, a.front, a.back] {
                    out.extend([dev(Area::D, base), dev(Area::D, base + 1)]);
                }
            }
            out.extend([
                dev(Area::D, 1127),
                dev(Area::D, 1128),
                dev(Area::D, 1133),
                dev(Area::D, 1134),
            ]);
        }
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let now = m.timing_io.now_us;
        if now != m.pulse_output.now_us {
            PulseState::advance(m, now);
        } else {
            PulseState::service_controls(m);
        }
        let mut state = std::mem::take(&mut m.pulse_output);
        state.visit(m, self, power, pc);
        m.pulse_output = state;
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PulseModel {
    #[default]
    Es2,
    Ss2,
    Ec5,
    Se2,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputOrder {
    #[default]
    FirstVisitedWins,
    LastVisitedWins,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RestartRamp {
    #[default]
    RemainingMove,
    NewMoveFromCurrentPosition,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ZeroMark {
    #[default]
    Disabled,
    NormalRampDown,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AutoRestart {
    #[default]
    OneFullIdleScan,
    NextVisitAfterEnd,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RampAb {
    #[default]
    TableModesOnly,
    AllowProseK2,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CountRounding {
    #[default]
    TruncateEachRampStep,
    CarryFractionBetweenSteps,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LowFrequency {
    #[default]
    UnverifiedHold,
    HundredthsHertzCandidate,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PositionWrites {
    #[default]
    OffsetFromLastPublished,
    AbsoluteAtVisit,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionPhase {
    RampUp,
    Cruise,
    RampDown,
    Compensation,
}
#[derive(Debug, Clone, Copy)]
struct Port {
    pv: u16,
    complete: u16,
    pause: u16,
    ramp_pause: u16,
    status: u16,
    auto: u16,
    low: u16,
}
fn port(y: u8) -> Option<Port> {
    let (pv, complete, pause, ramp_pause, status, auto, low) = match y {
        0 => (1030, 1029, 1078, 1108, 1538, 1347, 1190),
        1 => (1032, 1030, 1079, 1109, 1539, 1348, 1191),
        2 => (1336, 1102, 1104, 1110, 1540, 1524, 1192),
        3 => (1338, 1103, 1105, 1111, 1541, 1525, 1193),
        4 => (1375, 1321, 1310, 1133, 1542, 1526, 1183),
        6 => (1377, 1322, 1311, 1134, 1543, 1527, 1184),
        _ => return None,
    };
    Some(Port {
        pv,
        complete,
        pause,
        ramp_pause,
        status,
        auto,
        low,
    })
}
#[derive(Debug, Clone, Copy)]
struct Mark {
    input: u8,
    enable: u16,
    after: u16,
    front: u16,
    back: u16,
}
fn mark(y: u8, extended: bool) -> Mark {
    let k = if extended {
        match y {
            0 => 0,
            2 => 2,
            4 => 1,
            _ => 3,
        }
    } else {
        y.min(3)
    };
    match k {
        0 => Mark {
            input: 4,
            enable: 1156,
            after: 1232,
            front: 1026,
            back: 1100,
        },
        1 => Mark {
            input: if extended { 6 } else { 5 },
            enable: 1157,
            after: 1236,
            front: 1154,
            back: 1156,
        },
        2 => Mark {
            input: if extended { 5 } else { 6 },
            enable: 1158,
            after: 1234,
            front: 1135,
            back: 1102,
        },
        _ => Mark {
            input: 7,
            enable: 1159,
            after: 1238,
            front: 1158,
            back: 1160,
        },
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    Single(u8),
    Direction(u8),
    Quadrature(u8),
    CwCcw,
}
impl Route {
    fn pins(self) -> BTreeSet<Device> {
        let ys = match self {
            Self::Single(y) => vec![y],
            Self::Direction(y) | Self::Quadrature(y) => vec![y, y + 1],
            Self::CwCcw => vec![0, 2],
        };
        ys.into_iter().map(|y| dev(Area::Y, u16::from(y))).collect()
    }
}
#[derive(Debug, Clone, Copy)]
pub struct PulseSegment {
    pub duration_us: u64,
    pub frequency_millihz: u64,
    pub phase: MotionPhase,
}
/// Low-level axis requests use the same analytical accumulator as PLSY/PLSR.
/// Command-specific latching, homing and lifecycle stay with the caller.
#[derive(Debug, Clone)]
pub struct AxisRequest {
    pub owner_pc: u16,
    pub channel: u8,
    pub direction: Device,
    pub negative: bool,
    pub invert_direction: bool,
    pub target_pulses: Option<u64>,
    pub segments: VecDeque<PulseSegment>,
    pub completion: bool,
    pub paused: bool,
}
#[derive(Debug, Clone, Copy)]
pub struct AxisSnapshot {
    pub position: i32,
    pub emitted: u64,
    pub frequency_millihz: u64,
    pub phase: MotionPhase,
    pub busy: bool,
    pub done: bool,
    pub paused: bool,
}
#[derive(Debug, Clone)]
pub struct AxisRevision {
    pub frequency_millihz: Option<u64>,
    pub negative: Option<bool>,
    pub paused: Option<bool>,
    pub target_pulses: Option<Option<u64>>,
    pub segments: Option<VecDeque<PulseSegment>>,
}
#[derive(Debug, Clone, Copy)]
struct AxisPolicy {
    direction: Option<Device>,
    invert: bool,
    completion: bool,
    paused: bool,
}
#[derive(Debug, Clone)]
struct Run {
    axis: Option<AxisPolicy>,
    frequency_source: Option<CalendarValue>,
    pc: u16,
    y: u8,
    route: Route,
    active: bool,
    done: bool,
    restart_epoch: Option<u64>,
    negative: bool,
    target: Option<u64>,
    emitted: u64,
    base_position: i32,
    fraction: u64,
    segments: VecDeque<PulseSegment>,
    segment: PulseSegment,
    left: Option<u64>,
    pwm: Option<(u64, u64, u64)>,
    unit_us: u64,
    frequency: u64,
    kind: u8,
    up_us: u64,
    down_us: u64,
    start_hz: u64,
    mark_stop: bool,
    pause_target: Option<u64>,
    ramp_paused: bool,
    immediate_paused: bool,
    decelerating: bool,
    signature: String,
    mark_input: Option<Device>,
}
impl Run {
    fn physical_output_pins(&self) -> BTreeSet<Device> {
        let mut pins = self.route.pins();
        if let Some(axis) = self.axis
            && let Some(direction) = axis.direction
            && direction.area == Area::Y
        {
            pins.insert(direction);
        }
        pins
    }
}
#[derive(Debug, Clone)]
pub struct PulseState {
    pub model: PulseModel,
    pub output_order: OutputOrder,
    pub ramp_restart: RestartRamp,
    pub zero_mark: ZeroMark,
    pub auto_restart: AutoRestart,
    pub ramp_ab: RampAb,
    pub count_rounding: CountRounding,
    pub low_frequency: LowFrequency,
    pub position_writes: PositionWrites,
    pub es2_v5_odd_high_frequency: bool,
    pub high_speed_pwm: bool,
    pub mark_supported: bool,
    pub fixed_slope_supported: bool,
    pub unrestricted_mark_frequency_supported: bool,
    pub notices: BTreeSet<String>,
    pub positions: BTreeMap<u8, i32>,
    pub now_us: u64,
    running: bool,
    epoch: u64,
    runs: BTreeMap<u16, Run>,
    winners: BTreeMap<Device, u16>,
    published: BTreeMap<u8, i32>,
}
impl Default for PulseState {
    fn default() -> Self {
        Self {
            model: PulseModel::Es2,
            output_order: OutputOrder::FirstVisitedWins,
            ramp_restart: RestartRamp::RemainingMove,
            zero_mark: ZeroMark::Disabled,
            auto_restart: AutoRestart::OneFullIdleScan,
            ramp_ab: RampAb::TableModesOnly,
            count_rounding: CountRounding::TruncateEachRampStep,
            low_frequency: LowFrequency::UnverifiedHold,
            position_writes: PositionWrites::OffsetFromLastPublished,
            es2_v5_odd_high_frequency: false,
            high_speed_pwm: true,
            mark_supported: true,
            fixed_slope_supported: true,
            unrestricted_mark_frequency_supported: false,
            notices: BTreeSet::new(),
            positions: BTreeMap::new(),
            now_us: 0,
            running: true,
            epoch: 0,
            runs: BTreeMap::new(),
            winners: BTreeMap::new(),
            published: BTreeMap::new(),
        }
    }
}
fn read(v: &CalendarValue, m: &Memory) -> Result<i32, String> {
    let mut set = BTreeSet::new();
    v.devices(&mut set);
    if set.iter().any(|v| {
        v.index
            > match v.area {
                Area::D => m.data_register_limit.unwrap_or(9999),
                Area::M => m.relay_limit.unwrap_or(4095),
                Area::S => 1023,
                _ => 255,
            }
    }) {
        Err("Darbe operandının tam aralığı model belleğini aşıyor".into())
    } else {
        Ok(v.read(m))
    }
}
fn ceil_div(a: u128, b: u128) -> u64 {
    a.div_ceil(b).min(u128::from(u64::MAX)) as u64
}
fn counts(s: &PulseSegment) -> u64 {
    (u128::from(s.frequency_millihz) * u128::from(s.duration_us) / SCALE).min(u128::from(u64::MAX))
        as u64
}
fn shifts(start: u64, peak: u64, time: u64, n: u64, up: bool) -> Vec<PulseSegment> {
    (1..=n)
        .map(|i| {
            let f = if up {
                start + (peak - start) * i / n
            } else {
                peak - (peak - start) * i / n
            };
            PulseSegment {
                duration_us: (time / n).max(1),
                frequency_millihz: f * 1000,
                phase: if up {
                    MotionPhase::RampUp
                } else {
                    MotionPhase::RampDown
                },
            }
        })
        .collect()
}
fn plan(
    start: u64,
    peak: u64,
    up: u64,
    down: u64,
    n: u64,
    total: u64,
    limit_peak: bool,
) -> (VecDeque<PulseSegment>, u64, u64, u64) {
    let area = |p| {
        let a = shifts(start, p, up, n, true);
        let b = shifts(start, p, down, n, false);
        (
            a.iter().map(counts).sum::<u64>(),
            b.iter().map(counts).sum::<u64>(),
        )
    };
    let mut peak = peak.max(start);
    if limit_peak {
        let mut lo = start;
        let mut hi = peak;
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            let (a, b) = area(mid);
            if a.saturating_add(b) <= total {
                lo = mid
            } else {
                hi = mid - 1;
            }
        }
        peak = lo;
    }
    let (a, b) = area(peak);
    let mut out = VecDeque::new();
    if a.saturating_add(b) > total {
        if !limit_peak {
            out.extend(shifts(start, peak, up, n, true));
            out.extend(shifts(start, peak, down, n, false));
            return (out, peak, a, b);
        }
        out.push_back(PulseSegment {
            duration_us: ceil_div(u128::from(total) * SCALE, u128::from(start.max(1)) * 1000),
            frequency_millihz: start.max(1) * 1000,
            phase: MotionPhase::Compensation,
        });
        return (out, peak, 0, 0);
    }
    out.extend(shifts(start, peak, up, n, true));
    let coast = total - a - b;
    if coast > 0 {
        out.push_back(PulseSegment {
            duration_us: ceil_div(u128::from(coast) * SCALE, u128::from(peak.max(1)) * 1000),
            frequency_millihz: peak.max(1) * 1000,
            phase: MotionPhase::Cruise,
        });
    }
    out.extend(shifts(start, peak, down, n, false));
    (out, peak, a, b)
}
impl PulseState {
    /// Explicit shared 20/10-step local trajectory; not a hardware interpolation claim.
    pub fn axis_step_plan(
        start_hz: u64,
        peak_hz: u64,
        up_us: u64,
        down_us: u64,
        total: u64,
        steps: u64,
    ) -> VecDeque<PulseSegment> {
        plan(start_hz, peak_hz, up_us, down_us, steps, total, true).0
    }
    pub fn axis_snapshot(&self, pc: u16) -> Option<AxisSnapshot> {
        self.runs.get(&pc).map(|r| AxisSnapshot {
            position: Self::position(r),
            emitted: r.emitted,
            frequency_millihz: r.segment.frequency_millihz,
            phase: r.segment.phase,
            busy: r.active && !r.done,
            done: r.done,
            paused: r.immediate_paused,
        })
    }
    pub fn axis_owner(&self, channel: u8) -> Option<u16> {
        self.runs
            .iter()
            .find_map(|(&pc, r)| (r.active && r.y == channel).then_some(pc))
    }
    /// Immediate DICF partner lookup is by the genuine source DEVICE, not its value.
    pub fn train_source(&self, pc: u16) -> Option<&CalendarValue> {
        self.runs
            .get(&pc)
            .filter(|r| r.active && r.kind == 0)?
            .frequency_source
            .as_ref()
    }
    pub fn axis_position(&self, channel: u8, m: &Memory) -> i32 {
        self.starting_position(m, channel)
    }
    pub fn axis_publish(m: &mut Memory, pc: u16) {
        let mut s = std::mem::take(&mut m.pulse_output);
        if let Some(mut r) = s.runs.remove(&pc) {
            s.publish_visit(m, &mut r);
            s.runs.insert(pc, r);
        }
        m.pulse_output = s;
    }
    pub fn submit_axis(m: &mut Memory, request: AxisRequest, at_us: u64) -> Result<(), String> {
        if at_us != m.pulse_output.now_us {
            Self::advance(m, at_us);
        }
        let mut s = std::mem::take(&mut m.pulse_output);
        let result = s.submit_axis_inner(m, request, at_us, false);
        m.pulse_output = s;
        result
    }
    /// A directionless axis (DPTPO and pulse-only DVSPO). The request direction
    /// is ignored, never validated, written, or reserved.
    pub fn submit_single_axis(
        m: &mut Memory,
        request: AxisRequest,
        at_us: u64,
    ) -> Result<(), String> {
        if at_us != m.pulse_output.now_us {
            Self::advance(m, at_us);
        }
        let mut s = std::mem::take(&mut m.pulse_output);
        let result = s.submit_axis_inner(m, request, at_us, true);
        m.pulse_output = s;
        result
    }
    /// End an externally controlled move at its actual event timestamp while
    /// retaining its direction and ownership until the driving contact releases.
    pub fn finish_axis(m: &mut Memory, pc: u16, at_us: u64) {
        if at_us != m.pulse_output.now_us {
            Self::advance(m, at_us);
        }
        let mut s = std::mem::take(&mut m.pulse_output);
        if let Some(mut r) = s.runs.remove(&pc) {
            s.publish_visit(m, &mut r);
            s.complete(m, &mut r);
            s.runs.insert(pc, r);
        }
        m.pulse_output = s;
    }
    fn submit_axis_inner(
        &mut self,
        m: &mut Memory,
        q: AxisRequest,
        at_us: u64,
        single_output: bool,
    ) -> Result<(), String> {
        if at_us != self.now_us {
            return Err("Eksen zamanı ortak darbe saatiyle uyuşmuyor".into());
        }
        let p = port(q.channel).ok_or("Eksen kanalı desteklenmiyor")?;
        let model_channel = match self.model {
            PulseModel::Es2 => q.channel <= 3,
            PulseModel::Ss2 => matches!(q.channel, 0 | 2),
            PulseModel::Ec5 | PulseModel::Se2 => matches!(q.channel, 0 | 2 | 4 | 6),
        };
        if !model_channel || u16::from(q.channel) >= m.refresh_io.profile.output_points {
            return Err("Eksen kanalı seçilen CPU modelinde desteklenmiyor".into());
        }
        let direction_in_model = q.direction.index
            <= match q.direction.area {
                Area::Y => m
                    .refresh_io
                    .profile
                    .output_points
                    .saturating_sub(1)
                    .min(255),
                Area::M => m.relay_limit.unwrap_or(4095),
                Area::S => 1023,
                _ => 0,
            };
        if !single_output
            && (!q.direction.writable_bit()
                || q.direction.area == Area::Y && m.refresh_io.profile.output_points == 0
                || !direction_in_model
                || q.direction.to_string().parse::<Device>().ok() != Some(q.direction)
                || q.direction == dev(Area::Y, u16::from(q.channel)))
        {
            return Err("Eksen yön aygıtı yazılabilir ve modelde geçerli Y/M/S olmalı; darbe pininden farklı olmalı".into());
        }
        if !single_output && q.direction.area == Area::Y {
            let paired =
                q.channel.is_multiple_of(2) && q.direction.index == u16::from(q.channel) + 1;
            let modern_es2 = self.model == PulseModel::Es2
                && q.channel <= 3
                && q.direction.index == u16::from(q.channel) + 4;
            if !paired && !modern_es2 {
                return Err("Eksen yön çıkışı seçilen modelin kanal çiftine uymuyor".into());
            }
        }
        if q.segments.is_empty()
            || q.segments.len() > 128
            || q.segments.iter().any(|v| v.frequency_millihz > 100_000_000)
        {
            return Err("Eksen yolu 1..128 sonlu segment ve en fazla100kHz gerektirir".into());
        }
        let mut pins = BTreeSet::from([dev(Area::Y, u16::from(q.channel))]);
        if !single_output && q.direction.area == Area::Y {
            pins.insert(q.direction);
        }
        let (_, matrix_outputs) = m.refresh_io.matrix_pins();
        if matrix_outputs.iter().any(|v| pins.contains(v)) {
            return Err("Eksen çıkışı etkin MTR matris pinleriyle çakışıyor".into());
        }
        let owner = ResourceOwner::External(format!("PULSE:{}", q.owner_pc));
        if self.runs.iter().any(|(&id, r)| {
            id != q.owner_pc
                && r.active
                && (r.y == q.channel || !r.physical_output_pins().is_disjoint(&pins))
        }) || pins.iter().any(|v| {
            m.timing_io
                .output_owners
                .get(v)
                .is_some_and(|o| o != &owner)
        }) {
            return Err("Eksen çıkışı başka darbe/çevre birimi sahibince kullanılıyor".into());
        }
        if self.runs.get(&q.owner_pc).is_some_and(|r| r.active) {
            return Err("Etkin eksen submit ile yeniden başlatılamaz; revise kullanın".into());
        }
        if self.active_count() >= 4 {
            return Err("Aynı anda en fazla dört darbe/eksen sahibi".into());
        }
        let mut segments = q.segments;
        let segment = segments.pop_front().unwrap();
        let mut r = Run {
            axis: Some(AxisPolicy {
                direction: (!single_output).then_some(q.direction),
                invert: q.invert_direction,
                completion: q.completion,
                paused: q.paused,
            }),
            frequency_source: None,
            pc: q.owner_pc,
            y: q.channel,
            route: Route::Single(q.channel),
            active: true,
            done: false,
            restart_epoch: None,
            negative: q.negative,
            target: q.target_pulses,
            emitted: 0,
            base_position: self.starting_position(m, q.channel),
            fraction: 0,
            segments,
            segment,
            left: (segment.duration_us != 0).then_some(segment.duration_us),
            pwm: None,
            unit_us: 0,
            frequency: segment.frequency_millihz,
            kind: 3,
            up_us: 0,
            down_us: 0,
            start_hz: segment.frequency_millihz / 1000,
            mark_stop: false,
            pause_target: None,
            ramp_paused: false,
            immediate_paused: q.paused,
            decelerating: false,
            signature: String::new(),
            mark_input: None,
        };
        if q.completion {
            m.set_system(p.complete, false);
        }
        self.publish_visit(m, &mut r);
        if r.target == Some(0) {
            self.complete(m, &mut r);
        } else {
            self.levels(m, &r, true);
        }
        self.runs.insert(q.owner_pc, r);
        self.sync_claims(m);
        Ok(())
    }
    pub fn revise_axis(
        m: &mut Memory,
        pc: u16,
        change: AxisRevision,
        at_us: u64,
    ) -> Result<(), String> {
        if at_us != m.pulse_output.now_us {
            Self::advance(m, at_us);
        }
        if at_us != m.pulse_output.now_us {
            return Err("Eksen zamanı geriye gidiyor".into());
        }
        if change.frequency_millihz.is_some_and(|v| v > 100_000_000)
            || change.segments.as_ref().is_some_and(|v| {
                v.is_empty() || v.len() > 128 || v.iter().any(|s| s.frequency_millihz > 100_000_000)
            })
        {
            return Err("Eksen hız/segment sınırı geçersiz".into());
        }
        let mut s = std::mem::take(&mut m.pulse_output);
        let result = if let Some(mut r) = s.runs.remove(&pc) {
            let result = if !r.active || r.done {
                Err("Frekans değişikliği için etkin hareket gerekli".into())
            } else {
                if let Some(v) = change.frequency_millihz {
                    r.frequency = v;
                    r.segment.frequency_millihz = v;
                    r.segments.clear();
                    r.left = None;
                }
                if let Some(v) = change.negative {
                    let position = Self::position(&r);
                    r.negative = v;
                    r.base_position = if v {
                        position.wrapping_add(r.emitted as i32)
                    } else {
                        position.wrapping_sub(r.emitted as i32)
                    };
                }
                if let Some(v) = change.paused {
                    if let Some(a) = r.axis.as_mut() {
                        a.paused = v;
                    }
                    r.immediate_paused = v;
                }
                if let Some(v) = change.target_pulses {
                    r.target = v;
                }
                if let Some(mut v) = change.segments {
                    r.segment = v.pop_front().unwrap();
                    r.left = (r.segment.duration_us != 0).then_some(r.segment.duration_us);
                    r.segments = v;
                }
                s.publish(m, &r);
                s.levels(m, &r, true);
                Ok(())
            };
            s.runs.insert(pc, r);
            result
        } else {
            Err("Eksen sahibi bulunamadı".into())
        };
        m.pulse_output = s;
        result
    }
    pub fn release_axis(m: &mut Memory, pc: u16, clear_direction: bool, at_us: u64) {
        if at_us != m.pulse_output.now_us {
            Self::advance(m, at_us);
        }
        let mut s = std::mem::take(&mut m.pulse_output);
        if let Some(mut r) = s.runs.remove(&pc) {
            s.publish_visit(m, &mut r);
            r.active = false;
            s.levels(m, &r, false);
            if clear_direction
                && let Some(a) = r.axis
                && let Some(direction) = a.direction
            {
                m.set_internal(direction, false);
                if direction.area == Area::Y {
                    m.set_physical_output(direction, false);
                }
            }
        }
        s.sync_claims(m);
        m.pulse_output = s;
    }
    pub fn claimed_outputs(&self) -> BTreeMap<Device, ResourceOwner> {
        self.claims()
            .into_iter()
            .map(|(d, name)| (d, ResourceOwner::External(name)))
            .collect()
    }
    pub fn owned_outputs(&self) -> BTreeSet<Device> {
        self.claimed_outputs().into_keys().collect()
    }
    pub fn claimed_inputs(&self) -> BTreeMap<Device, ResourceOwner> {
        self.runs
            .values()
            .filter(|r| r.active)
            .filter_map(|r| {
                r.mark_input
                    .map(|input| (input, ResourceOwner::External(format!("PULSE:{}", r.pc))))
            })
            .collect()
    }
    pub fn claims(&self) -> BTreeMap<Device, String> {
        self.runs
            .values()
            .filter(|r| r.active)
            .flat_map(|r| {
                r.route
                    .pins()
                    .into_iter()
                    .chain(
                        r.axis
                            .and_then(|a| a.direction)
                            .filter(|d| d.area == Area::Y),
                    )
                    .map(move |d| (d, format!("PULSE:{}", r.pc)))
            })
            .collect()
    }
    pub fn is_busy(&self, y: u8) -> bool {
        self.runs.values().any(|r| r.active && r.y == y && !r.done)
    }
    pub fn active_count(&self) -> usize {
        self.runs.values().filter(|r| r.active).count()
    }
    /// Zero-time control service, only changed pause controls; it must not
    /// repaint every pulse pin after an unrelated ordinary logical OUT.
    pub fn service_controls(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.pulse_output);
        let ids: Vec<_> = s
            .runs
            .iter()
            .filter_map(|(&id, r)| {
                let p = port(r.y).unwrap();
                (r.active
                    && !r.done
                    && r.kind != 1
                    && r.axis.is_none()
                    && (r.immediate_paused != m.system_bit(p.pause)
                        || r.kind == 2
                            && (m.system_bit(p.ramp_pause) && !r.ramp_paused && !r.decelerating
                                || !m.system_bit(p.ramp_pause) && r.ramp_paused)))
                    .then_some(id)
            })
            .collect();
        for id in ids {
            let mut r = s.runs.remove(&id).unwrap();
            s.advance_run(m, &mut r, 0);
            s.runs.insert(id, r);
        }
        m.pulse_output = s;
    }
    pub fn begin_scan(m: &mut Memory) {
        m.pulse_output.epoch = m.pulse_output.epoch.wrapping_add(1);
        m.pulse_output.winners.clear();
    }
    pub fn end_scan(m: &mut Memory) {
        let mut state = std::mem::take(&mut m.pulse_output);
        for r in state.runs.values_mut() {
            if r.active
                && r.axis.is_none()
                && r.done
                && m.system_bit(port(r.y).unwrap().auto)
                && r.restart_epoch.is_none()
            {
                r.restart_epoch = Some(
                    state.epoch
                        + if state.auto_restart == AutoRestart::OneFullIdleScan {
                            2
                        } else {
                            1
                        },
                );
            }
        }
        m.pulse_output = state;
    }
    /// Discards executing owners and their claims; watchdog caller clears Y.
    /// No asynchronous resume is possible until another actual powered visit.
    pub fn abort(m: &mut Memory) {
        m.pulse_output.runs.clear();
        m.pulse_output.winners.clear();
        m.pulse_output.running = false;
        m.timing_io
            .output_owners
            .retain(|_, v| !matches!(v,ResourceOwner::External(s) if s.starts_with("PULSE:")));
        m.timing_io
            .input_owners
            .retain(|_, v| !matches!(v,ResourceOwner::External(s) if s.starts_with("PULSE:")));
    }
    pub fn set_running(m: &mut Memory, running: bool) {
        m.pulse_output.running = running;
    }
    fn max_hz(&self, y: u8, wide: bool) -> u64 {
        if self.model == PulseModel::Ss2 {
            10_000
        } else if self.model == PulseModel::Ec5 && wide {
            50_000
        } else if y % 2 == 1 {
            if wide && self.model == PulseModel::Es2 && self.es2_v5_odd_high_frequency {
                100_000
            } else {
                10_000
            }
        } else if wide {
            100_000
        } else {
            32767
        }
    }
    fn route(&self, m: &Memory, y: u8, ramp: bool) -> Result<Route, String> {
        let extended = matches!(self.model, PulseModel::Ec5 | PulseModel::Se2);
        if extended && y % 2 == 1 || !extended && y >= 4 {
            return Err("Darbe çıkışı seçilen CPU ailesinde desteklenmiyor".into());
        }
        if !extended && m.word(dev(Area::D, 1220)) == 3 && y == 0 && !ramp {
            return Ok(Route::CwCcw);
        }
        if !extended && m.word(dev(Area::D, 1220)) == 3 && y == 2 {
            return Err("CW/CCW modunda Y2 eş çıkışının bağımsız komut sahipliği doğrulanmadı; Y0 ana kanalı kullanılmalı".into());
        }
        let mode = m.word(dev(
            Area::D,
            match y {
                0 | 1 => 1220,
                2 | 3 => 1221,
                4 => 1341,
                _ => 1342,
            },
        ));
        if y % 2 == 1 {
            if mode == 0 || !ramp && m.word(dev(Area::D, 1220)) == 3 {
                return Ok(Route::Single(y));
            }
            return Err("Eş yön/AB pini bağımsız darbe çıkışı olarak kullanılamaz".into());
        }
        match mode {0=>Ok(Route::Single(y)),1=>Ok(Route::Direction(y)),2 if !ramp || self.ramp_ab==RampAb::AllowProseK2=>Ok(Route::Quadrature(y)),_=>Err("PLSR mod tablosu yalnız K0/K1 tanımlar; K2 prose profili ayrı, diğer modlar doğrulanmadı".into())}
    }
    fn pwm_unit(&self, m: &Memory, y: u8) -> Result<u64, String> {
        if y.is_multiple_of(2) {
            if m.system_bit(1116) {
                if !self.high_speed_pwm || self.model == PulseModel::Ec5 {
                    return Err("PWM 1us modu bu firmware/CPU profilinde yok".into());
                }
                Ok(1)
            } else {
                Ok(
                    if m.system_bit(match y {
                        0 => 1112,
                        2 => 1113,
                        4 => 1530,
                        _ => 1531,
                    }) {
                        10
                    } else {
                        100
                    },
                )
            }
        } else if m.system_bit(1117) {
            if !self.high_speed_pwm {
                Err("PWM 10us modu bu firmware profilinde yok".into())
            } else {
                Ok(10)
            }
        } else {
            Ok(if m.system_bit(if y == 1 { 1070 } else { 1071 }) {
                100
            } else {
                1000
            })
        }
    }
    fn sync_claims(&self, m: &mut Memory) {
        m.timing_io
            .output_owners
            .retain(|_, v| !matches!(v,ResourceOwner::External(s) if s.starts_with("PULSE:")));
        m.timing_io
            .input_owners
            .retain(|_, v| !matches!(v,ResourceOwner::External(s) if s.starts_with("PULSE:")));
        for (d, owner) in self.claims() {
            m.timing_io
                .output_owners
                .entry(d)
                .or_insert(ResourceOwner::External(owner));
        }
        for (d, owner) in self.claimed_inputs() {
            m.timing_io.input_owners.entry(d).or_insert(owner);
        }
    }
    fn visit(&mut self, m: &mut Memory, a: &PulseAction, power: bool, pc: u16) {
        let y = a.dest().index as u8;
        let p = port(y).unwrap();
        if !power {
            if !matches!(a, PulseAction::Pwm { .. }) {
                m.set_system(p.complete, false);
            }
            if let Some(mut r) = self.runs.remove(&pc) {
                if r.active {
                    self.publish_visit(m, &mut r);
                    r.active = false;
                    self.levels(m, &r, false);
                    if r.kind != 1 {
                        m.set_system(p.status, false);
                    }
                }
                if r.kind == 2 && self.ramp_restart == RestartRamp::RemainingMove && !r.done {
                    self.runs.insert(pc, r);
                }
            }
            self.sync_claims(m);
            return;
        }
        self.running = true;
        if self.runs.len() >= 512 && !self.runs.contains_key(&pc) {
            lim(m, "Darbe komutu site sınırı512; yeni komut tutuldu");
            return;
        }
        let kind = if matches!(a, PulseAction::Pwm { .. }) {
            1
        } else if matches!(a, PulseAction::Ramp { .. }) {
            2
        } else {
            0
        };
        let route = if let Some(r) = self.runs.get(&pc).filter(|r| r.active) {
            r.route
        } else {
            match self.route(m, y, kind == 2) {
                Ok(r) => r,
                Err(e) => {
                    lim(m, e);
                    return;
                }
            }
        };
        let pins = route.pins();
        // An axis is an already activated owner, including its explicit
        // direction pin. Legacy per-scan first/last policies apply only among
        // legacy pulse commands and must not evict this ongoing motion.
        if self.runs.iter().any(|(&id, r)| {
            id != pc && r.active && r.axis.is_some() && !r.physical_output_pins().is_disjoint(&pins)
        }) {
            self.notices
                .insert("Etkin hareket ekseni darbe ve yön pinlerinde önceliklidir".into());
            return;
        }
        if kind == 1 && route != Route::Single(y) {
            lim(
                m,
                "PWM için tek-pin darbe çıkış modu gerekli; eş pin yön/AB davranışı doğrulanmadı",
            );
            return;
        }
        if pins.iter().any(|p| {
            m.timing_io
                .output_owners
                .get(p)
                .is_some_and(|v| !matches!(v,ResourceOwner::External(s) if s.starts_with("PULSE:")))
        }) {
            lim(
                m,
                "Darbe çıkışı başka bir PWM/CSFO/MTR/kaynak sahibi tarafından kullanılıyor",
            );
            return;
        }
        let competitors: Vec<_> = self
            .runs
            .iter()
            .filter(|(id, r)| {
                **id != pc && r.active && !r.physical_output_pins().is_disjoint(&pins)
            })
            .map(|(&id, _)| id)
            .collect();
        if self.output_order == OutputOrder::FirstVisitedWins
            && pins
                .iter()
                .any(|p| self.winners.get(p).is_some_and(|&id| id != pc))
        {
            self.notices.insert(
                "Aynı darbe kanalında ilk ziyaret edilen komut bu taramada öncelikli".into(),
            );
            return;
        }
        if self.active_count().saturating_sub(competitors.len()) >= 4
            && !self.runs.get(&pc).is_some_and(|r| r.active)
        {
            lim(
                m,
                "PLSY/PWM/PLSR aynı anda en fazla dört komut çalıştırabilir",
            );
            return;
        }
        if self
            .runs
            .get(&pc)
            .is_some_and(|r| r.active && r.done && r.restart_epoch.is_none_or(|e| self.epoch < e))
        {
            let mut r = self.runs.remove(&pc).unwrap();
            self.publish_visit(m, &mut r);
            self.runs.insert(pc, r);
            return;
        }
        let raw = match a {
            PulseAction::Train {
                frequency, count, ..
            } => read(frequency, m).and_then(|f| Ok((f, read(count, m)?, 0))),
            PulseAction::Pwm { width, period, .. } => {
                read(width, m).and_then(|f| Ok((f, read(period, m)?, 0)))
            }
            PulseAction::Ramp {
                frequency,
                count,
                ramp,
                ..
            } => read(frequency, m).and_then(|f| Ok((f, read(count, m)?, read(ramp, m)?))),
        };
        let (frequency, count, ramp) = match raw {
            Ok(v) => v,
            Err(e) => {
                lim(m, e);
                return;
            }
        };
        if kind == 0 && m.system_bit(p.low) && self.low_frequency == LowFrequency::UnverifiedHold {
            lim(
                m,
                "PLSY düşük frekans ham operand ölçeği PDF'de açıklanmadı; açık aday profili olmadan tutuldu",
            );
            return;
        }
        if let Some(mut r) = self.runs.remove(&pc) {
            if r.active && !r.done {
                if kind == 0 {
                    let low = m.system_bit(p.low);
                    let rate = if low {
                        frequency.clamp(0, 1000) as u64 * 10
                    } else {
                        (frequency.max(0) as u64).min(self.max_hz(y, a.wide())) * 1000
                    };
                    r.frequency = rate;
                    r.segment.frequency_millihz = rate;
                }
                if kind == 1 {
                    let max = if y.is_multiple_of(2) { 10000 } else { 32767 };
                    let valid = frequency > 0
                        && count > 0
                        && frequency <= count
                        && frequency <= max
                        && count <= max;
                    if valid && self.model == PulseModel::Ss2 && r.unit_us == 1 && frequency <= 20 {
                        lim(
                            m,
                            "SS2 PWM 1us modunda genişlik20 üzeri olmalı; önceki geçerli çıkış ayarı korundu",
                        );
                        self.runs.insert(pc, r);
                        return;
                    }
                    let period = if valid { count as u64 * r.unit_us } else { 1 };
                    r.pwm = Some((
                        if valid {
                            frequency as u64 * r.unit_us
                        } else {
                            0
                        },
                        period,
                        r.pwm.unwrap().2 % period,
                    ));
                }
                self.conditional_stop(m, &mut r);
                self.commit_ownership(pc, &pins, &competitors);
                self.publish_visit(m, &mut r);
                self.levels(m, &r, true);
                self.runs.insert(pc, r);
                self.sync_claims(m);
                return;
            }
            if !r.active
                && kind == 2
                && self.ramp_restart == RestartRamp::RemainingMove
                && r.signature == format!("{frequency}/{count}/{ramp}")
                && r.route == route
                && !r.done
            {
                let owner = ResourceOwner::External(format!("PULSE:{pc}"));
                if r.mark_input.is_some_and(|input| {
                    m.timing_io
                        .input_owners
                        .get(&input)
                        .is_some_and(|current| current != &owner)
                }) {
                    lim(
                        m,
                        "PLSR devam hareketinin işaret girişi başka hızlı sayaç/hız/darbe kaynağına ait; hareket pasif tutuldu",
                    );
                    self.runs.insert(pc, r);
                    return;
                }
                r.active = true;
                r.ramp_paused = false;
                r.decelerating = false;
                self.replan(&mut r);
                self.commit_ownership(pc, &pins, &competitors);
                self.publish_visit(m, &mut r);
                self.levels(m, &r, true);
                self.runs.insert(pc, r);
                m.set_system(p.complete, false);
                self.sync_claims(m);
                return;
            }
        }
        let negative = count < 0;
        let signed_total = i64::from(count).unsigned_abs();
        let target = if kind == 0 && count == 0 {
            None
        } else {
            Some(signed_total)
        };
        if kind == 2 && count == 0 {
            lim(
                m,
                "PLSR sıfır darbe sayısı sürekli çıkış değildir; hareket başlatılmadı",
            );
            return;
        }
        if kind == 2 && negative && !matches!(route, Route::Direction(_) | Route::Quadrature(_)) {
            lim(m, "PLSR negatif sayı yalnız yönlü modda belgelenmiştir");
            return;
        }
        let low = kind == 0 && m.system_bit(p.low);
        let rate = if low {
            frequency.clamp(0, 1000) as u64 * 10
        } else {
            (frequency.max(if kind == 2 { 6 } else { 0 }) as u64).min(self.max_hz(y, a.wide()))
                * 1000
        };
        let mut r = Run {
            axis: None,
            frequency_source: match a {
                PulseAction::Train { frequency, .. } => Some(frequency.clone()),
                _ => None,
            },
            pc,
            y,
            route,
            active: true,
            done: false,
            restart_epoch: None,
            negative,
            target,
            emitted: 0,
            base_position: self.starting_position(m, y),
            fraction: 0,
            segments: VecDeque::new(),
            segment: PulseSegment {
                duration_us: 0,
                frequency_millihz: rate,
                phase: MotionPhase::Cruise,
            },
            left: None,
            pwm: None,
            unit_us: 0,
            frequency: rate,
            kind,
            up_us: 0,
            down_us: 0,
            start_hz: 0,
            mark_stop: false,
            pause_target: None,
            ramp_paused: false,
            immediate_paused: false,
            decelerating: false,
            signature: format!("{frequency}/{count}/{ramp}"),
            mark_input: None,
        };
        if kind == 1 {
            r.unit_us = match self.pwm_unit(m, y) {
                Ok(v) => v,
                Err(e) => {
                    lim(m, e);
                    return;
                }
            };
            let max = if y.is_multiple_of(2) { 10000 } else { 32767 };
            let valid = frequency > 0
                && count > 0
                && frequency <= count
                && frequency <= max
                && count <= max;
            if self.model == PulseModel::Ss2 && r.unit_us == 1 && frequency <= 20 && valid {
                lim(
                    m,
                    "SS2 PWM 1us modunda genişlik20 üzeri olmalı; fiziksel bant genişliği nedeniyle tutuldu",
                );
                return;
            }
            r.pwm = Some((
                if valid {
                    frequency as u64 * r.unit_us
                } else {
                    0
                },
                if valid { count as u64 * r.unit_us } else { 1 },
                0,
            ));
            r.target = None;
        }
        if kind == 2 {
            let mark = mark(y, matches!(self.model, PulseModel::Ec5 | PulseModel::Se2));
            if m.system_bit(mark.enable) {
                if !self.mark_supported || self.model == PulseModel::Ss2 && y % 2 == 1 {
                    lim(
                        m,
                        "PLSR işaret kanalı seçilen CPU/firmware profilinde desteklenmiyor",
                    );
                    return;
                }
                let input = dev(Area::X, u16::from(mark.input));
                let owner = ResourceOwner::External(format!("PULSE:{pc}"));
                if m.timing_io
                    .input_owners
                    .get(&input)
                    .is_some_and(|o| o != &owner)
                {
                    lim(
                        m,
                        "PLSR işaret girişi başka hızlı sayaç/hız/darbe kaynağına ait",
                    );
                    return;
                }
                r.mark_input = Some(input);
            }
            if y.is_multiple_of(2) && m.system_bit(1257) {
                lim(
                    m,
                    "PLSR S-eğrisi katsayı/step tablosu doğrulanmadı; düz rampa yerine sessizce konulmadı",
                );
                return;
            }
            r.up_us = ramp.max(20) as u64 * 1000;
            r.down_us = if y.is_multiple_of(2)
                && (m.system_bit(1534 + u16::from(y / 2)) || m.system_bit(mark.enable))
            {
                m.word(dev(
                    Area::D,
                    match y {
                        0 => 1348,
                        2 => 1349,
                        4 => 1350,
                        _ => 1351,
                    },
                ))
                .max(20) as u64
                    * 1000
            } else {
                r.up_us
            };
            r.start_hz = if y.is_multiple_of(2) {
                let d = match y {
                    0 => 1340,
                    2 => 1352,
                    4 => 1379,
                    _ => 1380,
                };
                (m.word(dev(Area::D, d)).max(6) as u64).min(rate / 1000)
            } else {
                0
            };
            let (fixed, reg) = match y {
                0 => (1604, 1410),
                1 => (1606, 1988),
                2 => (1605, 1412),
                3 => (1607, 1990),
                4 => (1610, 1970),
                _ => (1611, 1972),
            };
            if m.system_bit(fixed) {
                if !self.fixed_slope_supported || self.model == PulseModel::Ss2 && y % 2 == 1 {
                    lim(
                        m,
                        "PLSR sabit eğim seçilen model/firmware kanalında desteklenmiyor",
                    );
                    return;
                }
                let max = m.double_word(dev(Area::D, reg));
                if max <= r.start_hz as i32 {
                    lim(
                        m,
                        "PLSR sabit eğim maksimum frekansı başlangıç frekansından büyük olmalı",
                    );
                    return;
                }
                r.up_us = ((u128::from(r.up_us) * u128::from(rate / 1000 - r.start_hz))
                    / u128::from(max as u64 - r.start_hz))
                .max(1)
                .min(u128::from(u64::MAX)) as u64;
                r.down_us = ((u128::from(r.down_us) * u128::from(rate / 1000 - r.start_hz))
                    / u128::from(max as u64 - r.start_hz))
                .max(1)
                .min(u128::from(u64::MAX)) as u64;
            }
            let unconstrained = m.system_bit(
                1680 + match y {
                    0..=3 => u16::from(y),
                    4 => 4,
                    _ => 5,
                },
            );
            if unconstrained
                && (!self.unrestricted_mark_frequency_supported || self.model == PulseModel::Ss2)
            {
                lim(
                    m,
                    "PLSR hedef frekans sınırını kaldırma seçilen firmware/CPU profilinde yok",
                );
                return;
            }
            let (q, peak, up, down) = plan(
                r.start_hz,
                rate / 1000,
                r.up_us,
                r.down_us,
                if y.is_multiple_of(2) { 20 } else { 10 },
                signed_total,
                !unconstrained,
            );
            r.frequency = peak * 1000;
            r.segments = q;
            r.segment = r.segments.pop_front().unwrap();
            r.left = Some(r.segment.duration_us);
            if y == 0 {
                m.set_double_word_raw(dev(Area::D, 1127), up as i32);
                m.set_double_word_raw(dev(Area::D, 1133), down as i32);
            }
            self.notices.insert("PLSR: belgeli20/10 basamaklı yerel profil; tam PLC basamak yuvarlaması/S-eğrisi/işaret modeli eşdeğerliği değildir".into());
        }
        self.conditional_stop(m, &mut r);
        self.commit_ownership(pc, &pins, &competitors);
        if kind != 1 {
            m.set_system(p.complete, false);
            m.set_system(p.status, false);
        }
        self.publish(m, &r);
        let position = Self::position(&r);
        m.set_double_word_raw(dev(Area::D, p.pv), position);
        self.published.insert(y, position);
        self.levels(m, &r, true);
        self.runs.insert(pc, r);
        self.sync_claims(m);
    }
    fn conditional_stop(&self, m: &mut Memory, r: &mut Run) {
        if r.kind != 2 || r.y % 2 == 1 {
            return;
        }
        let flag = match r.y {
            0 => 1334,
            2 => 1335,
            4 => 1520,
            _ => 1521,
        };
        if m.system_bit(flag) {
            m.set_system(flag, false);
            m.set_system(port(r.y).unwrap().ramp_pause, true);
            if !r.decelerating && !r.ramp_paused {
                self.ramp_stop(r, None);
            }
        }
    }
    fn commit_ownership(&mut self, pc: u16, pins: &BTreeSet<Device>, competitors: &[u16]) {
        for id in competitors {
            if let Some(r) = self.runs.get_mut(id) {
                r.active = false;
            }
        }
        for &pin in pins {
            self.winners.insert(pin, pc);
        }
    }
    fn replan(&self, r: &mut Run) {
        let remaining = r.target.unwrap_or(0).saturating_sub(r.emitted);
        let (q, _, _, _) = plan(
            r.start_hz,
            r.frequency / 1000,
            r.up_us,
            r.down_us,
            if r.y.is_multiple_of(2) { 20 } else { 10 },
            remaining,
            true,
        );
        r.segments = q;
        r.segment = r.segments.pop_front().unwrap();
        r.left = Some(r.segment.duration_us);
        r.fraction = 0;
        r.pause_target = None;
    }
    fn levels(&self, m: &mut Memory, r: &Run, enabled: bool) {
        let on = enabled && r.active && !r.done && !r.ramp_paused && !r.immediate_paused;
        let (a, b) = if let Some((width, period, phase)) = r.pwm {
            (on && width > 0 && phase % period < width, false)
        } else {
            let q = u128::from(r.fraction);
            (
                on && r.segment.frequency_millihz != 0 && q < SCALE / 2,
                on && r.segment.frequency_millihz != 0 && (SCALE / 4..SCALE * 3 / 4).contains(&q),
            )
        };
        if let Some(axis) = r.axis {
            let output = dev(Area::Y, u16::from(r.y));
            m.set_internal(output, a);
            m.set_physical_output(output, a);
            if let Some(direction) = axis.direction {
                m.set_internal(direction, r.negative ^ axis.invert);
                if direction.area == Area::Y {
                    m.set_physical_output(direction, r.negative ^ axis.invert);
                }
            }
            return;
        }
        let values: Vec<_> = match r.route {
            Route::Single(y) => vec![(y, a)],
            Route::Direction(y) => vec![(y, a), (y + 1, r.negative)],
            Route::Quadrature(y) => {
                if r.negative {
                    vec![(y, b), (y + 1, a)]
                } else {
                    vec![(y, a), (y + 1, b)]
                }
            }
            Route::CwCcw => vec![(0, a && !r.negative), (2, a && r.negative)],
        };
        for (y, on) in values {
            m.set_internal(dev(Area::Y, u16::from(y)), on);
            m.set_physical_output(dev(Area::Y, u16::from(y)), on);
        }
    }
    fn position(r: &Run) -> i32 {
        let delta = r.emitted as u32 as i32;
        if r.negative {
            r.base_position.wrapping_sub(delta)
        } else {
            r.base_position.wrapping_add(delta)
        }
    }
    fn starting_position(&self, m: &Memory, y: u8) -> i32 {
        let observed = m.double_word(dev(Area::D, port(y).unwrap().pv));
        let current = self.positions.get(&y).copied().unwrap_or(observed);
        if let Some(&last) = self.published.get(&y) {
            if observed == last || observed == current {
                current
            } else if self.position_writes == PositionWrites::AbsoluteAtVisit {
                observed
            } else {
                current.wrapping_add(observed.wrapping_sub(last))
            }
        } else {
            observed
        }
    }
    fn publish(&mut self, m: &mut Memory, r: &Run) {
        let position = Self::position(r);
        self.positions.insert(r.y, position);
        m.refresh_io.pulse_positions.insert(r.y, position);
    }
    fn publish_visit(&mut self, m: &mut Memory, r: &mut Run) {
        let observed = m.double_word(dev(Area::D, port(r.y).unwrap().pv));
        let current = Self::position(r);
        if let Some(&last) = self.published.get(&r.y)
            && observed != last
            && observed != current
        {
            r.base_position = r.base_position.wrapping_add(
                if self.position_writes == PositionWrites::AbsoluteAtVisit {
                    observed.wrapping_sub(current)
                } else {
                    observed.wrapping_sub(last)
                },
            );
        }
        self.publish(m, r);
        let value = Self::position(r);
        m.set_double_word_raw(dev(Area::D, port(r.y).unwrap().pv), value);
        self.published.insert(r.y, value);
    }
    fn complete(&self, m: &mut Memory, r: &mut Run) {
        if r.pause_target.is_some() {
            r.ramp_paused = true;
            r.decelerating = false;
            r.pause_target = None;
            m.set_system(port(r.y).unwrap().status, true);
        } else {
            r.done = true;
            if r.axis.is_none_or(|a| a.completion) {
                m.set_system(port(r.y).unwrap().complete, true);
            }
        }
        self.levels(m, r, false);
    }
    fn ramp_stop(&self, r: &mut Run, after: Option<u64>) {
        let n = if r.y.is_multiple_of(2) { 20 } else { 10 };
        let current = r.segment.frequency_millihz / 1000;
        let down = shifts(0, current, r.down_us.max(20_000), n, false);
        let natural = down.iter().map(counts).sum::<u64>();
        let distance = after
            .unwrap_or(natural)
            .min(r.target.unwrap_or(u64::MAX).saturating_sub(r.emitted));
        r.pause_target = Some(r.emitted.saturating_add(distance));
        r.decelerating = true;
        r.segments = VecDeque::new();
        if distance > natural && current > 0 {
            r.segments.push_back(PulseSegment {
                duration_us: ceil_div(
                    u128::from(distance - natural) * SCALE,
                    u128::from(current) * 1000,
                ),
                frequency_millihz: current * 1000,
                phase: MotionPhase::Cruise,
            });
        }
        r.segments.extend(down);
        r.segment = r.segments.pop_front().unwrap();
        r.left = Some(r.segment.duration_us);
        r.fraction = 0;
    }
    fn advance_run(&mut self, m: &mut Memory, r: &mut Run, mut dt: u64) {
        let p = port(r.y).unwrap();
        r.immediate_paused = r
            .axis
            .map_or(r.kind != 1 && m.system_bit(p.pause), |a| a.paused);
        if r.immediate_paused {
            m.set_system(p.status, true);
            self.levels(m, r, false);
            return;
        }
        if r.kind == 2 {
            if m.system_bit(p.ramp_pause) && !r.decelerating && !r.ramp_paused {
                self.ramp_stop(r, None);
            }
            if !m.system_bit(p.ramp_pause) && r.ramp_paused {
                r.ramp_paused = false;
                r.mark_stop = false;
                self.replan(r);
            }
            if r.ramp_paused {
                self.levels(m, r, false);
                return;
            }
        }
        if r.pause_target == Some(r.emitted) {
            self.complete(m, r);
            return;
        }
        if r.kind != 1 && r.axis.is_none() {
            m.set_system(p.status, false);
        }
        if let Some((width, period, phase)) = r.pwm {
            if width == 0 {
                self.levels(m, r, false);
                return;
            }
            let total = u128::from(phase) + u128::from(dt);
            r.emitted = r.emitted.wrapping_add((total / u128::from(period)) as u64);
            r.pwm = Some((width, period, (total % u128::from(period)) as u64));
            self.publish(m, r);
            self.levels(m, r, true);
            return;
        }
        let mut steps = 0;
        while dt > 0 && !r.done && !r.ramp_paused {
            steps += 1;
            if steps > 128 {
                lim(m, "Darbe segment sınırı aşıldı; ilerleme tutuldu");
                break;
            }
            let rate = r.segment.frequency_millihz;
            let end = r.pause_target.or(r.target);
            let remaining = end.map(|v| v.saturating_sub(r.emitted));
            if remaining == Some(0) {
                self.complete(m, r);
                break;
            }
            let to_target = if rate > 0 {
                remaining.map(|v| {
                    ceil_div(
                        u128::from(v) * SCALE - u128::from(r.fraction),
                        u128::from(rate),
                    )
                })
            } else {
                None
            };
            let used = dt
                .min(r.left.unwrap_or(u64::MAX))
                .min(to_target.unwrap_or(u64::MAX));
            let total = u128::from(r.fraction) + u128::from(rate) * u128::from(used);
            r.emitted = r.emitted.saturating_add((total / SCALE) as u64);
            r.fraction = (total % SCALE) as u64;
            dt -= used;
            if let Some(left) = r.left.as_mut() {
                *left -= used;
            }
            if end.is_some_and(|v| r.emitted >= v) {
                self.complete(m, r);
                break;
            }
            if r.left == Some(0) {
                if let Some(next) = r.segments.pop_front() {
                    r.segment = next;
                    r.left = Some(next.duration_us);
                    if r.axis.is_none()
                        && self.count_rounding == CountRounding::TruncateEachRampStep
                    {
                        r.fraction = 0;
                    }
                } else {
                    r.segment = PulseSegment {
                        duration_us: 0,
                        frequency_millihz: if r.axis.is_some() {
                            r.segment.frequency_millihz
                        } else {
                            r.start_hz.max(6) * 1000
                        },
                        phase: MotionPhase::Compensation,
                    };
                    r.left = None;
                }
            }
            if used == 0 && r.left != Some(0) {
                break;
            }
        }
        self.publish(m, r);
        self.levels(m, r, true);
    }
    /// Analytical pulse accumulation: visits <=128 ramp segments, never every
    /// pulse. Physical levels are samples at integer-us observation times.
    pub fn advance(m: &mut Memory, at_us: u64) {
        let mut state = std::mem::take(&mut m.pulse_output);
        if at_us < state.now_us {
            lim(m, "Darbe motoru zamanı geriye gidiyor");
            m.pulse_output = state;
            return;
        }
        let dt = at_us - state.now_us;
        state.now_us = at_us;
        if state.running {
            let ids: Vec<_> = state.runs.keys().copied().collect();
            for id in ids {
                let mut r = state.runs.remove(&id).unwrap();
                if r.active && !r.done {
                    state.advance_run(m, &mut r, dt);
                }
                state.runs.insert(id, r);
            }
        }
        m.pulse_output = state;
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        if !self.running {
            return None;
        }
        self.runs
            .values()
            .filter(|r| r.active && !r.done && !r.ramp_paused && !r.immediate_paused)
            .filter_map(|r| {
                let target = r.pause_target.or(r.target);
                let to_target = if r.segment.frequency_millihz > 0 {
                    target.map(|v| {
                        ceil_div(
                            (u128::from(v.saturating_sub(r.emitted)) * SCALE)
                                .saturating_sub(u128::from(r.fraction)),
                            u128::from(r.segment.frequency_millihz),
                        )
                    })
                } else {
                    None
                };
                r.left
                    .into_iter()
                    .chain(to_target)
                    .min()
                    .and_then(|dt| self.now_us.checked_add(dt))
            })
            .min()
    }
    pub fn input_edge(m: &mut Memory, input: Device, old: bool, new: bool, at_us: u64) {
        if input.area != Area::X || old == new {
            return;
        }
        Self::advance(m, at_us);
        if !m.pulse_output.running {
            return;
        }
        let mut s = std::mem::take(&mut m.pulse_output);
        let ids: Vec<_> = s.runs.keys().copied().collect();
        for id in ids {
            let mut r = s.runs.remove(&id).unwrap();
            if r.active && !r.done && r.kind == 2 && !r.mark_stop {
                let a = mark(r.y, matches!(s.model, PulseModel::Ec5 | PulseModel::Se2));
                if r.mark_input == Some(input) && m.system_bit(a.enable) {
                    if !s.mark_supported || s.model == PulseModel::Ss2 && r.y % 2 == 1 {
                        lim(
                            m,
                            "İşaret yakalama seçilen CPU/firmware kanalında desteklenmiyor",
                        );
                    } else {
                        let edge_mask = m.interrupts.reset_capture_edges()[a.input as usize];
                        let selected = if edge_mask == 0 {
                            new
                        } else {
                            (edge_mask & (if new { 1 } else { 2 })) != 0
                        };
                        let front = m.double_word(dev(Area::D, a.front));
                        let back = m.double_word(dev(Area::D, a.back));
                        let after = m.double_word(dev(Area::D, a.after));
                        let masked = if front < 0 {
                            if s.model == PulseModel::Ss2 {
                                lim(m, "SS2 negatif bölüm maskesi desteklemiyor");
                                true
                            } else {
                                match front {
                                    -1 => r.segment.phase == MotionPhase::RampUp,
                                    -2 => matches!(
                                        r.segment.phase,
                                        MotionPhase::RampUp | MotionPhase::Cruise
                                    ),
                                    -3 => true,
                                    _ => {
                                        lim(m, "İşaret ön maskesi negatif değeri belgelenmedi");
                                        true
                                    }
                                }
                            }
                        } else {
                            front > 0 && r.emitted <= front as u64
                        };
                        if selected && !masked && !(back > 0 && r.emitted >= back as u64) {
                            if after < 0 && s.model == PulseModel::Ss2 {
                                lim(m, "SS2 negatif işaret sonrası duruş sayısı desteklemiyor");
                            } else if after != 0 || s.zero_mark == ZeroMark::NormalRampDown {
                                r.mark_stop = true;
                                m.set_system(port(r.y).unwrap().ramp_pause, true);
                                if after < 0 {
                                    r.ramp_paused = true;
                                    m.set_system(port(r.y).unwrap().status, true);
                                    s.levels(m, &r, false);
                                } else {
                                    s.ramp_stop(
                                        &mut r,
                                        if after == 0 { None } else { Some(after as u64) },
                                    );
                                }
                            }
                        }
                    }
                }
            }
            s.runs.insert(id, r);
        }
        m.pulse_output = s;
    }
}

#[cfg(test)]
#[path = "pulse_output_tests.rs"]
mod tests;

#[cfg(test)]
mod axis_owner_tests {
    use super::*;
    use crate::refresh_io::RefreshAction;

    fn request(pc: u16, channel: u8, direction: Device) -> AxisRequest {
        AxisRequest {
            owner_pc: pc,
            channel,
            direction,
            negative: false,
            invert_direction: false,
            target_pulses: None,
            segments: VecDeque::from([PulseSegment {
                duration_us: 0,
                frequency_millihz: 100_000,
                phase: MotionPhase::Cruise,
            }]),
            completion: false,
            paused: false,
        }
    }
    fn pulse(m: &mut Memory, pc: u16, y: &str) {
        PulseAction::parse("PLSY", &["K100".into(), "K100".into(), y.into()])
            .unwrap()
            .execute(m, true, pc);
    }
    #[test]
    fn public_axis_direction_rejects_readonly_sparse_and_model_end_before_writes() {
        for direction in [
            dev(Area::M, 1000),
            dev(Area::M, 1011),
            dev(Area::M, 1114),
            dev(Area::M, 4096),
            dev(Area::S, 1024),
            dev(Area::X, 0),
            dev(Area::Y, 255),
        ] {
            let mut m = Memory::default();
            assert!(
                PulseState::submit_axis(&mut m, request(11, 0, direction), 0).is_err(),
                "{direction}"
            );
            assert!(m.pulse_output.claimed_outputs().is_empty());
            assert!(!m.bit(dev(Area::Y, 0)));
        }
        let mut m = Memory::default();
        m.relay_limit = Some(31);
        assert!(PulseState::submit_axis(&mut m, request(11, 0, dev(Area::M, 32)), 0).is_err());
        assert!(PulseState::submit_axis(&mut m, request(11, 0, dev(Area::M, 31)), 0).is_ok());
    }
    #[test]
    fn public_axis_channel_and_y_direction_respect_model_and_configured_pins() {
        let mut m = Memory::default();
        m.pulse_output.model = PulseModel::Ss2;
        assert!(PulseState::submit_axis(&mut m, request(11, 1, dev(Area::Y, 5)), 0).is_err());
        assert!(PulseState::submit_axis(&mut m, request(11, 0, dev(Area::Y, 4)), 0).is_err());
        m.pulse_output.model = PulseModel::Es2;
        m.refresh_io.profile.output_points = 4;
        assert!(PulseState::submit_axis(&mut m, request(11, 0, dev(Area::Y, 4)), 0).is_err());
        assert!(PulseState::submit_axis(&mut m, request(11, 0, dev(Area::Y, 1)), 0).is_ok());
    }
    #[test]
    fn axis_cannot_steal_mtr_strobe_or_y_result_even_without_timing_claim_entry() {
        for (strobe, dest) in [
            (dev(Area::Y, 0), dev(Area::M, 0)),
            (dev(Area::Y, 16), dev(Area::Y, 0)),
        ] {
            let mut m = Memory::default();
            m.refresh_io.set_matrix_rows(7, vec![0, 0]).unwrap();
            RefreshAction::Matrix {
                source: dev(Area::X, 32),
                strobe,
                dest,
                rows: 2,
            }
            .execute(&mut m, true, 7);
            assert_eq!(m.refresh_io.matrix_owner(), Some(7));
            let before = m.physical_output(strobe);
            assert!(PulseState::submit_axis(&mut m, request(11, 0, dev(Area::Y, 1)), 0).is_err());
            assert_eq!(m.refresh_io.matrix_owner(), Some(7));
            assert_eq!(m.physical_output(strobe), before);
            assert!(m.pulse_output.claimed_outputs().is_empty());
        }
    }
    #[test]
    fn active_axis_keeps_its_pulse_and_direction_from_legacy_commands_across_scans() {
        for order in [OutputOrder::FirstVisitedWins, OutputOrder::LastVisitedWins] {
            for target in ["Y0", "Y1"] {
                let mut m = Memory::default();
                m.pulse_output.output_order = order;
                PulseState::submit_axis(&mut m, request(11, 0, dev(Area::Y, 1)), 0).unwrap();
                PulseState::begin_scan(&mut m); // clears legacy winners, not motion ownership
                pulse(&mut m, 22, target);
                assert_eq!(m.pulse_output.axis_owner(0), Some(11));
                assert!(m.pulse_output.axis_snapshot(22).is_none());
                assert_eq!(
                    m.pulse_output.claimed_outputs()[&dev(Area::Y, 1)],
                    ResourceOwner::External("PULSE:11".into())
                );
                PulseState::advance(&mut m, 100_000);
                assert_eq!(m.pulse_output.positions[&0], 10);
                assert!(!m.bit(dev(Area::Y, 1)));
                assert!(m.execution_limitations.is_empty());
            }
        }
    }
    #[test]
    fn legacy_last_visited_policy_still_replaces_another_legacy_pulse() {
        let mut m = Memory::default();
        m.pulse_output.output_order = OutputOrder::LastVisitedWins;
        pulse(&mut m, 11, "Y0");
        pulse(&mut m, 22, "Y0");
        assert_eq!(m.pulse_output.axis_owner(0), Some(22));
        assert!(m.execution_limitations.is_empty());
    }
    #[test]
    fn done_axis_keeps_direction_pin_until_explicit_release() {
        let mut m = Memory::default();
        let mut q = request(11, 0, dev(Area::Y, 1));
        q.target_pulses = Some(1);
        q.negative = true;
        PulseState::submit_axis(&mut m, q, 0).unwrap();
        PulseState::advance(&mut m, 10_000);
        m.timing_io.now_us = 10_000;
        assert!(m.pulse_output.axis_snapshot(11).unwrap().done);
        PulseState::begin_scan(&mut m);
        pulse(&mut m, 22, "Y1");
        assert!(m.pulse_output.axis_snapshot(22).is_none());
        assert!(m.bit(dev(Area::Y, 1)));
        PulseState::release_axis(&mut m, 11, true, 10_000);
        pulse(&mut m, 22, "Y1");
        assert_eq!(m.pulse_output.axis_owner(1), Some(22));
    }
}
