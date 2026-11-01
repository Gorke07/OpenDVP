//! PDF-qualified virtual table, feedback and variable-speed motion.
//! Actual edge/servo physics and the installed coarse host clock are separate.
use crate::calendar_operand::CalendarValue;
use crate::motion_basic::MotionOperand;
use crate::plc::{Area, Device, Memory};
use crate::pulse_output::{
    AxisRequest, AxisRevision, MotionPhase, PulseModel, PulseSegment, PulseState,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn d(area: Area, index: u16) -> Device {
    Device { area, index }
}
fn limited(m: &mut Memory, message: impl Into<String>) {
    m.execution_limitations.insert(message.into());
}
#[derive(Debug, Clone, PartialEq)]
pub enum ExtendedMotionAction {
    Table {
        source: Device,
        control: Device,
        output: Device,
    },
    Closed {
        feedback: Device,
        target: CalendarValue,
        frequency: CalendarValue,
        output: Device,
    },
    Variable {
        frequency: Device,
        count: CalendarValue,
        gap: CalendarValue,
        output: Device,
    },
}
impl ExtendedMotionAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let n = match name {
            "DPTPO" => 3,
            "DCLLM" | "DVSPO" => 4,
            _ => return Err("DPTPO/DCLLM/DVSPO gerekli; P ve dar biçim yok".into()),
        };
        if args.len() != n {
            return Err(format!("{name}: {n} operand gerekli"));
        }
        let direct = |i: usize, area: Area| -> Result<Device, String> {
            let v: Device = args[i].parse()?;
            if v.area != area || area == Area::D && v.index >= 9999 {
                return Err(format!("{name}: doğrudan {area:?} aygıtı gerekli"));
            }
            Ok(v)
        };
        let output = direct(n - 1, Area::Y)?;
        if ![0, 2, 4, 6].contains(&output.index) {
            return Err("Y0/Y2 veya SE2 Y4/Y6 gerekli".into());
        }
        let numeric = |i: usize| -> Result<CalendarValue, String> {
            let v = CalendarValue::parse(&args[i], true)?;
            if matches!(v, CalendarValue::Index { .. })
                || v.base().is_some_and(|v| v.area != Area::D)
            {
                return Err("Yalnız K/H/D sayısal kaynak desteklenir".into());
            }
            Ok(v)
        };
        Ok(match name {
            "DPTPO" => Self::Table {
                source: direct(0, Area::D)?,
                control: direct(1, Area::D)?,
                output,
            },
            "DCLLM" => {
                let feedback: Device = args[0].parse()?;
                if !(feedback.area == Area::X && [4, 6].contains(&feedback.index)
                    || feedback.area == Area::C && (243..=254).contains(&feedback.index))
                {
                    return Err("DCLLM geri besleme X4/X6 veya C243..C254 olmalı".into());
                }
                Self::Closed {
                    feedback,
                    target: numeric(1)?,
                    frequency: numeric(2)?,
                    output,
                }
            }
            _ => Self::Variable {
                frequency: direct(0, Area::D)?,
                count: numeric(1)?,
                gap: numeric(2)?,
                output,
            },
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Table { .. } => "DPTPO",
            Self::Closed { .. } => "DCLLM",
            Self::Variable { .. } => "DVSPO",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Table { .. } => 0xa501,
            Self::Closed { .. } => 0xa901,
            Self::Variable { .. } => 0xab01,
        }
    }
    pub fn words(&self) -> u16 {
        if matches!(self, Self::Table { .. }) {
            13
        } else {
            17
        }
    }
    pub fn wide(&self) -> bool {
        true
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Table { output, .. }
            | Self::Closed { output, .. }
            | Self::Variable { output, .. } => *output,
        }
    }
    pub fn destination(&self) -> Device {
        self.dest()
    }
    pub fn operands(&self) -> Vec<MotionOperand> {
        let value = |v: Device| {
            MotionOperand::Value(
                CalendarValue::parse(&v.to_string(), true).expect("direct D encoded pair"),
            )
        };
        match self {
            Self::Table {
                source,
                control,
                output,
            } => vec![value(*source), value(*control), MotionOperand::Bit(*output)],
            Self::Closed {
                feedback,
                target,
                frequency,
                output,
            } => vec![
                MotionOperand::Bit(*feedback),
                MotionOperand::Value(target.clone()),
                MotionOperand::Value(frequency.clone()),
                MotionOperand::Bit(*output),
            ],
            Self::Variable {
                frequency,
                count,
                gap,
                output,
            } => vec![
                value(*frequency),
                MotionOperand::Value(count.clone()),
                MotionOperand::Value(gap.clone()),
                MotionOperand::Bit(*output),
            ],
        }
    }
    pub fn label(&self) -> String {
        format!(
            "{} {}",
            self.name(),
            self.operands()
                .iter()
                .map(|v| match v {
                    MotionOperand::Bit(v) => v.to_string(),
                    MotionOperand::Value(v) => v.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" ")
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
        for n in [1029, 1102, 1078, 1104, 1538, 1540] {
            out.insert(d(Area::M, n));
        }
        if matches!(self, Self::Table { .. }) {
            out.insert(d(Area::M, 1262));
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Self::Table {
            source, control, ..
        } = self
        {
            let n = m.word(*control);
            if (1..=40).contains(&n) {
                for k in 0..n as u16 * 4 {
                    if span(m, *source, u32::from(k) + 1) {
                        out.insert(d(Area::D, source.index + k));
                    }
                }
            }
        }
        for n in [
            1030, 1031, 1336, 1337, 1340, 1352, 1343, 1353, 1348, 1349, 1131, 1132, 1244, 1245,
            1026, 1027, 1135, 1136, 1220, 1221, 1222, 1223,
        ] {
            out.insert(d(Area::D, n));
        }
        for n in [1305, 1306, 1108, 1110, 1156, 1158, 1347, 1524, 1534, 1535] {
            out.insert(d(Area::M, n));
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let now = m.pulse_output.now_us;
        let mut s = std::mem::take(&mut m.motion_extended);
        s.now_us = now;
        if !power {
            if s.runs.remove(&pc).is_some() {
                s.captures.retain(|(owner, _), _| *owner != pc);
                PulseState::release_axis(m, pc, true, now);
                m.set_system(completion(self.dest()), false);
                m.set_system(status(self.dest()), false);
            }
        } else if let Some(mut run) = s.runs.remove(&pc) {
            if run.action != *self {
                limited(
                    m,
                    "Aynı PC'nin hareket tanımı değişti; yeniden başlatma gerekli",
                );
            } else if !run.done {
                s.visit(m, pc, &mut run, false);
            } else if let ExtendedMotionAction::Table { control, .. } = run.action
                && let Kind::Table { index, .. } = run.kind
            {
                m.set_word(d(Area::D, control.index + 1), index as i16 + 1);
            }
            PulseState::axis_publish(m, pc);
            s.runs.insert(pc, run);
        } else {
            match s.start(m, self.clone(), pc) {
                Ok(run) => {
                    s.runs.insert(pc, run);
                }
                Err(e) => limited(m, e),
            }
        }
        m.motion_extended = s;
    }
}
fn span(m: &Memory, base: Device, n: u32) -> bool {
    base.area == Area::D
        && n > 0
        && u32::from(base.index) + n - 1 <= u32::from(m.data_register_limit.unwrap_or(9999))
}
fn read(m: &Memory, v: &CalendarValue) -> Result<i32, String> {
    let mut ds = BTreeSet::new();
    v.devices(&mut ds);
    if ds.iter().any(|v| v.area == Area::D && !span(m, *v, 1)) {
        Err("Hareket sayısal kaynak çifti model belleğini aşıyor".into())
    } else {
        Ok(v.read(m))
    }
}
fn completion(output: Device) -> u16 {
    match output.index {
        0 => 1029,
        2 => 1102,
        4 => 1321,
        _ => 1322,
    }
}
fn status(output: Device) -> u16 {
    1538 + output.index / 2
}
fn pause(output: Device) -> u16 {
    match output.index {
        0 => 1078,
        2 => 1104,
        4 => 1542,
        _ => 1543,
    }
}
fn ramp_stop(output: Device) -> u16 {
    match output.index {
        0 => 1108,
        2 => 1110,
        4 => 1544,
        _ => 1545,
    }
}
fn rev() -> AxisRevision {
    AxisRevision {
        frequency_millihz: None,
        negative: None,
        paused: None,
        target_pulses: None,
        segments: None,
    }
}
fn segment(hz: u64) -> VecDeque<PulseSegment> {
    VecDeque::from([PulseSegment {
        duration_us: 0,
        frequency_millihz: hz * 1000,
        phase: MotionPhase::Cruise,
    }])
}
fn ceil_us(count: u64, hz: u64) -> u64 {
    ((u128::from(count) * 1_000_000).div_ceil(u128::from(hz))).min(u128::from(u64::MAX)) as u64
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IdleBudget {
    #[default]
    IncludesLastDecel,
    ExcludesLastDecel,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RatioRounding {
    #[default]
    FloorCandidate,
    NearestCandidate,
    HoldFractional,
}
#[derive(Debug, Clone)]
struct TablePart {
    hz: u64,
    count: u64,
}
#[derive(Debug, Clone)]
enum Kind {
    Table {
        parts: Vec<TablePart>,
        index: usize,
        cyclic: bool,
        deadline: u64,
    },
    Variable {
        target_hz: u64,
        gap_hz: u64,
        gap_us: u64,
        current_hz: u64,
        next: Option<u64>,
    },
    Closed {
        planned: u64,
        feedback_target: i32,
        idle: bool,
        idle_limit: u64,
        last_decel: u64,
        mask: i32,
    },
}
#[derive(Debug, Clone)]
struct Run {
    action: ExtendedMotionAction,
    kind: Kind,
    done: bool,
    paused: bool,
    stopping: bool,
}
#[derive(Debug, Clone, Default)]
pub struct MotionExtendedState {
    pub now_us: u64,
    /// Explicit firmware capability; default preserves the ordinary latched table.
    pub cyclic_table_edits: bool,
    pub idle_budget: IdleBudget,
    pub ratio_rounding: RatioRounding,
    pub notices: BTreeSet<String>,
    runs: BTreeMap<u16, Run>,
    captures: BTreeMap<(u16, String), (u64, bool)>,
}
impl MotionExtendedState {
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.runs
            .values()
            .filter(|r| !r.done && !r.paused)
            .filter_map(|r| match &r.kind {
                Kind::Table { deadline, .. } => Some(*deadline),
                Kind::Variable { next, .. } => *next,
                Kind::Closed { .. } => None,
            })
            .filter(|at| *at > self.now_us)
            .min()
    }
    pub fn reset_instruction_state(&mut self) {
        self.runs.clear();
        self.captures.clear();
        self.now_us = 0;
    }
    pub fn begin_scan(_m: &mut Memory) {}
    pub fn end_scan(_m: &mut Memory) {}
    pub fn abort(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.motion_extended);
        let ids: Vec<_> = s.runs.keys().copied().collect();
        for pc in ids {
            PulseState::release_axis(m, pc, true, s.now_us);
        }
        s.runs.clear();
        s.captures.clear();
        m.motion_extended = s;
    }
    pub fn advance(m: &mut Memory, at_us: u64) {
        let mut s = std::mem::take(&mut m.motion_extended);
        if at_us < s.now_us {
            limited(m, "Genişletilmiş hareket saati geriye gidiyor");
            m.motion_extended = s;
            return;
        }
        // Unified scheduler splits at both PulseState and this state's deadlines.
        let dt = at_us - s.now_us;
        s.now_us = at_us;
        let ids: Vec<_> = s.runs.keys().copied().collect();
        for pc in ids {
            let mut run = s.runs.remove(&pc).unwrap();
            if run.paused {
                match &mut run.kind {
                    Kind::Table { deadline, .. } => *deadline = deadline.saturating_add(dt),
                    Kind::Variable { next: Some(v), .. } => *v = v.saturating_add(dt),
                    _ => {}
                }
            }
            s.progress(m, pc, &mut run);
            s.runs.insert(pc, run);
        }
        m.motion_extended = s;
    }
    pub fn service_controls(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.motion_extended);
        s.now_us = m.pulse_output.now_us;
        let ids: Vec<_> = s.runs.keys().copied().collect();
        for pc in ids {
            let mut r = s.runs.remove(&pc).unwrap();
            s.controls(m, pc, &mut r);
            s.progress(m, pc, &mut r);
            s.runs.insert(pc, r);
        }
        m.motion_extended = s;
    }
    /// Raw edges do not bypass EI, DI, interrupt masks or handler availability.
    /// The VM reports eligible actual IRQ dispatch via feedback_interrupt.
    pub fn input_edge(m: &mut Memory, input: Device, old: bool, new: bool, at_us: u64) {
        if input.area != Area::X || old == new {
            return;
        }
        // The interrupt engine has already selected polarity and coalescing.
        // A retained Earliest event has an older timestamp and must keep its
        // original mask decision; Latest replaces both timestamp and decision.
        let events:Vec<_>=m.interrupts.pending().iter().filter_map(|(id,event)|{
            matches!(id,crate::interrupt::InterruptId::External{input:i,..} if u16::from(*i)==input.index)
                .then_some((id.to_string(),event.at_us))
        }).filter(|(_,at)|*at==at_us).collect();
        let mut s = std::mem::take(&mut m.motion_extended);
        for (&pc, run) in &s.runs {
            if run.done
                || !matches!(&run.action,ExtendedMotionAction::Closed{feedback,..} if *feedback==input)
            {
                continue;
            }
            if let Kind::Closed {
                mask,
                idle,
                planned,
                ..
            } = run.kind
            {
                let snap = m.pulse_output.axis_snapshot(pc);
                let count =
                    snap.map_or(0, |v| v.emitted)
                        .saturating_add(if idle { planned } else { 0 });
                let masked = mask_decision(m, mask, count, snap.map(|s| s.phase));
                for (id, at) in &events {
                    s.captures.insert((pc, id.clone()), (*at, !masked));
                }
            }
        }
        m.motion_extended = s;
    }
    pub fn feedback_interrupt(m: &mut Memory, id: &str, at_us: u64) {
        let mut s = std::mem::take(&mut m.motion_extended);
        s.now_us = at_us;
        let ids: Vec<_> = s.runs.keys().copied().collect();
        for pc in ids {
            let mut r = s.runs.remove(&pc).unwrap();
            if !r.done
                && let ExtendedMotionAction::Closed {
                    feedback, output, ..
                } = r.action
            {
                let matches = if feedback.area == Area::X {
                    matches!(
                        (feedback.index, id),
                        (4, "I400" | "I401") | (6, "I600" | "I601")
                    )
                } else {
                    (output.index == 0 && id == "I010") || (output.index == 2 && id == "I050")
                };
                if matches {
                    let snap = m.pulse_output.axis_snapshot(pc);
                    let (masked, reached) = if let Kind::Closed {
                        mask,
                        idle,
                        planned,
                        feedback_target,
                        ..
                    } = r.kind
                    {
                        let count = snap.map_or(0, |v| v.emitted).saturating_add(if idle {
                            planned
                        } else {
                            0
                        });
                        let masked = if feedback.area == Area::X {
                            if let Some((_, accepted)) = s.captures.remove(&(pc, id.to_string())) {
                                !accepted
                            } else if m.interrupts.active.is_some() {
                                s.notices.insert("DCLLM: bu hareket için yakalanmış X geri besleme olayı yok; eski/pending olay tutuldu".into());
                                true
                            } else {
                                mask_decision(m, mask, count, snap.map(|s| s.phase))
                            }
                        } else {
                            mask_decision(m, mask, count, snap.map(|s| s.phase))
                        };
                        let reached = feedback.area == Area::X
                            || if feedback_target < 0 {
                                m.double_word(feedback) <= feedback_target
                            } else {
                                m.double_word(feedback) >= feedback_target
                            };
                        (masked, reached)
                    } else {
                        (false, false)
                    };
                    if !masked && reached {
                        s.finish(m, pc, &mut r);
                    }
                }
            }
            s.runs.insert(pc, r);
        }
        m.motion_extended = s;
    }
    pub fn change_frequency(
        m: &mut Memory,
        frequency: Device,
        gap: &CalendarValue,
        output: Device,
        at_us: u64,
    ) -> bool {
        let mut s = std::mem::take(&mut m.motion_extended);
        s.now_us = at_us;
        let found=s.runs.iter().find_map(|(&pc,r)|matches!(&r.action,ExtendedMotionAction::Variable{frequency:f,gap:g,output:o,..} if *f==frequency && g==gap && *o==output && !r.done).then_some(pc));
        if let Some(pc) = found {
            let mut r = s.runs.remove(&pc).unwrap();
            s.visit(m, pc, &mut r, true);
            s.runs.insert(pc, r);
        }
        m.motion_extended = s;
        found.is_some()
    }
    /// REF refreshes only qualified table owners for the requested pulse channel.
    pub fn refresh_table(m: &mut Memory, channel: u8) {
        let mut s = std::mem::take(&mut m.motion_extended);
        let ids: Vec<_> = s
            .runs
            .iter()
            .filter_map(|(&pc, r)| {
                (r.action.dest().index == u16::from(channel)
                    && matches!(r.kind, Kind::Table { .. }))
                .then_some(pc)
            })
            .collect();
        for pc in ids {
            let mut r = s.runs.remove(&pc).unwrap();
            s.refresh_parts(m, &mut r);
            s.runs.insert(pc, r);
        }
        m.motion_extended = s;
    }
    fn validate_output(&self, m: &Memory, a: &ExtendedMotionAction) -> Result<(), String> {
        let y = a.dest().index;
        if m.pulse_output.model == PulseModel::Ec5
            || y >= m.refresh_io.profile.output_points
            || ![0, 2].contains(&y)
                && !(m.pulse_output.model == PulseModel::Se2
                    && matches!(
                        a,
                        ExtendedMotionAction::Table { .. } | ExtendedMotionAction::Variable { .. }
                    )
                    && [4, 6].contains(&y))
        {
            return Err("Komut/çıkış seçilen CPU modelinde belgelenmedi".into());
        }
        Ok(())
    }
    fn hz(&self, m: &Memory, v: i32) -> Result<u64, String> {
        if v < 0 {
            return Err("Negatif pulse frekansının anlamı belgelenmedi".into());
        }
        let max = if m.pulse_output.model == PulseModel::Ss2 {
            10_000
        } else {
            100_000
        };
        Ok((v as u64).min(max))
    }
    fn table(m: &Memory, source: Device, control: Device) -> Result<Vec<TablePart>, String> {
        if !span(m, control, 2) {
            return Err("DPTPO kontrol çifti model sınırını aşıyor".into());
        }
        let n = m.word(control);
        if !(1..=40).contains(&n) {
            return Err("DPTPO segment sayısı1..40 olmalı".into());
        }
        if !span(m, source, n as u32 * 4) {
            return Err("DPTPO bütün tablo aralığı model sınırını aşıyor".into());
        }
        let mut out = Vec::new();
        for i in 0..n as u16 {
            let hz = m.double_word(d(Area::D, source.index + 4 * i));
            let count = m.double_word(d(Area::D, source.index + 4 * i + 2));
            if hz <= 0 || count <= 0 {
                return Err(
                    "DPTPO sıfır/negatif segment Hz veya pulse sayısı bu PDF'de tanımlı değil"
                        .into(),
                );
            }
            let max = if m.pulse_output.model == PulseModel::Ss2 {
                10_000
            } else {
                100_000
            };
            out.push(TablePart {
                hz: (hz as u64).min(max),
                count: count as u64,
            });
        }
        Ok(out)
    }
    fn gap(m: &Memory, v: &CalendarValue) -> Result<(u64, u64), String> {
        let packed = read(m, v)? as u32;
        Ok((
            (packed & 65535).clamp(1, 32767) as u64,
            u64::from((packed >> 16).clamp(1, 100)) * 1000,
        ))
    }
    // Keep the axis request fields explicit at the five motion transitions.
    #[allow(clippy::too_many_arguments)]
    fn submit(
        &self,
        m: &mut Memory,
        pc: u16,
        output: Device,
        negative: bool,
        count: Option<u64>,
        segments: VecDeque<PulseSegment>,
        single: bool,
    ) -> Result<(), String> {
        let y = output.index as u8;
        let q = AxisRequest {
            owner_pc: pc,
            channel: y,
            direction: d(Area::Y, output.index + 1),
            negative,
            invert_direction: m.system_bit(if y == 0 { 1305 } else { 1306 }),
            target_pulses: count,
            segments,
            completion: false,
            paused: m.system_bit(pause(output)),
        };
        if single {
            PulseState::submit_single_axis(m, q, self.now_us)
        } else {
            PulseState::submit_axis(m, q, self.now_us)
        }
    }
    fn start(
        &mut self,
        m: &mut Memory,
        action: ExtendedMotionAction,
        pc: u16,
    ) -> Result<Run, String> {
        self.validate_output(m, &action)?;
        let mut run = Run {
            action: action.clone(),
            kind: Kind::Closed {
                planned: 0,
                feedback_target: 0,
                idle: false,
                idle_limit: 0,
                last_decel: 0,
                mask: 0,
            },
            done: false,
            paused: m.system_bit(pause(action.dest())),
            stopping: false,
        };
        match &action {
            ExtendedMotionAction::Table {
                source,
                control,
                output,
            } => {
                let parts = Self::table(m, *source, *control)?;
                let p = &parts[0];
                self.submit(m, pc, *output, false, Some(p.count), segment(p.hz), true)?;
                run.kind = Kind::Table {
                    deadline: self.now_us.saturating_add(ceil_us(p.count, p.hz)),
                    parts,
                    index: 0,
                    cyclic: m.system_bit(1262),
                };
                m.set_word(d(Area::D, control.index + 1), 1);
            }
            ExtendedMotionAction::Variable {
                frequency,
                count,
                gap,
                output,
            } => {
                if !span(m, *frequency, 2) {
                    return Err("DVSPO frekans çifti model sınırını aşıyor".into());
                }
                let target = self.hz(m, m.double_word(*frequency))?;
                let count = read(m, count)?;
                let (gap_hz, gap_us) = Self::gap(m, gap)?;
                let first = target.min(gap_hz);
                let direction_mode = m.word(d(Area::D, 1220 + output.index / 2)) == 1;
                self.submit(
                    m,
                    pc,
                    *output,
                    count < 0 && direction_mode,
                    if count == 0 {
                        None
                    } else {
                        Some(u64::from(count.unsigned_abs()))
                    },
                    segment(first),
                    !direction_mode,
                )?;
                run.kind = Kind::Variable {
                    target_hz: target,
                    gap_hz,
                    gap_us,
                    current_hz: first,
                    next: (first != target).then_some(self.now_us.saturating_add(gap_us)),
                };
                if gap.constant().is_some() {
                    self.notices.insert(
                        "DVSPO sabit gap: lowWORD Hz/highWORD ms açık yerel paketleme adayı".into(),
                    );
                }
            }
            ExtendedMotionAction::Closed {
                feedback,
                target,
                frequency,
                output,
            } => {
                if feedback.area == Area::X
                    && ((feedback.index == 4 && output.index != 0)
                        || (feedback.index == 6 && output.index != 2)
                        || feedback.index >= m.refresh_io.profile.input_points)
                {
                    return Err("DCLLM X4/Y0 veya X6/Y2 eşleşmesi gerekli".into());
                }
                let target = read(m, target)?;
                let peak = self.hz(m, read(m, frequency)?.max(6))?;
                let ratio = m.word(d(Area::D, if output.index == 0 { 1131 } else { 1132 }));
                if !(1..=10000).contains(&ratio) {
                    return Err("DCLLM oran D1131/D1132 için1..10000 gerekli".into());
                }
                let numerator = u64::from(target.unsigned_abs()) * ratio as u64;
                if !numerator.is_multiple_of(100)
                    && self.ratio_rounding == RatioRounding::HoldFractional
                {
                    return Err("DCLLM kesirli oran yuvarlaması belgelenmedi".into());
                }
                let planned = if self.ratio_rounding == RatioRounding::NearestCandidate {
                    (numerator + 50) / 100
                } else {
                    numerator / 100
                };
                self.notices.insert("DCLLM: açık yerel20basamak/ratio yuvarlama profili; fiziksel/native algoritma eşitliği değildir".into());
                let y = output.index;
                let start = m.word(d(Area::D, if y == 0 { 1340 } else { 1352 })).max(6) as u64;
                let up = m.word(d(Area::D, if y == 0 { 1343 } else { 1353 })).max(20) as u64 * 1000;
                let down = if m.system_bit(if y == 0 { 1534 } else { 1535 }) {
                    m.word(d(Area::D, if y == 0 { 1348 } else { 1349 })).max(20) as u64 * 1000
                } else {
                    up
                };
                let plan = PulseState::axis_step_plan(start, peak, up, down, planned, 20);
                let last_decel = plan.back().map_or(0, |p| {
                    ((u128::from(p.frequency_millihz) * u128::from(p.duration_us)) / 1_000_000_000)
                        as u64
                });
                let idle = m.word(d(Area::D, if y == 0 { 1244 } else { 1245 }));
                let mask = if m.system_bit(if y == 0 { 1156 } else { 1158 }) {
                    m.double_word(d(Area::D, if y == 0 { 1026 } else { 1135 }))
                } else {
                    0
                };
                self.submit(m, pc, *output, target < 0, Some(planned), plan, false)?;
                run.kind = Kind::Closed {
                    planned,
                    feedback_target: target,
                    idle: false,
                    idle_limit: if feedback.area == Area::X && idle > 0 {
                        idle as u64
                    } else {
                        0
                    },
                    last_decel,
                    mask,
                };
            }
        }
        m.set_system(completion(action.dest()), false);
        m.set_system(status(action.dest()), false);
        Ok(run)
    }
    fn refresh_parts(&mut self, m: &mut Memory, r: &mut Run) {
        if !self.cyclic_table_edits {
            return;
        }
        if let ExtendedMotionAction::Table {
            source, control, ..
        } = r.action
            && let Kind::Table {
                parts,
                index,
                cyclic,
                ..
            } = &mut r.kind
            && *cyclic
            && parts.len() >= 3
        {
            match Self::table(m, source, control) {
                Ok(new) if new.len() == parts.len() => {
                    for (i, p) in new.into_iter().enumerate() {
                        if i != *index && i != (*index + 1) % parts.len() {
                            parts[i] = p;
                        }
                    }
                }
                Ok(_) => {
                    self.notices.insert(
                        "DPTPO çalışma sırasında segment sayısı değişikliği uygulanmadı".into(),
                    );
                }
                Err(e) => limited(m, e),
            }
        }
    }
    fn controls(&mut self, m: &mut Memory, pc: u16, r: &mut Run) {
        if r.done {
            return;
        }
        let output = r.action.dest();
        let paused = m.system_bit(pause(output));
        if paused != r.paused {
            let mut change = rev();
            change.paused = Some(paused);
            if let Err(e) = PulseState::revise_axis(m, pc, change, self.now_us) {
                limited(m, e);
            }
            r.paused = paused;
        }
        m.set_system(
            status(output),
            paused || matches!(r.kind, Kind::Variable { current_hz: 0, .. }),
        );
        if matches!(r.kind, Kind::Closed { .. }) && m.system_bit(ramp_stop(output)) && !r.stopping {
            let hz = m
                .pulse_output
                .axis_snapshot(pc)
                .map_or(0, |s| s.frequency_millihz / 1000);
            let y = output.index;
            let ms = m.word(d(Area::D, if y == 0 { 1348 } else { 1349 })).max(20) as u64;
            let mut segs = VecDeque::new();
            for i in 1..=20 {
                segs.push_back(PulseSegment {
                    duration_us: ms * 1000 / 20,
                    frequency_millihz: hz * (20 - i) / 20 * 1000,
                    phase: MotionPhase::RampDown,
                });
            }
            segs.push_back(PulseSegment {
                duration_us: 0,
                frequency_millihz: 0,
                phase: MotionPhase::Cruise,
            });
            let mut c = rev();
            c.target_pulses = Some(None);
            c.segments = Some(segs);
            if let Err(e) = PulseState::revise_axis(m, pc, c, self.now_us) {
                limited(m, e);
            }
            r.stopping = true;
            self.notices.insert("DCLLM ramp-stop açık sanal20basamak profili; tekrar başlatma için contactOFF gerekli".into());
        }
    }
    fn visit(&mut self, m: &mut Memory, pc: u16, r: &mut Run, immediate: bool) {
        self.controls(m, pc, r);
        self.refresh_parts(m, r);
        if let ExtendedMotionAction::Table { control, .. } = r.action
            && let Kind::Table { index, .. } = r.kind
        {
            m.set_word(d(Area::D, control.index + 1), index as i16 + 1);
        }
        if let ExtendedMotionAction::Variable {
            frequency, ref gap, ..
        } = r.action
            && let Kind::Variable {
                target_hz,
                gap_hz,
                gap_us,
                current_hz,
                next,
            } = &mut r.kind
        {
            match self
                .hz(m, m.double_word(frequency))
                .and_then(|hz| Ok((hz, Self::gap(m, gap)?)))
            {
                Ok((hz, (step, time))) if hz != *target_hz => {
                    *target_hz = hz;
                    *gap_hz = step;
                    *gap_us = time;
                    if immediate {
                        *current_hz = step_towards(*current_hz, hz, step);
                        let mut c = rev();
                        c.frequency_millihz = Some(*current_hz * 1000);
                        if let Err(e) = PulseState::revise_axis(m, pc, c, self.now_us) {
                            limited(m, e);
                        }
                    }
                    *next = (*current_hz != hz).then_some(self.now_us.saturating_add(time));
                }
                Ok(_) if immediate && *current_hz != *target_hz => {
                    *current_hz = step_towards(*current_hz, *target_hz, *gap_hz);
                    let mut c = rev();
                    c.frequency_millihz = Some(*current_hz * 1000);
                    if let Err(e) = PulseState::revise_axis(m, pc, c, self.now_us) {
                        limited(m, e);
                    }
                    *next =
                        (*current_hz != *target_hz).then_some(self.now_us.saturating_add(*gap_us));
                }
                Ok(_) => {}
                Err(e) => limited(m, e),
            }
        }
        self.progress(m, pc, r);
    }
    fn finish(&mut self, m: &mut Memory, pc: u16, r: &mut Run) {
        PulseState::finish_axis(m, pc, self.now_us);
        r.done = true;
        m.set_system(completion(r.action.dest()), true);
        let y = r.action.dest().index;
        if m.system_bit(if y == 0 { 1347 } else { 1524 }) {
            m.set_system(if y == 0 { 1347 } else { 1524 }, false);
        }
    }
    fn progress(&mut self, m: &mut Memory, pc: u16, r: &mut Run) {
        if r.done || r.paused {
            return;
        }
        let Some(snap) = m.pulse_output.axis_snapshot(pc) else {
            return;
        };
        match &mut r.kind {
            Kind::Table {
                parts,
                index,
                cyclic,
                deadline,
            } => {
                if !snap.done {
                    return;
                }
                if *index + 1 == parts.len() {
                    if !*cyclic {
                        self.finish(m, pc, r);
                        return;
                    }
                    *index = 0;
                    if let ExtendedMotionAction::Table {
                        source, control, ..
                    } = r.action
                        && self.cyclic_table_edits
                    {
                        match Self::table(m, source, control) {
                            Ok(p) => *parts = p,
                            Err(e) => {
                                limited(m, e);
                                return;
                            }
                        }
                    }
                } else {
                    *index += 1;
                }
                let p = &parts[*index];
                PulseState::release_axis(m, pc, false, self.now_us);
                if let Err(e) = self.submit(
                    m,
                    pc,
                    r.action.dest(),
                    false,
                    Some(p.count),
                    segment(p.hz),
                    true,
                ) {
                    limited(m, e);
                    r.done = true;
                    return;
                }
                *deadline = self.now_us.saturating_add(ceil_us(p.count, p.hz));
            }
            Kind::Variable {
                target_hz,
                gap_hz,
                gap_us,
                current_hz,
                next,
            } => {
                if snap.done {
                    self.finish(m, pc, r);
                    return;
                }
                if next.is_some_and(|v| v <= self.now_us) {
                    *current_hz = step_towards(*current_hz, *target_hz, *gap_hz);
                    let mut c = rev();
                    c.frequency_millihz = Some(*current_hz * 1000);
                    if let Err(e) = PulseState::revise_axis(m, pc, c, self.now_us) {
                        limited(m, e);
                    }
                    *next =
                        (*current_hz != *target_hz).then_some(self.now_us.saturating_add(*gap_us));
                    m.set_system(status(r.action.dest()), *current_hz == 0);
                }
            }
            Kind::Closed {
                feedback_target,
                idle,
                idle_limit,
                last_decel,
                ..
            } => {
                if r.stopping {
                    return;
                }
                if snap.done && !*idle {
                    *idle = true;
                    let out = r.action.dest();
                    let start = m
                        .word(d(Area::D, if out.index == 0 { 1340 } else { 1352 }))
                        .max(6) as u64;
                    let budget = if *idle_limit == 0 {
                        None
                    } else {
                        Some(if self.idle_budget == IdleBudget::IncludesLastDecel {
                            idle_limit.saturating_sub(*last_decel)
                        } else {
                            *idle_limit
                        })
                    };
                    PulseState::release_axis(m, pc, false, self.now_us);
                    if let Err(e) = self.submit(
                        m,
                        pc,
                        out,
                        *feedback_target < 0,
                        budget,
                        segment(start),
                        false,
                    ) {
                        limited(m, e);
                        r.done = true;
                    }
                    if budget == Some(0) {
                        self.finish(m, pc, r);
                    }
                } else if *idle && snap.done {
                    self.finish(m, pc, r);
                }
            }
        }
    }
}
fn mask_decision(m: &mut Memory, mask: i32, count: u64, phase: Option<MotionPhase>) -> bool {
    if mask >= 0 {
        return mask > 0 && count <= mask as u64;
    }
    if m.pulse_output.model == PulseModel::Ss2 {
        limited(m, "SS2 negatif DCLLM bölüm maskesi desteklenmiyor");
        return true;
    }
    match mask {
        -1 => phase == Some(MotionPhase::RampUp),
        -2 => matches!(phase, Some(MotionPhase::RampUp | MotionPhase::Cruise)),
        -3 => true,
        _ => {
            limited(m, "DCLLM negatif mask değeri belgelenmedi");
            true
        }
    }
}
fn step_towards(current: u64, target: u64, gap: u64) -> u64 {
    if current < target {
        current.saturating_add(gap).min(target)
    } else {
        current.saturating_sub(gap).max(target)
    }
}
#[cfg(test)]
#[path = "motion_extended_tests.rs"]
mod tests;
