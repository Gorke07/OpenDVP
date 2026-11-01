//! Calibrated virtual servo trajectories and shared two-axis pulse interpolation.
//! Candidate timing/rounding policies are explicit; no physical CAN/servo parity.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::canopen::{CanCpu, CanServoModel};
use crate::plc::{Area, Device, Memory, Operand};
use crate::pulse_output::{AxisRequest, MotionPhase, PulseModel, PulseSegment, PulseState};
use crate::wide::WideOperand;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[path = "servo_motion.rs"]
mod servo_next;
pub use servo_next::{EmergencyMethod, HomeEvent, ServoNextAction};
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn relay(index: u16) -> Device {
    Device {
        area: Area::M,
        index,
    }
}
fn span(m: &Memory, base: Device, n: u16) -> Result<(), String> {
    if base.area != Area::D
        || base
            .index
            .checked_add(n - 1)
            .is_none_or(|last| last > m.data_register_limit.unwrap_or(9999))
    {
        Err("Eşgüdümlü hareket tam D aralığı model sınırını aşıyor".into())
    } else {
        Ok(())
    }
}
fn value(m: &Memory, v: &CalendarValue) -> Result<i32, String> {
    let mut set = BTreeSet::new();
    v.devices(&mut set);
    for x in set {
        span(m, x, 1)?;
    }
    Ok(v.read(m))
}
#[derive(Debug, Clone, PartialEq)]
pub enum CoordinatedOperand {
    Value(CalendarValue),
    Device(Device),
}
#[derive(Debug, Clone, PartialEq)]
pub enum CoordinatedAction {
    ServoNext(ServoNextAction),
    Casd {
        node: i32,
        acceleration: CalendarValue,
        deceleration: CalendarValue,
    },
    Servo {
        node: i32,
        target: CalendarValue,
        speed: CalendarValue,
        absolute: bool,
    },
    Planar {
        x: CalendarValue,
        y: CalendarValue,
        setting: CalendarValue,
        output: Device,
        absolute: bool,
        arc: bool,
    },
}
impl CoordinatedAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if matches!(
            name,
            "PLSVC" | "DPLSVC" | "ZRNC" | "DZRNC" | "ZRNM" | "EMER"
        ) {
            return ServoNextAction::parse(name, args).map(Self::ServoNext);
        }
        let planar = matches!(name, "DPPMR" | "DPPMA" | "DCIMR" | "DCIMA");
        let wide = name != "CASD";
        if !matches!(
            name,
            "CASD" | "DDRVIC" | "DDRVAC" | "DPPMR" | "DPPMA" | "DCIMR" | "DCIMA"
        ) || args.len() != if planar { 4 } else { 3 }
        {
            return Err("CASD/DDRVIC/DDRVAC üç, DPPMR/DPPMA/DCIMR/DCIMA dört operand gerektirir; P biçimi yok".into());
        }
        let scalar = |i: usize| -> Result<CalendarValue, String> {
            if !args[i].starts_with(['K', 'H', 'D']) {
                return Err("Hareket sayısal operandı yalnız K/H/D olmalı".into());
            }
            let v = CalendarValue::parse(&args[i], wide)?;
            let valid = match &v {
                CalendarValue::Additional(AdditionalValue::Word(
                    Operand::Constant(_) | Operand::Register(Device { area: Area::D, .. }),
                )) => !wide,
                CalendarValue::Additional(AdditionalValue::Double(
                    WideOperand::Constant(_) | WideOperand::Register(Device { area: Area::D, .. }),
                )) => wide,
                _ => false,
            };
            if !valid {
                return Err("Hareket operandı skaler K/H/D olmalı".into());
            }
            Ok(v)
        };
        if planar {
            let output: Device = args[3].parse()?;
            if output.area != Area::Y {
                return Err("İki eksen çıkışı Y olmalı".into());
            }
            let arc = name.starts_with("DCI");
            let setting = scalar(2)?;
            if arc
                && !matches!(
                    &setting,
                    CalendarValue::Additional(AdditionalValue::Double(WideOperand::Register(
                        Device { area: Area::D, .. }
                    )))
                )
            {
                return Err("DCIMR/DCIMA ayar çifti yalnız D olmalı".into());
            }
            Ok(Self::Planar {
                x: scalar(0)?,
                y: scalar(1)?,
                setting,
                output,
                absolute: matches!(name, "DPPMA" | "DCIMA"),
                arc,
            })
        } else {
            let n = scalar(0)?;
            let node = match n {
                CalendarValue::Additional(AdditionalValue::Word(Operand::Constant(v))) => {
                    i32::from(v)
                }
                CalendarValue::Additional(AdditionalValue::Double(WideOperand::Constant(v))) => v,
                _ => return Err("CAN servo node yalnız K/H sabit olmalı".into()),
            };
            if name == "CASD" {
                Ok(Self::Casd {
                    node,
                    acceleration: scalar(1)?,
                    deceleration: scalar(2)?,
                })
            } else {
                Ok(Self::Servo {
                    node,
                    target: scalar(1)?,
                    speed: scalar(2)?,
                    absolute: name == "DDRVAC",
                })
            }
        }
    }
    pub fn name(&self) -> &'static str {
        if let Self::ServoNext(a) = self {
            return a.name();
        }
        match self {
            Self::ServoNext(_) => unreachable!(),
            Self::Casd { .. } => "CASD",
            Self::Servo {
                absolute: false, ..
            } => "DDRVIC",
            Self::Servo { .. } => "DDRVAC",
            Self::Planar {
                absolute: false,
                arc: false,
                ..
            } => "DPPMR",
            Self::Planar {
                absolute: true,
                arc: false,
                ..
            } => "DPPMA",
            Self::Planar {
                absolute: false,
                arc: true,
                ..
            } => "DCIMR",
            Self::Planar { .. } => "DCIMA",
        }
    }
    pub fn header(&self) -> Option<u16> {
        if let Self::ServoNext(a) = self {
            return Some(a.header());
        }
        Some(match self.name() {
            "CASD" => 0xb202,
            "DDRVIC" => 0xb502,
            "DDRVAC" => 0xb702,
            "DPPMR" => 0x9d01,
            "DPPMA" => 0x9f01,
            "DCIMR" => 0xa101,
            _ => 0xa301,
        })
    }
    pub fn words(&self) -> u16 {
        if let Self::ServoNext(a) = self {
            return a.words();
        }
        match self {
            Self::ServoNext(_) => unreachable!(),
            Self::Casd { .. } => 7,
            Self::Servo { .. } => 13,
            Self::Planar { .. } => 17,
        }
    }
    pub fn wide(&self) -> bool {
        if let Self::ServoNext(a) = self {
            return a.wide();
        }
        !matches!(self, Self::Casd { .. })
    }
    pub fn dest(&self) -> Device {
        if let Self::ServoNext(a) = self {
            return a.dest();
        }
        match self {
            Self::ServoNext(_) => unreachable!(),
            Self::Casd { .. } => relay(1615),
            Self::Servo { node, .. } => relay(1623 + (*node).clamp(1, 8) as u16),
            Self::Planar { output, .. } => *output,
        }
    }
    pub fn operands(&self) -> Vec<CoordinatedOperand> {
        if let Self::ServoNext(a) = self {
            return a.operands();
        }
        let v = |x: &CalendarValue| CoordinatedOperand::Value(x.clone());
        let n = |node: i32| {
            CoordinatedOperand::Value(
                CalendarValue::parse(&format!("K{node}"), self.wide()).unwrap(),
            )
        };
        match self {
            Self::ServoNext(_) => unreachable!(),
            Self::Casd {
                node,
                acceleration,
                deceleration,
            } => vec![n(*node), v(acceleration), v(deceleration)],
            Self::Servo {
                node,
                target,
                speed,
                ..
            } => vec![n(*node), v(target), v(speed)],
            Self::Planar {
                x,
                y,
                setting,
                output,
                ..
            } => vec![v(x), v(y), v(setting), CoordinatedOperand::Device(*output)],
        }
    }
    pub fn label(&self) -> String {
        format!(
            "{} {}",
            self.name(),
            self.operands()
                .iter()
                .map(|o| match o {
                    CoordinatedOperand::Value(v) => v.to_string(),
                    CoordinatedOperand::Device(d) => d.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
    pub fn display_value(&self, m: &Memory) -> String {
        if let Self::ServoNext(a) = self {
            return a.display_value(m);
        }
        match self {
            Self::ServoNext(_) => unreachable!(),
            Self::Planar { output, .. } => format!(
                "X={} Y={}",
                m.pulse_output.axis_position(output.index as u8, m),
                m.pulse_output.axis_position(output.index as u8 + 2, m)
            ),
            Self::Casd { .. } => format!("M1615={}", m.system_bit(1615)),
            Self::Servo { node, .. } => format!(
                "PUU={}",
                m.double_word(d(6032 + 2 * ((*node).clamp(1, 8) as u16 - 1)))
            ),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        if let Self::ServoNext(a) = self {
            a.devices(out);
            return;
        }
        for o in self.operands() {
            match o {
                CoordinatedOperand::Value(v) => v.devices(out),
                CoordinatedOperand::Device(d) => {
                    out.insert(d);
                }
            }
        }
        match self {
            Self::ServoNext(_) => unreachable!(),
            Self::Planar { output, .. } => {
                out.extend(
                    (output.index..output.index.saturating_add(4)).map(|index| Device {
                        area: Area::Y,
                        index,
                    }),
                );
                out.insert(relay(if output.index == 4 { 1321 } else { 1029 }));
                let bases = if output.index == 4 {
                    [1375, 1377]
                } else {
                    [1030, 1336]
                };
                for base in bases {
                    out.extend([d(base), d(base + 1)]);
                }
                out.extend([1340, 1343, 1379, 1381].map(d));
            }
            Self::Casd { .. } | Self::Servo { .. } => {
                out.extend([1614, 1615, 1616].map(relay));
                out.extend([6000, 6001, 6002].map(d));
                if let Self::Servo { node, .. } = self {
                    let i = (*node).clamp(1, 8) as u16 - 1;
                    out.extend([1624 + i, 1632 + i, 1640 + i, 1648 + i, 1656 + i].map(relay));
                    out.extend((6032 + 2 * i..=6033 + 2 * i).map(d));
                    out.extend((6048 + 2 * i..=6049 + 2 * i).map(d));
                }
            }
        }
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let now = m.timing_io.now_us;
        if now != m.coordinated_motion.now_us {
            CoordinatedState::advance(m, now);
        }
        let mut state = std::mem::take(&mut m.coordinated_motion);
        state.visit(m, self, power, pc);
        m.coordinated_motion = state;
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ServoCalibration {
    Rotary {
        puu_per_revolution: f64,
    },
    Linear {
        puu_per_meter: f64,
        reference_speed_mps: f64,
    },
}
impl ServoCalibration {
    fn valid(self) -> bool {
        match self {
            Self::Rotary {
                puu_per_revolution: v,
            } => v.is_finite() && v > 0.,
            Self::Linear {
                puu_per_meter: v,
                reference_speed_mps: r,
            } => v.is_finite() && v > 0. && r.is_finite() && r > 0.,
        }
    }
    fn rate(self, speed: i32) -> f64 {
        match self {
            Self::Rotary {
                puu_per_revolution: v,
            } => f64::from(speed) * v / 600.,
            Self::Linear {
                puu_per_meter: v, ..
            } => f64::from(speed) * v / 1_000_000.,
        }
    }
    fn reference(self) -> f64 {
        match self {
            Self::Rotary {
                puu_per_revolution: v,
            } => 50. * v,
            Self::Linear {
                puu_per_meter: v,
                reference_speed_mps: r,
            } => v * r,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum ServoIntent {
    Speed {
        speed: i32,
    },
    Homing {
        first: i32,
        second: i32,
        wide: bool,
    },
    HomingMode {
        mode: u16,
    },
    Ramp {
        acceleration_ms: u16,
        deceleration_ms: u16,
    },
    Move {
        target: i32,
        speed: i32,
        absolute: bool,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct ServoRequest {
    pub id: u64,
    pub epoch: u64,
    pub pc: u16,
    pub node: u8,
    pub at_us: u64,
    pub intent: ServoIntent,
}
#[derive(Debug, Clone, PartialEq)]
pub enum ServoReply {
    Accepted,
    Fault { code: u16 },
}
#[derive(Debug, Clone)]
struct Plant {
    calibration: Option<ServoCalibration>,
    position: i32,
    acceleration_ms: Option<u16>,
    deceleration_ms: Option<u16>,
}
#[derive(Debug, Clone, Copy)]
struct Trajectory {
    start: f64,
    sign: f64,
    distance: f64,
    peak: f64,
    acceleration: f64,
    deceleration: f64,
    up: f64,
    coast: f64,
    down: f64,
    at_us: u64,
}
impl Trajectory {
    fn new(start: f64, target: f64, rate: f64, a: f64, b: f64, at_us: u64) -> Result<Self, String> {
        if ![start, target, rate, a, b].iter().all(|v| v.is_finite())
            || rate <= 0.
            || a <= 0.
            || b <= 0.
        {
            return Err("Servo kalibrasyonu sonlu ve pozitif hız/ivme gerektirir".into());
        }
        let distance = (target - start).abs();
        let peak = rate.min((2. * distance / (1. / a + 1. / b)).sqrt());
        let up = peak / a;
        let down = peak / b;
        let coast = if peak > 0. {
            ((distance - peak * (up + down) / 2.) / peak).max(0.)
        } else {
            0.
        };
        Ok(Self {
            start,
            sign: if target < start { -1. } else { 1. },
            distance,
            peak,
            acceleration: a,
            deceleration: b,
            up,
            coast,
            down,
            at_us,
        })
    }
    fn duration(self) -> f64 {
        self.up + self.coast + self.down
    }
    fn sample(self, now: u64) -> (f64, f64, bool) {
        let t = now.saturating_sub(self.at_us) as f64 / 1e6;
        let (d, v) = if t < self.up {
            (0.5 * self.acceleration * t * t, self.acceleration * t)
        } else if t < self.up + self.coast {
            (
                self.peak * self.up / 2. + self.peak * (t - self.up),
                self.peak,
            )
        } else {
            let dt = (t - self.up - self.coast).clamp(0., self.down);
            (
                self.peak * self.up / 2. + self.peak * self.coast + self.peak * dt
                    - self.deceleration * dt * dt / 2.,
                (self.peak - self.deceleration * dt).max(0.),
            )
        };
        let done = t >= self.duration();
        (
            self.start
                + self.sign
                    * if done {
                        self.distance
                    } else {
                        d.min(self.distance)
                    },
            v,
            done,
        )
    }
    fn deadline(self) -> u64 {
        self.at_us
            .saturating_add((self.duration() * 1e6).ceil() as u64)
    }
}
#[derive(Debug, Clone)]
struct ServoRun {
    node: u8,
    path: Trajectory,
    done: bool,
    stopping: bool,
    absolute: bool,
    target: i32,
    staged_target: i32,
    speed: i32,
    toward_target: bool,
    power: bool,
}
#[derive(Debug, Clone)]
struct PulseRun {
    tokens: [u16; 2],
    channels: [u8; 2],
    done: bool,
}
#[derive(Debug, Clone)]
struct Pending {
    request: ServoRequest,
}
#[derive(Debug, Clone, Default)]
pub struct CoordinatedState {
    pub now_us: u64,
    pub notices: BTreeSet<String>,
    next: servo_next::ServoMotionState,
    plants: BTreeMap<u8, Plant>,
    servo_runs: BTreeMap<u16, ServoRun>,
    pulse_runs: BTreeMap<u16, PulseRun>,
    pending: Option<Pending>,
    completed: BTreeSet<u16>,
    sequence: u64,
    epoch: u64,
    replies: BTreeMap<(u64, u64), (ServoRequest, ServoReply)>,
    outgoing: VecDeque<ServoRequest>,
}
impl CoordinatedState {
    fn advance_next(&mut self, m: &mut Memory, at: u64) {
        let mut next = std::mem::take(&mut self.next);
        next.advance(self, m, at);
        self.next = next;
    }
    pub fn configure_homing(
        &mut self,
        node: u8,
        mode: u16,
        setting: u32,
        origin_definition: i32,
    ) -> Result<(), String> {
        self.next
            .configure_homing(node, mode, setting, origin_definition)
    }
    pub fn queue_home_event(&mut self, at: u64, node: u8, event: HomeEvent) -> Result<(), String> {
        self.next.queue_home_event(self.now_us, at, node, event)
    }
    pub fn queue_emergency(&mut self, at: u64, node: u8, data: [u8; 8]) -> Result<(), String> {
        self.next.queue_emergency(self.now_us, at, node, data)
    }
    pub fn set_emergency_method(&mut self, method: EmergencyMethod) -> Result<(), String> {
        self.next.set_emergency_method(method)
    }
    pub fn configure_servo(
        &mut self,
        node: u8,
        calibration: ServoCalibration,
        initial_position: i32,
    ) -> Result<(), String> {
        if !(1..=8).contains(&node)
            || !calibration.valid()
            || self.pending.is_some()
            || self.servo_runs.values().any(|r| r.node == node && !r.done)
            || self.next.node_busy(node)
        {
            return Err("Servo açık1..8node, sonlu kalibrasyon ve boş hareket gerektirir".into());
        }
        let previous = self.plants.get(&node);
        let acceleration_ms = previous.and_then(|p| p.acceleration_ms);
        let deceleration_ms = previous.and_then(|p| p.deceleration_ms);
        self.plants.insert(
            node,
            Plant {
                calibration: Some(calibration),
                position: initial_position,
                acceleration_ms,
                deceleration_ms,
            },
        );
        Ok(())
    }
    pub fn servo_position(&self, node: u8) -> Option<i32> {
        self.plants
            .get(&node)
            .and_then(|p| p.calibration.map(|_| p.position))
    }
    pub fn outgoing_requests(&self) -> &VecDeque<ServoRequest> {
        &self.outgoing
    }
    pub fn pending(&self) -> Option<&ServoRequest> {
        self.pending.as_ref().map(|p| &p.request)
    }
    pub fn queue_reply(
        &mut self,
        at_us: u64,
        request: ServoRequest,
        reply: ServoReply,
    ) -> Result<(), String> {
        if at_us < self.now_us || self.replies.len() >= 4096 {
            return Err("Servo yanıtı gelecek zaman/4096olay sınırını aşamaz".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Servo sıra sayacı taştı")?;
        self.replies
            .insert((at_us, self.sequence), (request, reply));
        Ok(())
    }
    pub fn reset_instruction_state(&mut self) {
        self.pending = None;
        self.next.reset_instruction_state();
        self.servo_runs.clear();
        self.pulse_runs.clear();
        self.completed.clear();
        self.replies.clear();
        self.outgoing.clear();
        self.now_us = 0;
        self.epoch = self.epoch.wrapping_add(1);
    }
    pub fn restart_cpu_clock(&mut self, elapsed: u64) -> Result<(), String> {
        if elapsed < self.now_us {
            return Err("Eşgüdümlü reset saati geriye gidemez".into());
        }
        self.reset_instruction_state();
        Ok(())
    }
    pub fn abort(m: &mut Memory) {
        let mut state = std::mem::take(&mut m.coordinated_motion);
        let now = state.now_us;
        if let Some(p) = &state.pending {
            m.canopen.release_exchange(p.request.pc);
        }
        for r in state.pulse_runs.values() {
            for token in r.tokens {
                PulseState::release_axis(m, token, true, now);
            }
        }
        let mut next = std::mem::take(&mut state.next);
        next.abort();
        state.reset_instruction_state();
        state.next = next;
        state.now_us = now;
        m.coordinated_motion = state;
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.replies
            .first_key_value()
            .map(|(k, _)| k.0)
            .into_iter()
            .chain(
                self.servo_runs
                    .values()
                    .filter(|r| !r.done)
                    .map(|r| r.path.deadline()),
            )
            .chain(self.next.next_deadline_us())
            .min()
    }
    pub fn advance(m: &mut Memory, at: u64) {
        let mut s = std::mem::take(&mut m.coordinated_motion);
        if at < s.now_us {
            m.execution_limitations
                .insert("Eşgüdümlü hareket saati geriye gidiyor".into());
            m.coordinated_motion = s;
            return;
        }
        let mut steps = 0;
        while let Some(t) = s.next_deadline_us() {
            if t > at {
                break;
            }
            steps += 1;
            if steps > 4096 {
                m.execution_limitations
                    .insert("Servo çevrim olay sınırı; daha küçük zaman adımı gerekli".into());
                break;
            }
            s.now_us = t;
            s.integrate(m, t);
            s.advance_next(m, t);
            let keys = s
                .replies
                .range(..=(t, u64::MAX))
                .map(|(k, _)| *k)
                .collect::<Vec<_>>();
            for key in keys {
                let (request, reply) = s.replies.remove(&key).unwrap();
                s.reply(m, request, reply);
            }
        }
        if steps <= 4096 {
            s.now_us = at;
            s.integrate(m, at);
            s.advance_next(m, at);
        }
        m.coordinated_motion = s;
    }
    pub fn service_controls(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.coordinated_motion);
        let mut next = std::mem::take(&mut s.next);
        next.service_controls(&mut s, m);
        s.next = next;
        for r in s.pulse_runs.values_mut() {
            if !r.done
                && r.tokens
                    .iter()
                    .all(|pc| m.pulse_output.axis_snapshot(*pc).is_some_and(|v| v.done))
            {
                r.done = true;
                m.set_system(if r.channels[0] == 4 { 1321 } else { 1029 }, true);
            }
        }
        for run in s.servo_runs.values_mut() {
            if run.done {
                continue;
            }
            if !m
                .canopen
                .node(run.node)
                .is_some_and(|n| n.servo_enabled && n.online)
            {
                run.done = true;
                continue;
            }
            if m.system_bit(1631 + u16::from(run.node)) && !run.stopping {
                let (position, speed, _) = run.path.sample(s.now_us);
                let stopping_distance = speed * speed / (2. * run.path.deceleration);
                run.path = Trajectory {
                    start: position,
                    sign: run.path.sign,
                    distance: stopping_distance,
                    peak: speed,
                    acceleration: run.path.acceleration,
                    deceleration: run.path.deceleration,
                    up: 0.,
                    coast: 0.,
                    down: speed / run.path.deceleration,
                    at_us: s.now_us,
                };
                run.stopping = true;
            }
        }
        m.coordinated_motion = s;
    }
    fn visit(&mut self, m: &mut Memory, a: &CoordinatedAction, power: bool, pc: u16) {
        if let CoordinatedAction::ServoNext(action) = a {
            let mut next = std::mem::take(&mut self.next);
            next.visit(self, m, action, power, pc);
            self.next = next;
            return;
        }
        if !power {
            self.completed.remove(&pc);
            if let Some(r) = self.pulse_runs.remove(&pc) {
                for token in r.tokens {
                    PulseState::release_axis(m, token, true, self.now_us);
                }
            }
            if let Some(r) = self.servo_runs.remove(&pc)
                && let Some(p) = self.plants.get_mut(&r.node)
            {
                p.position = r.path.sample(self.now_us).0.round() as i64 as i32;
            }
            return;
        }
        if let Some(r) = self.pulse_runs.get(&pc) {
            for token in r.tokens {
                PulseState::axis_publish(m, token);
            }
            return;
        }
        if let Some(r) = self.servo_runs.get_mut(&pc) {
            if let CoordinatedAction::Servo { target, .. } = a
                && let Ok(v) = value(m, target)
            {
                r.staged_target = v;
            }
            return;
        }
        if self.completed.contains(&pc) || self.pending.as_ref().is_some_and(|p| p.request.pc == pc)
        {
            return;
        }
        let result = match a {
            CoordinatedAction::Planar { .. } => self.start_planar(m, a, pc),
            _ => self.start_servo(m, a, pc),
        };
        if let Err(e) = result {
            m.execution_limitations
                .insert(format!("{} STEP{pc}: {e}", a.name()));
        }
    }
    fn start_servo(
        &mut self,
        m: &mut Memory,
        a: &CoordinatedAction,
        pc: u16,
    ) -> Result<(), String> {
        let node = match a {
            CoordinatedAction::Casd { node, .. } | CoordinatedAction::Servo { node, .. } => *node,
            _ => unreachable!(),
        };
        if !(1..=8).contains(&node) {
            return Ok(());
        }
        if self.pending.is_some()
            || self
                .servo_runs
                .values()
                .any(|r| i32::from(r.node) == node && !r.done)
        {
            return Ok(());
        }
        let id = node as u8;
        if self.next.node_busy(id) {
            return Ok(());
        }
        let profile = m
            .canopen
            .profile()
            .ok_or("Açık ES2-C/SE2 profili gerekli")?;
        let peer = m
            .canopen
            .node(id)
            .ok_or("Servo node ortamı yapılandırılmamış")?;
        if !m.system_bit(1614) || !m.system_bit(1615) || !peer.online || !peer.initialized {
            return Err("CAN servo gerçek INITC tamamlanması gerektirir".into());
        }
        span(m, d(6000), 3)?;
        let intent = match a {
            CoordinatedAction::Casd {
                acceleration,
                deceleration,
                ..
            } => {
                let clamp = |v: i32| {
                    if (1..=32767).contains(&v) {
                        v as u16
                    } else {
                        1
                    }
                };
                ServoIntent::Ramp {
                    acceleration_ms: clamp(value(m, acceleration)?),
                    deceleration_ms: clamp(value(m, deceleration)?),
                }
            }
            CoordinatedAction::Servo {
                target,
                speed,
                absolute,
                ..
            } => {
                if !peer.servo_enabled {
                    return Err("DDRVIC/DDRVAC gerçek ASDON ServoON gerektirir".into());
                }
                let plant = self.plants.get(&id).ok_or(
                    "PUU/rev veya PUU/m fiziksel birim kalibrasyonu açıkça yapılandırılmalı",
                )?;
                if plant.acceleration_ms.is_none() || plant.deceleration_ms.is_none() {
                    return Err("Servo hareketinden önce CASD ACK gerekli".into());
                }
                let modern = profile.cpu == CanCpu::Se2
                    || (profile.firmware_major, profile.firmware_minor) >= (3, 60);
                if modern && profile.servo == CanServoModel::A2M {
                    return Err("Seçili PDF modern DDRVIC/DDRVAC yalnız A3-M/B3-M verir; A2-M eşitliği doğrulanmadı".into());
                }
                let rate = value(m, speed)?;
                let max = match plant
                    .calibration
                    .ok_or("Servo PUU kalibrasyonu yapılandırılmamış")?
                {
                    ServoCalibration::Rotary { .. } => 60000,
                    ServoCalibration::Linear { .. } => {
                        if !modern {
                            return Err("Lineer motor eski A2-M profilinde belgelenmiyor".into());
                        }
                        15999999
                    }
                };
                if !(1..=max).contains(&rate) {
                    return Err("Servo hız değeri motor tipinin belgeli aralığı dışında".into());
                }
                span(m, d(6032 + 2 * u16::from(id - 1)), 2)?;
                span(m, d(6048 + 2 * u16::from(id - 1)), 2)?;
                ServoIntent::Move {
                    target: value(m, target)?,
                    speed: rate,
                    absolute: *absolute,
                }
            }
            _ => unreachable!(),
        };
        if self.sequence == u64::MAX {
            return Err("Servo request sayacı taştı".into());
        }
        if !m.canopen.reserve_exchange(0, pc) {
            return Ok(());
        }
        self.sequence += 1;
        let request = ServoRequest {
            id: self.sequence,
            epoch: self.epoch,
            pc,
            node: id,
            at_us: self.now_us,
            intent,
        };
        if matches!(request.intent, ServoIntent::Ramp { .. }) {
            m.set_system(1615, false);
        }
        if self.outgoing.len() >= 4096 {
            self.outgoing.pop_front();
        }
        self.outgoing.push_back(request.clone());
        self.pending = Some(Pending { request });
        self.notices.insert("CAN servo calibrated virtual plant: explicit ACK, PUU calibration; no physical motor/SDO/PDO equivalence".into());
        Ok(())
    }
    fn reply(&mut self, m: &mut Memory, request: ServoRequest, reply: ServoReply) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if pending.request != request {
            self.pending = Some(pending);
            self.notices.insert("Başka/eski servo ACK atıldı".into());
            return;
        }
        m.canopen.release_exchange(request.pc);
        if matches!(
            &request.intent,
            ServoIntent::Speed { .. } | ServoIntent::Homing { .. } | ServoIntent::HomingMode { .. }
        ) {
            let mut next = std::mem::take(&mut self.next);
            next.reply(self, m, request, reply);
            self.next = next;
            return;
        }
        if let ServoReply::Fault { code } = reply {
            m.set_system(1616, true);
            m.set_word(d(6000), i16::from(request.node));
            m.set_word(d(6001), code as i16);
            m.set_word(d(6002), request.pc as i16);
            self.completed.insert(request.pc);
            return;
        }
        match request.intent {
            ServoIntent::Speed { .. }
            | ServoIntent::Homing { .. }
            | ServoIntent::HomingMode { .. } => unreachable!(),
            ServoIntent::Ramp {
                acceleration_ms,
                deceleration_ms,
            } => {
                let plant = self.plants.entry(request.node).or_insert(Plant {
                    calibration: None,
                    position: 0,
                    acceleration_ms: None,
                    deceleration_ms: None,
                });
                plant.acceleration_ms = Some(acceleration_ms);
                plant.deceleration_ms = Some(deceleration_ms);
                m.set_system(1615, true);
                self.completed.insert(request.pc);
            }
            ServoIntent::Move {
                target,
                speed,
                absolute,
            } => {
                let Some(plant) = self.plants.get(&request.node) else {
                    return;
                };
                let start = f64::from(plant.position);
                let end = if absolute {
                    f64::from(target)
                } else {
                    start + f64::from(target)
                };
                let a = plant
                    .calibration
                    .expect("configured servo calibration")
                    .reference()
                    * 1000.
                    / f64::from(plant.acceleration_ms.unwrap());
                let b = plant
                    .calibration
                    .expect("configured servo calibration")
                    .reference()
                    * 1000.
                    / f64::from(plant.deceleration_ms.unwrap());
                match Trajectory::new(
                    start,
                    end,
                    plant
                        .calibration
                        .expect("configured servo calibration")
                        .rate(speed),
                    a,
                    b,
                    self.now_us,
                ) {
                    Ok(path) => {
                        m.set_system(1623 + u16::from(request.node), false);
                        m.set_double_word_raw(
                            d(6048 + 2 * u16::from(request.node - 1)),
                            end as i64 as i32,
                        );
                        m.set_system(1655 + u16::from(request.node), true);
                        self.servo_runs.insert(
                            request.pc,
                            ServoRun {
                                node: request.node,
                                path,
                                done: false,
                                stopping: false,
                                absolute,
                                target,
                                staged_target: target,
                                speed,
                                toward_target: true,
                                power: true,
                            },
                        );
                    }
                    Err(e) => {
                        m.execution_limitations.insert(e);
                    }
                }
            }
        }
    }
    fn integrate(&mut self, m: &mut Memory, at: u64) {
        for run in self.servo_runs.values_mut() {
            if run.done {
                continue;
            }
            let (position, _, done) = run.path.sample(at);
            if let Some(plant) = self.plants.get_mut(&run.node) {
                plant.position = position.round() as i64 as i32;
                m.set_double_word_raw(d(6032 + 2 * u16::from(run.node - 1)), plant.position);
                if done {
                    if !run.stopping {
                        m.set_system(1623 + u16::from(run.node), true);
                    }
                    let repeat = run.absolute
                        && !run.stopping
                        && run.power
                        && m.system_bit(1647 + u16::from(run.node));
                    if repeat {
                        run.toward_target = !run.toward_target;
                        if run.toward_target {
                            run.target = run.staged_target;
                        }
                        let target = if run.toward_target { run.target } else { 0 };
                        let a = plant
                            .calibration
                            .expect("configured servo calibration")
                            .reference()
                            * 1000.
                            / f64::from(plant.acceleration_ms.unwrap());
                        let b = plant
                            .calibration
                            .expect("configured servo calibration")
                            .reference()
                            * 1000.
                            / f64::from(plant.deceleration_ms.unwrap());
                        match Trajectory::new(
                            position,
                            f64::from(target),
                            plant
                                .calibration
                                .expect("configured servo calibration")
                                .rate(run.speed),
                            a,
                            b,
                            run.path.deadline(),
                        ) {
                            Ok(next) if next.duration() > 0. => {
                                run.path = next;
                                m.set_system(1655 + u16::from(run.node), run.toward_target);
                                m.set_double_word_raw(
                                    d(6048 + 2 * u16::from(run.node - 1)),
                                    target,
                                );
                            }
                            _ => run.done = true,
                        }
                    } else {
                        run.done = true;
                    }
                }
            }
        }
    }
    fn start_planar(
        &mut self,
        m: &mut Memory,
        a: &CoordinatedAction,
        pc: u16,
    ) -> Result<(), String> {
        let CoordinatedAction::Planar {
            x,
            y,
            setting,
            output,
            absolute,
            arc,
        } = a
        else {
            unreachable!()
        };
        if pc >= 15872 {
            return Err("İki eksen yerel owner tokenı için gerçek PC<15872 gerekli".into());
        }
        if output.index != 0 && !(output.index == 4 && m.pulse_output.model == PulseModel::Se2) {
            return Err("İki eksen grubu Y0 veya SE2'deY4 olmalı".into());
        }
        if m.pulse_output.model == PulseModel::Ec5 {
            return Err("Seçili PDF EC5 iki eksen grubunu doğrulamıyor".into());
        }
        let base = output.index as u8;
        let starts = [
            m.pulse_output.axis_position(base, m),
            m.pulse_output.axis_position(base + 2, m),
        ];
        let targets = [value(m, x)?, value(m, y)?];
        let delta = [
            if *absolute {
                i64::from(targets[0]) - i64::from(starts[0])
            } else {
                i64::from(targets[0])
            },
            if *absolute {
                i64::from(targets[1]) - i64::from(starts[1])
            } else {
                i64::from(targets[1])
            },
        ];
        let tokens = [0x8000 | (pc << 1), 0x8000 | (pc << 1) | 1];
        let cap = if m.pulse_output.model == PulseModel::Ss2 {
            10000
        } else {
            100000
        };
        let plans = if *arc {
            let config = value(m, setting)? as u32;
            let direction = (config & 0xffff) as u16;
            let units = config >> 16;
            if direction > 1 || units == 1 || units > 200 {
                return Err("CIM ayarı low0/1, high0(auto) veya2..200 olmalı".into());
            }
            if delta.iter().any(|v| v.unsigned_abs() > 10_000_000) {
                self.notices
                    .insert("DCIMR/DCIMA >10000000pulse için belgeli no-execution".into());
                return Ok(());
            }
            arc_segments(
                delta,
                direction == 0,
                if units == 0 {
                    200000
                } else {
                    u64::from(units) * 100000
                },
                cap,
            )
        } else {
            let peak = value(m, setting)?.clamp(100, 100000) as u64;
            let peak = peak.min(cap);
            let longest = delta.iter().map(|v| v.unsigned_abs()).max().unwrap();
            let small = delta.iter().any(|v| *v != 0 && v.unsigned_abs() < 20);
            let mut master = if small {
                VecDeque::from([PulseSegment {
                    duration_us: if longest == 0 {
                        1
                    } else {
                        (longest * 1_000_000).div_ceil(peak.min(3000))
                    },
                    frequency_millihz: peak.min(3000) * 1000,
                    phase: MotionPhase::Cruise,
                }])
            } else {
                PulseState::axis_step_plan(
                    m.word(d(if base == 4 { 1379 } else { 1340 })).max(6) as u64,
                    peak,
                    m.word(d(if base == 4 { 1381 } else { 1343 })).max(20) as u64 * 1000,
                    m.word(d(if base == 4 { 1381 } else { 1343 })).max(20) as u64 * 1000,
                    longest,
                    20,
                )
            };
            if master.is_empty() {
                master.push_back(PulseSegment {
                    duration_us: 1,
                    frequency_millihz: 0,
                    phase: MotionPhase::Cruise,
                });
            }
            std::array::from_fn(|i| {
                master
                    .iter()
                    .map(|s| PulseSegment {
                        duration_us: s.duration_us.max(1),
                        frequency_millihz: if longest == 0 {
                            0
                        } else {
                            (u128::from(s.frequency_millihz) * u128::from(delta[i].unsigned_abs()))
                                .div_ceil(u128::from(longest)) as u64
                        },
                        phase: s.phase,
                    })
                    .collect()
            })
        };
        let requests = std::array::from_fn::<_, 2, _>(|i| AxisRequest {
            owner_pc: tokens[i],
            channel: base + i as u8 * 2,
            direction: Device {
                area: Area::Y,
                index: u16::from(base) + i as u16 * 2 + 1,
            },
            negative: if delta[i] == 0 {
                m.bit(Device {
                    area: Area::Y,
                    index: u16::from(base) + i as u16 * 2 + 1,
                })
            } else {
                delta[i] < 0
            },
            invert_direction: false,
            target_pulses: Some(delta[i].unsigned_abs()),
            segments: plans[i].clone(),
            completion: false,
            paused: false,
        });
        let mut trial = m.clone();
        for request in &requests {
            PulseState::submit_axis(&mut trial, request.clone(), self.now_us)?;
        }
        for request in requests {
            PulseState::submit_axis(m, request, self.now_us)?;
        }
        m.set_system(if base == 4 { 1321 } else { 1029 }, false);
        self.pulse_runs.insert(
            pc,
            PulseRun {
                tokens,
                channels: [base, base + 2],
                done: false,
            },
        );
        self.notices.insert("İki eksen explicit shared-step DDA/20chord quarter-ellipse candidate; pulse millihertz/microsecond rounding, native/hardware interpolation equivalence açık".into());
        Ok(())
    }
}
/// Twenty geometrical quarter-ellipse chords; linear fallback for a short axis.
pub fn quarter_points(delta: [i64; 2], clockwise: bool) -> Vec<[i64; 2]> {
    let linear = delta.iter().any(|v| v.unsigned_abs() < 500);
    let vertical_first = clockwise == (delta[0].signum() == delta[1].signum());
    (1..=20)
        .map(|i| {
            if i == 20 {
                return delta;
            }
            let t = f64::from(i) / 20.;
            if linear {
                return [
                    (delta[0] as f64 * t).round() as i64,
                    (delta[1] as f64 * t).round() as i64,
                ];
            }
            let angle = t * std::f64::consts::FRAC_PI_2;
            let (a, b) = if vertical_first {
                (1. - angle.cos(), angle.sin())
            } else {
                (angle.sin(), 1. - angle.cos())
            };
            [
                (delta[0] as f64 * a).round() as i64,
                (delta[1] as f64 * b).round() as i64,
            ]
        })
        .collect()
}
fn arc_segments(
    delta: [i64; 2],
    clockwise: bool,
    time_us: u64,
    cap: u64,
) -> [VecDeque<PulseSegment>; 2] {
    let points = quarter_points(delta, clockwise);
    let mut output: [VecDeque<PulseSegment>; 2] = std::array::from_fn(|_| VecDeque::new());
    let mut old = [0, 0];
    for point in points {
        let counts = [
            (point[0] - old[0]).unsigned_abs(),
            (point[1] - old[1]).unsigned_abs(),
        ];
        let duration = (time_us.div_ceil(20))
            .max(
                counts
                    .iter()
                    .map(|v| (v * 1_000_000).div_ceil(cap))
                    .max()
                    .unwrap(),
            )
            .max(1);
        for i in 0..2 {
            output[i].push_back(PulseSegment {
                duration_us: duration,
                frequency_millihz: (u128::from(counts[i]) * 1_000_000_000u128)
                    .div_ceil(u128::from(duration)) as u64,
                phase: MotionPhase::Cruise,
            });
        }
        old = point;
    }
    output
}
#[cfg(test)]
#[path = "coordinated_motion_tests.rs"]
mod tests;
