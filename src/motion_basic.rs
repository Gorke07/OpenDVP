//! Documented virtual axis commands. Original installed host-clock handlers
//! are a separate capability; this module does not claim physical equivalence.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarIndices, CalendarValue};
use crate::plc::{Area, Device, Memory};
use crate::pulse_output::{
    AxisRequest, AxisRevision, MotionPhase, PulseModel, PulseSegment, PulseState,
};
use crate::timing_io::ResourceOwner;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn d(area: Area, index: u16) -> Device {
    Device { area, index }
}
fn limit(m: &mut Memory, s: impl Into<String>) {
    m.execution_limitations.insert(s.into());
}
#[derive(Debug, Clone, PartialEq)]
pub enum MotionOperand {
    Value(CalendarValue),
    Bit(Device),
}
#[derive(Debug, Clone, PartialEq)]
pub enum MotionAction {
    AbsoluteRead {
        source: Device,
        control: Device,
        value: CalendarValue,
    },
    Home {
        frequency: CalendarValue,
        jog: CalendarValue,
        dog: Device,
        output: Device,
    },
    Velocity {
        wide: bool,
        frequency: CalendarValue,
        output: Device,
        direction: Device,
    },
    Move {
        absolute: bool,
        position: CalendarValue,
        frequency: CalendarValue,
        output: Device,
        direction: Device,
    },
    Frequency {
        frequency: CalendarValue,
        gap: CalendarValue,
        output: Device,
    },
}
impl MotionAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let n=match name {"DABSR"|"PLSV"|"DPLSV"|"DICF"=>3,"DZRN"|"DDRVI"|"DDRVA"=>4,_=>return Err("DABSR/DZRN/PLSV/DPLSV/DDRVI/DDRVA/DICF gerekli; DICFP dış darbe sarmalayıcısı kullanır".into())};
        if args.len() != n {
            return Err(format!("{name} için {n} operand gerekli"));
        }
        let bit = |i: usize, areas: &[Area]| -> Result<Device, String> {
            let v: Device = args[i].parse()?;
            if !areas.contains(&v.area) {
                return Err(format!("{name} bit operand alanı geçersiz"));
            }
            let output_bit = (name == "DABSR" && i == 1)
                || (matches!(name, "PLSV" | "DPLSV") && i == 2)
                || (matches!(name, "DDRVI" | "DDRVA") && i == 3);
            if output_bit && !v.writable_bit() {
                return Err(format!("{name} kontrol/yön biti salt okunur olamaz"));
            }
            Ok(v)
        };
        let output = |i| -> Result<Device, String> {
            let v = bit(i, &[Area::Y])?;
            if ![0, 1, 2, 3, 4, 6].contains(&v.index) {
                return Err(
                    "Hareket darbe çıkışı Y0/Y1/Y2/Y3/Y4/Y6 olmalı; model yürütmede denetlenir"
                        .into(),
                );
            }
            Ok(v)
        };
        Ok(match name {
            "DABSR" => {
                if args[2].starts_with(['E', 'F']) {
                    return Err("DABSR hedef matrisinde E/F yok".into());
                }
                let value =
                    CalendarValue::Additional(AdditionalValue::parse(&args[2], true, true)?);
                Self::AbsoluteRead {
                    source: bit(0, &[Area::X, Area::Y, Area::M, Area::S])?,
                    control: bit(1, &[Area::Y, Area::M, Area::S])?,
                    value,
                }
            }
            "DZRN" => Self::Home {
                frequency: CalendarValue::parse(&args[0], true)?,
                jog: CalendarValue::parse(&args[1], true)?,
                dog: bit(2, &[Area::X])?,
                output: output(3)?,
            },
            "PLSV" | "DPLSV" => Self::Velocity {
                wide: name == "DPLSV",
                frequency: CalendarValue::parse(&args[0], name == "DPLSV")?,
                output: output(1)?,
                direction: bit(2, &[Area::Y, Area::M, Area::S])?,
            },
            "DDRVI" | "DDRVA" => Self::Move {
                absolute: name == "DDRVA",
                position: CalendarValue::parse(&args[0], true)?,
                frequency: CalendarValue::parse(&args[1], true)?,
                output: output(2)?,
                direction: bit(3, &[Area::Y, Area::M, Area::S])?,
            },
            _ => {
                let frequency = CalendarValue::parse(&args[0], true)?;
                if frequency.base().is_none_or(|v| v.area != Area::D) || args[0].starts_with('E') {
                    return Err("DICF S1 çıplak D çifti olmalı".into());
                }
                let gap = CalendarValue::parse(&args[1], true)?;
                if gap.base().is_some_and(|v| v.area != Area::D)
                    || matches!(gap, CalendarValue::Index { .. })
                {
                    return Err("DICF S2 K/H/D olmalı".into());
                }
                Self::Frequency {
                    frequency,
                    gap,
                    output: output(2)?,
                }
            }
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::AbsoluteRead { .. } => "DABSR",
            Self::Home { .. } => "DZRN",
            Self::Velocity { wide: false, .. } => "PLSV",
            Self::Velocity { wide: true, .. } => "DPLSV",
            Self::Move {
                absolute: false, ..
            } => "DDRVI",
            Self::Move { absolute: true, .. } => "DDRVA",
            Self::Frequency { .. } => "DICF",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::AbsoluteRead { .. } => 0x5701,
            Self::Home { .. } => 0x5901,
            Self::Velocity { wide: false, .. } => 0x5A01,
            Self::Velocity { wide: true, .. } => 0x5B01,
            Self::Move {
                absolute: false, ..
            } => 0x5D01,
            Self::Move { absolute: true, .. } => 0x5F01,
            Self::Frequency { .. } => 0xAD01,
        }
    }
    pub fn words(&self) -> u16 {
        match self {
            Self::AbsoluteRead { .. }
            | Self::Velocity { wide: true, .. }
            | Self::Frequency { .. } => 13,
            Self::Velocity { wide: false, .. } => 7,
            _ => 17,
        }
    }
    pub fn wide(&self) -> bool {
        !matches!(self, Self::Velocity { wide: false, .. })
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::AbsoluteRead { control, .. } => *control,
            Self::Home { output, .. }
            | Self::Velocity { output, .. }
            | Self::Move { output, .. }
            | Self::Frequency { output, .. } => *output,
        }
    }
    pub fn operands(&self) -> Vec<MotionOperand> {
        use MotionOperand::{Bit as B, Value as V};
        match self {
            Self::AbsoluteRead {
                source,
                control,
                value,
            } => vec![B(*source), B(*control), V(value.clone())],
            Self::Home {
                frequency,
                jog,
                dog,
                output,
            } => vec![V(frequency.clone()), V(jog.clone()), B(*dog), B(*output)],
            Self::Velocity {
                frequency,
                output,
                direction,
                ..
            } => vec![V(frequency.clone()), B(*output), B(*direction)],
            Self::Move {
                position,
                frequency,
                output,
                direction,
                ..
            } => vec![
                V(position.clone()),
                V(frequency.clone()),
                B(*output),
                B(*direction),
            ],
            Self::Frequency {
                frequency,
                gap,
                output,
            } => vec![V(frequency.clone()), V(gap.clone()), B(*output)],
        }
    }
    pub fn label(&self) -> String {
        format!(
            "{} {}",
            self.name(),
            self.operands()
                .iter()
                .map(|v| match v {
                    MotionOperand::Bit(d) => d.to_string(),
                    MotionOperand::Value(v) => v.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
    pub fn display_value(&self, m: &Memory) -> String {
        if let Self::AbsoluteRead { value, .. } = self {
            return format!("DABSR: {}", value.read(m));
        }
        let y = self.dest().index as u8;
        format!(
            "{}: konum {}",
            self.name(),
            m.pulse_output.positions.get(&y).copied().unwrap_or(0)
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for v in self.operands() {
            match v {
                MotionOperand::Bit(v) => {
                    out.insert(v);
                }
                MotionOperand::Value(v) => v.devices(out),
            }
        }
        match self {
            Self::AbsoluteRead {
                source, control, ..
            } => {
                for n in 0..3 {
                    out.insert(d(source.area, source.index + n));
                    out.insert(d(control.area, control.index + n));
                }
                out.extend([
                    d(Area::M, 1177),
                    d(Area::M, 1580),
                    d(Area::M, 1581),
                    d(Area::M, 1029),
                ]);
            }
            Self::Home { dog, output, .. } => {
                out.insert(d(Area::X, dog.index + 1));
                out.insert(d(Area::X, if output.index == 0 { 2 } else { 3 }));
            }
            _ => {}
        }
        if !matches!(self, Self::AbsoluteRead { .. }) {
            let y = self.dest().index as u8;
            let p = port(y);
            out.extend([
                d(Area::D, p.pv),
                d(Area::D, p.pv + 1),
                d(Area::M, p.complete),
                d(Area::M, p.pause),
                d(Area::M, p.reverse),
                d(Area::D, p.start),
                d(Area::D, p.ramp),
                d(Area::D, p.down),
                d(Area::M, p.asym),
                d(Area::M, p.status),
                d(Area::M, 1119),
                d(Area::M, 1307),
                d(Area::M, 1308),
                d(Area::M, 1346),
                d(Area::D, 1312),
            ]);
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if m.system_bit(1119)
            && let Self::Move {
                position,
                frequency,
                ..
            } = self
        {
            if let Ok(v) = next_pair(position) {
                v.devices(out);
            }
            if let Ok(v) = next_pair(frequency) {
                v.devices(out);
            }
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let at = m.timing_io.now_us;
        if m.pulse_output.now_us != at {
            PulseState::advance(m, at);
        }
        let mut s = std::mem::take(&mut m.motion_basic);
        s.now_us = at;
        s.raw = m.timing_io.physical_inputs.clone();
        s.visit(m, self, power, pc);
        m.motion_basic = s;
    }
}
#[derive(Debug, Clone, Copy)]
struct Port {
    pv: u16,
    complete: u16,
    pause: u16,
    status: u16,
    reverse: u16,
    start: u16,
    ramp: u16,
    down: u16,
    asym: u16,
}
fn port(y: u8) -> Port {
    let (pv, complete, pause, status, reverse, start, ramp, down, asym) = match y {
        0 => (1030, 1029, 1078, 1538, 1305, 1340, 1343, 1348, 1534),
        1 => (1032, 1030, 1079, 1539, 1305, 1340, 1343, 1343, 1534),
        2 => (1336, 1102, 1104, 1540, 1306, 1352, 1353, 1349, 1535),
        3 => (1338, 1103, 1105, 1541, 1306, 1352, 1353, 1353, 1535),
        4 => (1375, 1321, 1310, 1542, 1532, 1379, 1381, 1350, 1536),
        _ => (1377, 1322, 1311, 1543, 1533, 1380, 1382, 1351, 1537),
    };
    Port {
        pv,
        complete,
        pause,
        status,
        reverse,
        start,
        ramp,
        down,
        asym,
    }
}
fn next_pair(v: &CalendarValue) -> Result<CalendarValue, String> {
    match v {
        CalendarValue::Additional(AdditionalValue::Double(crate::wide::WideOperand::Register(
            b,
        ))) => CalendarValue::parse(
            &d(
                b.area,
                b.index
                    .checked_add(if b.area == Area::C && b.index >= 200 {
                        1
                    } else {
                        2
                    })
                    .ok_or("İkinci hız adresi taştı")?,
            )
            .to_string(),
            true,
        ),
        _ => Err("İki hızlı hareket S1/S2 ardışık register çiftleri gerektirir".into()),
    }
}
fn read(m: &Memory, v: &CalendarValue) -> Result<i32, String> {
    let mut set = BTreeSet::new();
    v.devices(&mut set);
    if set.iter().any(|v| !fits(m, *v)) {
        Err("Hareket operandının tam aralığı model belleğini aşıyor".into())
    } else {
        Ok(v.read(m))
    }
}
fn fits(m: &Memory, v: Device) -> bool {
    if (v.area == Area::X && v.index >= m.refresh_io.profile.input_points)
        || (v.area == Area::Y && v.index >= m.refresh_io.profile.output_points)
    {
        return false;
    }
    v.index
        <= match v.area {
            Area::D => m.data_register_limit.unwrap_or(9999),
            Area::M => m.relay_limit.unwrap_or(4095),
            Area::X | Area::Y => 255,
            Area::S => 1023,
            _ => 255,
        }
}
fn valid_bit_span(m: &Memory, base: Device, count: u16, writable: bool) -> bool {
    (0..count).all(|offset| {
        base.index.checked_add(offset).is_some_and(|index| {
            let device = d(base.area, index);
            fits(m, device)
                && device.to_string().parse::<Device>() == Ok(device)
                && (!writable || device.writable_bit())
        })
    })
}
fn write(m: &mut Memory, v: &CalendarValue, value: i32) {
    match v {
        CalendarValue::Additional(a) => a.write(m, value),
        CalendarValue::Index {
            index,
            wide,
            high_bank,
        } => {
            if *wide {
                let _ = CalendarIndices::write_double(m, *index, value);
            } else {
                let _ = CalendarIndices::write_word(m, *index, *high_bank, value as i16);
            }
        }
    }
}
fn constant(rate: u64) -> VecDeque<PulseSegment> {
    VecDeque::from([PulseSegment {
        duration_us: 0,
        frequency_millihz: rate * 1000,
        phase: MotionPhase::Cruise,
    }])
}
fn revision() -> AxisRevision {
    AxisRevision {
        frequency_millihz: None,
        negative: None,
        paused: None,
        target_pulses: None,
        segments: None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrajectoryPolicy {
    #[default]
    SharedTwentyStepCandidate,
    HoldUnspecifiedInterpolation,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServoFrame {
    Mitsubishi {
        position: i32,
        check_valid: bool,
    },
    Delta {
        status: u16,
        multiturn: u16,
        puu: u32,
        valid: bool,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HomePhase {
    SeekDog,
    OnDog,
    LimitBackoff,
    ForwardDog,
    SeekZ,
    Extra,
    Clear,
    Done,
}
#[derive(Debug, Clone)]
enum RunKind {
    Velocity {
        negative: bool,
        resume_epoch: Option<u64>,
        pending: i32,
    },
    Move {
        two_speed: bool,
    },
    Home {
        dog: Device,
        phase: HomePhase,
        jog: u64,
        fast: u64,
        invert: bool,
        reverse: bool,
        right: bool,
        limit_enabled: bool,
        limit_falling: bool,
        extra: i16,
        pulses: bool,
        z_seen: u16,
        clear: Option<Device>,
        deadline: Option<u64>,
    },
    Servo {
        source: Device,
        control: Device,
        value: CalendarValue,
        delta: bool,
        deadline: Option<u64>,
        done: bool,
    },
}
#[derive(Debug, Clone)]
struct Run {
    action: MotionAction,
    kind: RunKind,
    active: bool,
}
#[derive(Debug, Clone, Default)]
pub struct MotionBasicState {
    pub now_us: u64,
    pub trajectory: TrajectoryPolicy,
    pub notices: BTreeSet<String>,
    pub servo_frames: BTreeMap<u16, ServoFrame>,
    runs: BTreeMap<u16, Run>,
    raw: BTreeMap<Device, bool>,
    epoch: u64,
}
impl MotionBasicState {
    pub fn set_servo_frame(&mut self, pc: u16, frame: ServoFrame) {
        self.servo_frames.insert(pc, frame);
        self.notices.insert(
            "DABSR: açık sanal servo yanıtı; ham ABS tel protokolü/checksum eşdeğerliği değildir"
                .into(),
        );
    }
    pub fn claimed_inputs(&self) -> BTreeMap<Device, ResourceOwner> {
        let mut out = BTreeMap::new();
        for (&pc, r) in &self.runs {
            if !r.active {
                continue;
            }
            let mut inputs = vec![];
            match &r.kind {
                RunKind::Home {
                    dog,
                    limit_enabled,
                    extra,
                    pulses,
                    ..
                } => {
                    inputs.push(*dog);
                    if *limit_enabled {
                        inputs.push(d(Area::X, dog.index + 1));
                    }
                    if *extra != 0 && !*pulses {
                        inputs.push(d(Area::X, if r.action.dest().index == 0 { 2 } else { 3 }));
                    }
                }
                RunKind::Servo { source, .. } => {
                    for n in 0..3 {
                        if source.area == Area::X {
                            inputs.push(d(Area::X, source.index + n));
                        }
                    }
                }
                _ => {}
            }
            for v in inputs {
                out.insert(v, ResourceOwner::External(format!("MOTION:{pc}")));
            }
        }
        out
    }
    pub fn claimed_outputs(&self) -> BTreeMap<Device, ResourceOwner> {
        let mut out = BTreeMap::new();
        for (&pc, r) in &self.runs {
            if !r.active {
                continue;
            }
            let mut outputs = vec![];
            match &r.kind {
                RunKind::Home { clear: Some(v), .. } => outputs.push(*v),
                RunKind::Servo { control, delta, .. } if control.area == Area::Y => {
                    for n in 0..if *delta { 2 } else { 3 } {
                        outputs.push(d(Area::Y, control.index + n));
                    }
                }
                _ => {}
            }
            for v in outputs {
                out.insert(v, ResourceOwner::External(format!("MOTION:{pc}")));
            }
        }
        out
    }
    fn sync(&self, m: &mut Memory) {
        m.timing_io.input_owners.retain(|_, v| !ours(v));
        m.timing_io.output_owners.retain(|_, v| !ours(v));
        for (v, o) in self.claimed_inputs() {
            m.timing_io.input_owners.entry(v).or_insert(o);
        }
        for (v, o) in self.claimed_outputs() {
            m.timing_io.output_owners.entry(v).or_insert(o);
        }
    }
    fn claimable(&self, m: &Memory, pc: u16, r: &Run) -> Result<(), String> {
        // Only the candidate contributes claims. Merging existing runs first
        // can overwrite a colliding pin with a different PC and hide the conflict.
        let mut test = Self::default();
        test.runs.insert(pc, r.clone());
        let (matrix_inputs, matrix_outputs) = m.refresh_io.matrix_pins();
        let owner = ResourceOwner::External(format!("MOTION:{pc}"));
        for (v, o) in test
            .claimed_inputs()
            .into_iter()
            .chain(test.claimed_outputs())
        {
            if o != owner {
                continue;
            }
            if !fits(m, v) {
                return Err("Hareket pin aralığı model belleğini aşıyor".into());
            }
            if (v.area == Area::X && matrix_inputs.contains(&v))
                || (v.area == Area::Y && matrix_outputs.contains(&v))
            {
                return Err("Hareket pini etkin MTR matrisine ait".into());
            }
            let existing = if v.area == Area::X {
                m.timing_io.input_owners.get(&v)
            } else {
                m.timing_io.output_owners.get(&v)
            };
            if existing.is_some_and(|v| v != &owner) {
                return Err("Hareket algılayıcı/servo pini başka çevre birimine ait".into());
            }
        }
        Ok(())
    }
    /// Homing owns DOG and its enabled limit through the final clear pulse.
    /// This does not mutate user masks or pending events of any other IRQ.
    pub fn suppresses_external_irq(&self, input: Device) -> bool {
        self.runs.values().any(|run| {
            run.active
                && matches!(&run.kind,
                RunKind::Home { dog, limit_enabled, phase, .. }
                if *phase != HomePhase::Done
                    && (input == *dog || (*limit_enabled && input == d(Area::X, dog.index + 1))))
        })
    }
    pub fn begin_scan(m: &mut Memory) {
        m.motion_basic.epoch = m.motion_basic.epoch.wrapping_add(1);
    }
    pub fn end_scan(_m: &mut Memory) {}
    /// Zero-time service only touches axes whose immediate pause changed.
    pub fn service_controls(m: &mut Memory) {
        let controls: Vec<_> = m
            .motion_basic
            .runs
            .iter()
            .filter_map(|(&pc, r)| {
                if !r.active || !matches!(r.kind, RunKind::Move { .. }) {
                    return None;
                }
                let p = port(r.action.dest().index as u8);
                let paused = m.system_bit(p.pause);
                m.pulse_output
                    .axis_snapshot(pc)
                    .filter(|s| s.busy && s.paused != paused)
                    .map(|_| (pc, paused, p.status))
            })
            .collect();
        for (pc, paused, status) in controls {
            let mut change = revision();
            change.paused = Some(paused);
            if let Err(e) = PulseState::revise_axis(m, pc, change, m.pulse_output.now_us) {
                limit(m, e);
            }
            m.set_system(status, paused);
        }
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.runs
            .values()
            .filter(|r| r.active)
            .filter_map(|r| match r.kind {
                RunKind::Home { deadline, .. } | RunKind::Servo { deadline, .. } => deadline,
                _ => None,
            })
            .min()
    }
    pub fn abort(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.motion_basic);
        s.runs.clear();
        s.sync(m);
        m.motion_basic = s;
    }
    pub fn advance(m: &mut Memory, at_us: u64, raw: &BTreeMap<Device, bool>) {
        let mut s = std::mem::take(&mut m.motion_basic);
        if at_us < s.now_us {
            limit(m, "Hareket saati geriye gidiyor");
            m.motion_basic = s;
            return;
        }
        s.now_us = at_us;
        s.raw = raw.clone();
        s.service(m);
        m.motion_basic = s;
    }
    pub fn input_edge(m: &mut Memory, input: Device, old: bool, new: bool, at_us: u64) {
        if old == new {
            return;
        }
        let mut s = std::mem::take(&mut m.motion_basic);
        s.now_us = at_us;
        s.raw.insert(input, new);
        let ids: Vec<_> = s.runs.keys().copied().collect();
        for pc in ids {
            let mut r = s.runs.remove(&pc).unwrap();
            s.home_edge(m, pc, &mut r, input, old, new);
            s.runs.insert(pc, r);
        }
        m.motion_basic = s;
    }
    fn level(&self, m: &Memory, input: Device) -> bool {
        self.raw
            .get(&input)
            .copied()
            .unwrap_or_else(|| m.bit(input))
    }
    fn stop(&mut self, m: &mut Memory, pc: u16, r: &mut Run) {
        PulseState::release_axis(m, pc, true, self.now_us);
        match &r.kind {
            RunKind::Servo { control, delta, .. } => {
                for n in 0..if *delta { 2 } else { 3 } {
                    set(m, d(control.area, control.index + n), false)
                }
            }
            RunKind::Home { clear: Some(v), .. } => set(m, *v, false),
            _ => {}
        }
        r.active = false;
    }
    fn visit(&mut self, m: &mut Memory, a: &MotionAction, power: bool, pc: u16) {
        if !power {
            if let Some(mut r) = self.runs.remove(&pc) {
                self.stop(m, pc, &mut r);
            }
            self.sync(m);
            return;
        }
        if let MotionAction::Frequency {
            frequency,
            gap,
            output,
        } = a
        {
            if let Some(source) = frequency.base()
                && crate::motion_extended::MotionExtendedState::change_frequency(
                    m,
                    source,
                    gap,
                    *output,
                    self.now_us,
                )
            {
                return;
            }
            self.change_frequency(m, frequency, *output);
            return;
        }
        if let Some(mut r) = self.runs.remove(&pc)
            && r.active
        {
            self.visit_existing(m, pc, &mut r);
            self.runs.insert(pc, r);
            self.sync(m);
            return;
        }
        if self.runs.len() >= 512 {
            limit(m, "Hareket site sınırı512");
            return;
        }
        if !matches!(a, MotionAction::AbsoluteRead { .. })
            && m.pulse_output
                .axis_owner(a.dest().index as u8)
                .is_some_and(|owner| owner != pc)
        {
            // The manual explicitly gives the first activated output owner priority.
            return;
        }
        if let Err(e) = self.start(m, a, pc) {
            limit(m, e);
        }
        self.sync(m);
    }
    fn start(&mut self, m: &mut Memory, a: &MotionAction, pc: u16) -> Result<(), String> {
        let r = match a {
            MotionAction::AbsoluteRead {
                source,
                control,
                value,
            } => {
                match value {
                    CalendarValue::Additional(AdditionalValue::Double(
                        crate::wide::WideOperand::Register(base),
                    )) if matches!(base.area, Area::T | Area::C | Area::D) => {}
                    CalendarValue::Additional(AdditionalValue::Group { base, digits, .. })
                        if valid_bit_span(m, *base, digits * 4, true) => {}
                    _ => {
                        return Err(
                            "DABSR yazılabilir T/C/D veya belgeli bit grubu gerektirir; E/F yok"
                                .into(),
                        );
                    }
                }
                read(m, value)?;
                if !valid_bit_span(m, *source, 3, false) {
                    return Err("DABSR tam giriş aralığı model belleğini aşıyor".into());
                }
                if self
                    .runs
                    .values()
                    .any(|r| r.active && matches!(r.kind, RunKind::Servo { .. }))
                {
                    return Err("DABSR programda yalnız bir kez kullanılabilir".into());
                }
                let delta = m.system_bit(1177);
                if delta && m.pulse_output.model == PulseModel::Ss2 {
                    return Err("SS2 ASDA-A2 DABSR modu belgeli değil".into());
                }
                if delta {
                    let CalendarValue::Additional(AdditionalValue::Double(
                        crate::wide::WideOperand::Register(base),
                    )) = value
                    else {
                        return Err(
                            "ASDA-A2 dört ayrı16bit sonuç için T/C/D register dizisi gerekir"
                                .into(),
                        );
                    };
                    if !matches!(base.area, Area::T | Area::C | Area::D)
                        || (base.area == Area::C && base.index > 196)
                        || !(0..4).all(|n| {
                            base.index
                                .checked_add(n)
                                .is_some_and(|index| fits(m, d(base.area, index)))
                        })
                    {
                        return Err("ASDA-A2 dört sonuç aralığı model sınırını aşıyor".into());
                    }
                }
                let r = Run {
                    action: a.clone(),
                    kind: RunKind::Servo {
                        source: *source,
                        control: *control,
                        value: value.clone(),
                        delta,
                        deadline: None,
                        done: false,
                    },
                    active: true,
                };
                self.claimable(m, pc, &r)?;
                if !valid_bit_span(m, *control, if delta { 2 } else { 3 }, true) {
                    return Err(
                        "DABSR kontrol aralığı model/salt okunur/seyrek bit sınırını aşıyor".into(),
                    );
                }
                set(m, *control, true);
                set(m, d(control.area, control.index + 1), true);
                if delta {
                    m.set_system(1580, false);
                    m.set_system(1581, false);
                } else {
                    m.set_system(1029, false);
                }
                r
            }
            MotionAction::Velocity {
                wide,
                frequency,
                output,
                direction,
            } => {
                let f = read(m, frequency)?;
                if output.index % 2 != 0 {
                    return Err("PLSV/DPLSV yalnız belgeli çift darbe kanallarını kullanır".into());
                }
                self.validate_axis(m, *output, *direction, *wide)?;
                let hz = self.hz(m, output.index as u8, f.unsigned_abs() as u64, *wide);
                let p = port(output.index as u8);
                PulseState::submit_axis(
                    m,
                    AxisRequest {
                        owner_pc: pc,
                        channel: output.index as u8,
                        direction: *direction,
                        negative: f < 0,
                        invert_direction: m.system_bit(p.reverse),
                        target_pulses: None,
                        segments: constant(hz),
                        completion: false,
                        paused: false,
                    },
                    self.now_us,
                )?;
                Run {
                    action: a.clone(),
                    kind: RunKind::Velocity {
                        negative: f < 0,
                        resume_epoch: None,
                        pending: f,
                    },
                    active: true,
                }
            }
            MotionAction::Move {
                absolute,
                position,
                frequency,
                output,
                direction,
            } => {
                self.validate_axis(m, *output, *direction, true)?;
                if m.system_bit(1257) {
                    return Err(
                        "Hareket S-eğrisinin tam formülü kanıtlanmadı; eksen tutuldu".into(),
                    );
                }
                if !*absolute && m.system_bit(if output.index == 0 { 1156 } else { 1158 }) {
                    return Err(
                        "DDRVI işaret/mask olaylarının ortak eksen entegrasyonu henüz doğrulanmadı"
                            .into(),
                    );
                }
                if self.trajectory == TrajectoryPolicy::HoldUnspecifiedInterpolation {
                    return Err("Hareket ara değerleme fiziksel profili seçilmedi".into());
                }
                let value = read(m, position)?;
                let frequency_value = read(m, frequency)?;
                let y = output.index as u8;
                let current = m.pulse_output.axis_position(y, m);
                let distance = if *absolute {
                    i64::from(value) - i64::from(current)
                } else {
                    i64::from(value)
                };
                let p = port(y);
                let peak = self.hz(m, y, frequency_value.max(6) as u64, true);
                let mut queue = self.move_plan(m, y, distance.unsigned_abs(), peak, *absolute);
                let two = m.system_bit(1119);
                let mut total = distance.unsigned_abs();
                let mut negative = distance < 0;
                if two {
                    let pos2 = read(m, &next_pair(position)?)?;
                    let freq2 = read(m, &next_pair(frequency)?)?;
                    let second = if *absolute {
                        i64::from(pos2) - i64::from(value)
                    } else {
                        i64::from(pos2)
                    };
                    if distance.signum() != second.signum() && distance != 0 && second != 0 {
                        return Err("İki hızlı hareket bölümleri aynı yönde olmalı".into());
                    }
                    if distance == 0 && second != 0 {
                        negative = second < 0;
                    }
                    total = total
                        .checked_add(second.unsigned_abs())
                        .ok_or("İki hızlı darbe sayısı taştı")?;
                    queue.extend(self.move_plan(
                        m,
                        y,
                        second.unsigned_abs(),
                        self.hz(m, y, freq2.max(6) as u64, true),
                        *absolute,
                    ));
                }
                self.notices.insert("DDRVI/DDRVA: ortak20/10 basamaklı açık yerel ara değerleme; tam firmware kısa-yol/yuvarlama eşdeğerliği değildir".into());
                PulseState::submit_axis(
                    m,
                    AxisRequest {
                        owner_pc: pc,
                        channel: y,
                        direction: *direction,
                        negative,
                        invert_direction: m.system_bit(p.reverse),
                        target_pulses: Some(total),
                        segments: queue,
                        completion: true,
                        paused: m.system_bit(p.pause),
                    },
                    self.now_us,
                )?;
                Run {
                    action: a.clone(),
                    kind: RunKind::Move { two_speed: two },
                    active: true,
                }
            }
            MotionAction::Home {
                frequency,
                jog,
                dog,
                output,
            } => {
                let y = output.index as u8;
                self.validate_axis(m, *output, d(Area::Y, output.index + 1), true)?;
                let expected = match y {
                    0 => 4,
                    2 => 6,
                    4 => 12,
                    6 => 14,
                    _ => return Err("DZRN yalnız çift darbe kanalı kullanır".into()),
                };
                if dog.index != expected {
                    return Err("DZRN DOG/çıkış çifti model tablosuyla uyuşmuyor".into());
                }
                let fast = self.hz(m, y, read(m, frequency)?.max(6) as u64, true);
                let slow = self.hz(m, y, read(m, jog)?.max(0) as u64, true);
                if slow == 0 || slow >= fast {
                    return Err("DZRN JOG pozitif ve hedef frekanstan küçük olmalı".into());
                }
                let extra = m.word(d(Area::D, 1312));
                let pulses = m.system_bit(1308);
                if (!pulses && extra.unsigned_abs() > 10)
                    || (pulses && extra.unsigned_abs() > 30000)
                    || (!pulses && extra != 0 && y >= 4)
                {
                    return Err("DZRN ek darbe/Z-faz parametresi model sınırını aşıyor".into());
                }
                let p = port(y);
                let on = self.level(m, *dog);
                let phase = if on {
                    HomePhase::OnDog
                } else {
                    HomePhase::SeekDog
                };
                let clear = m.system_bit(1346).then_some(d(
                    Area::Y,
                    if matches!(m.pulse_output.model, PulseModel::Ec5 | PulseModel::Se2) {
                        8 + u16::from(y / 2)
                    } else {
                        4 + u16::from(y / 2)
                    },
                ));
                let r = Run {
                    action: a.clone(),
                    active: true,
                    kind: RunKind::Home {
                        dog: *dog,
                        phase,
                        jog: slow,
                        fast,
                        invert: m.system_bit(p.reverse),
                        reverse: true,
                        right: m.system_bit(if y == 0 {
                            1106
                        } else if y == 2 {
                            1107
                        } else {
                            1586 + u16::from(y / 2)
                        }),
                        limit_enabled: m.system_bit(1307),
                        limit_falling: m.system_bit(1584 + u16::from(y / 2)),
                        extra,
                        pulses,
                        z_seen: 0,
                        clear,
                        deadline: None,
                    },
                };
                self.claimable(m, pc, &r)?;
                PulseState::submit_axis(
                    m,
                    AxisRequest {
                        owner_pc: pc,
                        channel: y,
                        direction: d(Area::Y, output.index + 1),
                        negative: true,
                        invert_direction: m.system_bit(p.reverse),
                        target_pulses: None,
                        segments: constant(if on { slow } else { fast }),
                        completion: false,
                        paused: false,
                    },
                    self.now_us,
                )?;
                m.set_system(p.complete, false);
                r
            }
            MotionAction::Frequency { .. } => unreachable!(),
        };
        let mut r = r;
        self.service_run(m, pc, &mut r);
        self.runs.insert(pc, r);
        Ok(())
    }
    fn validate_axis(
        &self,
        m: &Memory,
        output: Device,
        direction: Device,
        _wide: bool,
    ) -> Result<(), String> {
        let extended = matches!(m.pulse_output.model, PulseModel::Ec5 | PulseModel::Se2);
        if (!extended && output.index > 3)
            || (extended && (output.index > 6 || !output.index.is_multiple_of(2)))
        {
            return Err("Hareket kanalı seçilen CPU modelinde yok".into());
        }
        if !valid_bit_span(m, direction, 1, true) || direction == output {
            return Err("Hareket yön biti model aralığında ve darbe pininden farklı olmalı".into());
        }
        if direction.area == Area::Y {
            let ordinary = output.index.is_multiple_of(2) && direction.index == output.index + 1;
            let modern = m.pulse_output.model == PulseModel::Es2
                && output.index <= 3
                && direction.index == output.index + 4;
            if !ordinary && !modern {
                return Err("Hareket yön Y çifti belgeli tablolara uymuyor".into());
            }
        }
        Ok(())
    }
    fn hz(&self, m: &Memory, y: u8, f: u64, wide: bool) -> u64 {
        f.min(if m.pulse_output.model == PulseModel::Ss2 {
            10000
        } else if m.pulse_output.model == PulseModel::Ec5 {
            50000
        } else if !wide {
            32767
        } else if y % 2 == 1 {
            10000
        } else {
            100000
        })
    }
    fn move_plan(
        &self,
        m: &Memory,
        y: u8,
        total: u64,
        peak: u64,
        absolute: bool,
    ) -> VecDeque<PulseSegment> {
        let p = port(y);
        let raw = m.word(d(Area::D, p.ramp));
        let ramp = if absolute && raw < 20 {
            20
        } else {
            raw.clamp(20, 32767)
        } as u64
            * 1000;
        let down = if m.system_bit(p.asym) {
            m.word(d(Area::D, p.down)).clamp(20, 32767) as u64 * 1000
        } else {
            ramp
        };
        let maxstart = if absolute { 32767 } else { 100000 };
        let mut start = m.word(d(Area::D, p.start)).max(6) as u64;
        start = start.min(maxstart);
        if matches!(m.pulse_output.model, PulseModel::Ec5 | PulseModel::Se2) {
            start = start.min(peak);
        }
        PulseState::axis_step_plan(
            start,
            peak,
            ramp,
            down,
            total,
            if y.is_multiple_of(2) { 20 } else { 10 },
        )
    }
    fn visit_existing(&mut self, m: &mut Memory, pc: u16, r: &mut Run) {
        match &mut r.kind {
            RunKind::Velocity {
                negative,
                resume_epoch,
                pending,
            } => {
                let MotionAction::Velocity {
                    frequency,
                    output,
                    wide,
                    ..
                } = &r.action
                else {
                    return;
                };
                let f = match read(m, frequency) {
                    Ok(v) => v,
                    Err(e) => {
                        limit(m, e);
                        return;
                    }
                };
                let sign = f < 0;
                let mut change = revision();
                if let Some(epoch) = *resume_epoch {
                    *pending = f;
                    if self.epoch >= epoch {
                        change.frequency_millihz = Some(
                            self.hz(m, output.index as u8, f.unsigned_abs() as u64, *wide) * 1000,
                        );
                        change.negative = Some(sign);
                        change.paused = Some(false);
                        *negative = sign;
                        *resume_epoch = None;
                    }
                } else if sign != *negative && f != 0 {
                    change.paused = Some(true);
                    *resume_epoch = Some(self.epoch.wrapping_add(1));
                    *pending = f;
                } else {
                    change.frequency_millihz =
                        Some(self.hz(m, output.index as u8, f.unsigned_abs() as u64, *wide) * 1000);
                }
                if let Err(e) = PulseState::revise_axis(m, pc, change, self.now_us) {
                    limit(m, e);
                }
                PulseState::axis_publish(m, pc);
            }
            RunKind::Move { two_speed } => {
                let p = port(r.action.dest().index as u8);
                if let Some(s) = m.pulse_output.axis_snapshot(pc)
                    && s.done
                {
                    if *two_speed {
                        m.set_system(1119, false);
                    }
                    PulseState::axis_publish(m, pc);
                    return;
                }
                let mut change = revision();
                change.paused = Some(m.system_bit(p.pause));
                if let Err(e) = PulseState::revise_axis(m, pc, change, self.now_us) {
                    limit(m, e);
                }
                m.set_system(p.status, m.system_bit(p.pause));
                if m.system_bit(if r.action.dest().index == 0 {
                    1108
                } else {
                    1110
                }) {
                    limit(
                        m,
                        "DDRVI/DDRVA yavaşlayarak duraklatma yolu henüz ortak motorla doğrulanmadı",
                    );
                }
                PulseState::axis_publish(m, pc);
            }
            _ => {}
        }
        self.service_run(m, pc, r);
    }
    fn change_frequency(&self, m: &mut Memory, frequency: &CalendarValue, output: Device) {
        let Some(pc) = m.pulse_output.axis_owner(output.index as u8) else {
            limit(m, "DICF önce etkin DPLSY/DVSPO gerektirir");
            return;
        };
        if m.pulse_output.train_source(pc) != Some(frequency) {
            limit(
                m,
                "DICF etkin DPLSY frekans aygıtıyla veya DVSPO frekans/gap/çıkış üçlüsüyle eşleşmeli",
            );
            return;
        }
        let v = match read(m, frequency) {
            Ok(v) if v >= 0 => v,
            Ok(_) => {
                limit(m, "DICF negatif hedefin fiziksel anlamı belgelenmedi");
                return;
            }
            Err(e) => {
                limit(m, e);
                return;
            }
        };
        let mut change = revision();
        change.frequency_millihz = Some(self.hz(m, output.index as u8, v as u64, true) * 1000);
        if let Err(e) = PulseState::revise_axis(m, pc, change, self.now_us) {
            limit(m, e);
        }
    }
    fn service(&mut self, m: &mut Memory) {
        let ids: Vec<_> = self.runs.keys().copied().collect();
        for pc in ids {
            let mut r = self.runs.remove(&pc).unwrap();
            self.service_run(m, pc, &mut r);
            self.runs.insert(pc, r);
        }
    }
    fn service_run(&mut self, m: &mut Memory, pc: u16, r: &mut Run) {
        if !r.active {
            return;
        }
        match &mut r.kind {
            RunKind::Move { two_speed } => {
                if *two_speed && m.pulse_output.axis_snapshot(pc).is_some_and(|s| s.done) {
                    m.set_system(1119, false);
                }
            }
            RunKind::Servo {
                source,
                control,
                value,
                delta,
                deadline,
                done,
            } => {
                if *done {
                    return;
                }
                if deadline.is_none() {
                    let valid = self.servo_frames.get(&pc).is_some_and(|f| {
                        matches!(
                            (f, *delta),
                            (ServoFrame::Delta { .. }, true)
                                | (ServoFrame::Mitsubishi { .. }, false)
                        )
                    });
                    if valid {
                        *deadline = self.now_us.checked_add(
                            if source.area == Area::X && source.index < 8 {
                                2_000_000
                            } else {
                                2_500_000
                            },
                        );
                    } else {
                        self.notices.insert("DABSR: servo yanıtı bekleniyor; ham38bit kontrol toplamı/kenar zamanlaması kanıtlanmadı".into());
                    }
                }
                if deadline.is_some_and(|at| at <= self.now_us) {
                    let frame = self.servo_frames.get(&pc).cloned();
                    match frame {
                        Some(ServoFrame::Mitsubishi {
                            position,
                            check_valid,
                        }) if !*delta => {
                            if check_valid {
                                write(m, value, position);
                                m.set_system(1029, true);
                            } else {
                                limit(
                                    m,
                                    "DABSR sanal MR-J2 yanıtı kontrol doğrulaması başarısız; veri tutuldu",
                                );
                            }
                        }
                        Some(ServoFrame::Delta {
                            status,
                            multiturn,
                            puu,
                            valid,
                        }) if *delta => {
                            if valid {
                                if let Some(base) = value.base() {
                                    for (n, v) in
                                        [status, multiturn, puu as u16, (puu >> 16) as u16]
                                            .into_iter()
                                            .enumerate()
                                    {
                                        m.set_word(d(base.area, base.index + n as u16), v as i16);
                                    }
                                }
                                m.set_system(1580, true);
                            } else {
                                m.set_system(1581, true);
                            }
                        }
                        _ => {
                            limit(m, "DABSR servo yanıt profili transfer sırasında değişti");
                        }
                    }
                    *done = true;
                    *deadline = None;
                    set(m, d(control.area, control.index + 1), false);
                }
            }
            RunKind::Home {
                phase,
                clear,
                deadline,
                ..
            } => {
                if *phase == HomePhase::Extra
                    && m.pulse_output.axis_snapshot(pc).is_some_and(|s| s.done)
                {
                    self.home_finish(m, pc, r);
                    return;
                }
                if *phase == HomePhase::Clear && deadline.is_some_and(|v| v <= self.now_us) {
                    if let Some(v) = clear {
                        set(m, *v, false);
                    }
                    *phase = HomePhase::Done;
                    *deadline = None;
                    self.zero_home(m, pc, r.action.dest());
                }
            }
            _ => {}
        }
    }
    fn home_finish(&mut self, m: &mut Memory, pc: u16, r: &mut Run) {
        PulseState::release_axis(m, pc, false, self.now_us);
        if let RunKind::Home {
            phase,
            clear,
            deadline,
            ..
        } = &mut r.kind
        {
            if let Some(v) = clear {
                set(m, *v, true);
                *deadline = self.now_us.checked_add(20_000);
                *phase = HomePhase::Clear;
            } else {
                *phase = HomePhase::Done;
                self.zero_home(m, pc, r.action.dest());
            }
        }
    }
    fn zero_home(&self, m: &mut Memory, _pc: u16, output: Device) {
        let y = output.index as u8;
        let p = port(y);
        m.pulse_output.positions.insert(y, 0);
        m.refresh_io.pulse_positions.insert(y, 0);
        m.set_double_word_raw(d(Area::D, p.pv), 0);
        m.set_system(p.complete, true);
    }
    fn home_edge(
        &mut self,
        m: &mut Memory,
        pc: u16,
        r: &mut Run,
        input: Device,
        _old: bool,
        new: bool,
    ) {
        let RunKind::Home {
            dog,
            phase,
            jog,
            fast,
            invert,
            reverse,
            right,
            limit_enabled,
            limit_falling,
            extra,
            pulses,
            z_seen,
            ..
        } = &mut r.kind
        else {
            return;
        };
        if !r.active || matches!(phase, HomePhase::Done | HomePhase::Clear) {
            return;
        }
        let y = r.action.dest().index as u8;
        let mut rate = None;
        let mut finish = false;
        if *limit_enabled && input == d(Area::X, dog.index + 1) && new != *limit_falling {
            *phase = HomePhase::LimitBackoff;
            *reverse = false;
            rate = Some(*fast);
        } else if input == d(Area::X, dog.index + 1)
            && *phase == HomePhase::LimitBackoff
            && new == *limit_falling
        {
            *phase = HomePhase::ForwardDog;
        } else if input == *dog {
            if new {
                if *right {
                    finish = true;
                } else {
                    *phase = HomePhase::OnDog;
                    *reverse = true;
                    rate = Some(*jog);
                }
            } else if *phase == HomePhase::OnDog {
                if *extra == 0 {
                    finish = true;
                } else {
                    *reverse = *extra < 0;
                    rate = Some(*jog);
                    if *pulses {
                        let emitted = m.pulse_output.axis_snapshot(pc).map_or(0, |v| v.emitted);
                        let mut change = revision();
                        change.target_pulses =
                            Some(Some(emitted + u64::from(extra.unsigned_abs())));
                        let _ = PulseState::revise_axis(m, pc, change, self.now_us);
                        *phase = HomePhase::Extra;
                    } else {
                        *phase = HomePhase::SeekZ;
                    }
                }
            }
        } else if input == d(Area::X, if y == 0 { 2 } else { 3 })
            && new
            && *phase == HomePhase::SeekZ
        {
            *z_seen += 1;
            if *z_seen >= extra.unsigned_abs() {
                finish = true;
            }
        }
        if finish {
            self.home_finish(m, pc, r);
            return;
        }
        if let Some(rate) = rate {
            let mut change = revision();
            change.frequency_millihz = Some(rate * 1000);
            change.negative = Some(*reverse);
            if let Err(e) = PulseState::revise_axis(m, pc, change, self.now_us) {
                limit(m, e);
            }
            let _ = invert;
        }
    }
}
fn ours(v: &ResourceOwner) -> bool {
    matches!(v,ResourceOwner::External(s)if s.starts_with("MOTION:"))
}
fn set(m: &mut Memory, v: Device, on: bool) {
    m.set_internal(v, on);
    if v.area == Area::Y {
        m.set_physical_output(v, on);
    }
}

#[cfg(test)]
#[path = "motion_basic_tests.rs"]
mod tests;
