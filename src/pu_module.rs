//! Explicit virtual DVP02PU-E2/SL modules. CPU Y/X are not module pins.
//! Instruction WORD lengths come from the PDF; no WPL opcode is invented.
use crate::calendar_operand::CalendarValue;
use crate::file_memory::FirmwareVersion;
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[path = "pu_axis.rs"]
mod axis_math;
use axis_math::{AxisRun, HomePhase, RunMode};
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn limitation(m: &mut Memory, s: impl Into<String>) {
    m.execution_limitations.insert(s.into());
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PuKind {
    Configure,
    Status,
    Pulse,
    Relative,
    Absolute,
    Home,
    Jog,
    Mpg,
    Counter,
    Input,
    Limits,
}
impl PuKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Configure => "DPUCONF",
            Self::Status => "PUSTAT",
            Self::Pulse => "DPUPLS",
            Self::Relative => "DPUDRI",
            Self::Absolute => "DPUDRA",
            Self::Home => "DPUZRN",
            Self::Jog => "DPUJOG",
            Self::Mpg => "DPUMPG",
            Self::Counter => "DPUCNT",
            Self::Input => "PUX",
            Self::Limits => "DPULS",
        }
    }
    fn specs(self) -> &'static str {
        match self {
            Self::Configure => "vvvvvvVvvbbd",
            Self::Status => "vvidbbbd",
            Self::Pulse | Self::Relative | Self::Absolute => "vvVVbbd",
            Self::Home => "vvvVvbbd",
            Self::Jog => "vvVbbd",
            Self::Mpg => "vvvDDDD Dbd",
            Self::Counter => "vvviDDbd",
            Self::Input => "vvvvbbd",
            Self::Limits => "vvVVbbd",
        }
    }
    pub fn words(self) -> u16 {
        match self {
            Self::Configure => 49,
            Self::Status => 17,
            Self::Pulse | Self::Relative | Self::Absolute | Self::Limits => 29,
            Self::Home | Self::Counter => 33,
            Self::Jog => 25,
            Self::Mpg => 41,
            Self::Input => 15,
        }
    }
    fn axis(self) -> bool {
        !matches!(self, Self::Counter | Self::Input)
    }
    fn motion(self) -> bool {
        matches!(
            self,
            Self::Pulse | Self::Relative | Self::Absolute | Self::Home | Self::Jog | Self::Mpg
        )
    }
    fn encoder(self) -> bool {
        matches!(self, Self::Mpg | Self::Counter)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum PuOperand {
    Value(CalendarValue),
    Input(Device),
    Data(Device),
    Bit(Device),
}
#[derive(Debug, Clone, PartialEq)]
pub struct PuAction {
    pub kind: PuKind,
    pub args: Vec<PuOperand>,
}
impl PuAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let kind =
            match name {
                "DPUCONF" => PuKind::Configure,
                "PUSTAT" => PuKind::Status,
                "DPUPLS" => PuKind::Pulse,
                "DPUDRI" => PuKind::Relative,
                "DPUDRA" => PuKind::Absolute,
                "DPUZRN" => PuKind::Home,
                "DPUJOG" => PuKind::Jog,
                "DPUMPG" => PuKind::Mpg,
                "DPUCNT" => PuKind::Counter,
                "PUX" => PuKind::Input,
                "DPULS" => PuKind::Limits,
                _ => return Err(
                    "PU temel komut adı gerekli; yalnız PUX/DPULS için dış P sarmalayıcısı vardır"
                        .into(),
                ),
            };
        let spec: Vec<_> = kind
            .specs()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if spec.len() != args.len() {
            return Err(format!("{name} {} operand gerektirir", spec.len()));
        }
        let mut operands = Vec::new();
        for (raw, spec) in args.iter().zip(spec) {
            operands.push(match spec {
                'v' | 'V' => {
                    let value = CalendarValue::parse(raw, spec == 'V')?;
                    if !raw.starts_with(['K', 'H', 'D'])
                        || value.base().is_some_and(|v| v.area != Area::D)
                    {
                        return Err("PU sayısal matrisi yalnız K/H/D".into());
                    }
                    PuOperand::Value(value)
                }
                'd' | 'D' => {
                    let v: Device = raw.parse()?;
                    if v.area != Area::D || spec == 'D' && v.index == 9999 {
                        return Err("PU veri hedefi doğrudan D,32bit hedef tam çift olmalı".into());
                    }
                    PuOperand::Data(v)
                }
                'b' => {
                    let v: Device = raw.parse()?;
                    if !matches!(v.area, Area::Y | Area::M | Area::S) || !v.writable_bit() {
                        return Err("PU durum/çıkış operandı yazılabilir Y/M/S olmalı".into());
                    }
                    PuOperand::Bit(v)
                }
                'i' => {
                    let v: Device = raw.parse()?;
                    if !matches!(v.area, Area::X | Area::Y | Area::M | Area::S | Area::D) {
                        return Err("PU ZeroS yalnız X/Y/M/S/D".into());
                    }
                    PuOperand::Input(v)
                }
                _ => unreachable!(),
            });
        }
        Ok(Self {
            kind,
            args: operands,
        })
    }
    pub fn name(&self) -> &'static str {
        self.kind.name()
    }
    pub fn header(&self) -> Option<u16> {
        Some(match self.kind {
            PuKind::Configure => 0x0900,
            PuKind::Status => 0x0c00,
            PuKind::Pulse => 0xd700,
            PuKind::Relative => 0xd900,
            PuKind::Absolute => 0xdb00,
            PuKind::Home => 0xdd00,
            PuKind::Jog => 0xdf00,
            PuKind::Mpg => 0xe300,
            PuKind::Counter => 0xe500,
            PuKind::Input => 0xe800,
            PuKind::Limits => 0xeb00,
        })
    }
    pub fn words(&self) -> u16 {
        self.kind.words()
    }
    pub fn wide(&self) -> bool {
        !matches!(self.kind, PuKind::Status | PuKind::Input)
    }
    pub fn operands(&self) -> Vec<PuOperand> {
        self.args.clone()
    }
    pub fn dest(&self) -> Device {
        if self.kind == PuKind::Status {
            self.device(3)
        } else {
            self.device(self.args.len() - 1)
        }
    }
    pub fn label(&self) -> String {
        format!(
            "{} {}",
            self.name(),
            self.args
                .iter()
                .map(|a| match a {
                    PuOperand::Value(v) => v.to_string(),
                    PuOperand::Input(v) | PuOperand::Data(v) | PuOperand::Bit(v) => v.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
    pub fn display_value(&self, m: &Memory) -> String {
        format!(
            "PU modules:{} / {}={}",
            m.pu_module.modules.len(),
            self.dest(),
            m.word(self.dest())
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for (i, a) in self.args.iter().enumerate() {
            match a {
                PuOperand::Value(v) => v.devices(out),
                PuOperand::Data(v) => {
                    out.insert(*v);
                    if self.data_wide(i) && v.index < 9999 {
                        out.insert(d(v.index + 1));
                    }
                }
                PuOperand::Input(v) | PuOperand::Bit(v) => {
                    out.insert(*v);
                }
            }
        }
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let mut s = std::mem::take(&mut m.pu_module);
        s.visit(m, self, power, pc);
        m.pu_module = s;
    }
    fn device(&self, i: usize) -> Device {
        match self.args[i] {
            PuOperand::Input(v) | PuOperand::Data(v) | PuOperand::Bit(v) => v,
            _ => unreachable!(),
        }
    }
    fn value(&self, m: &Memory, i: usize) -> i32 {
        match &self.args[i] {
            PuOperand::Value(v) => v.read(m),
            PuOperand::Input(v) => {
                if v.area == Area::D {
                    i32::from(m.word(*v) != 0)
                } else {
                    i32::from(m.bit(*v))
                }
            }
            _ => unreachable!(),
        }
    }
    fn data_wide(&self, i: usize) -> bool {
        match self.kind {
            PuKind::Status => i == 3,
            PuKind::Mpg => (3..=7).contains(&i),
            PuKind::Counter => i == 4 || i == 5,
            _ => false,
        }
    }
    fn validate(&self, m: &Memory) -> Result<(), String> {
        for (i, a) in self.args.iter().enumerate() {
            if self.data_wide(i) && matches!(a,PuOperand::Data(v) if v.index==9999) {
                return Err("PU32bit veri hedefi tamDçifti olmalı".into());
            }
        }
        let mut ds = BTreeSet::new();
        self.devices(&mut ds);
        for v in ds {
            let max = match v.area {
                Area::D => m.data_register_limit.unwrap_or(9999),
                Area::Y => m
                    .refresh_io
                    .profile
                    .output_points
                    .saturating_sub(1)
                    .min(255),
                Area::X => m.refresh_io.profile.input_points.saturating_sub(1).min(255),
                Area::M => m.relay_limit.unwrap_or(4095),
                Area::S => 1023,
                _ => 255,
            };
            if v.index > max {
                return Err("PU tam operand aralığı model belleğini aşıyor".into());
            }
        }
        Ok(())
    }
    fn error(&self, m: &mut Memory, code: u16) {
        m.set_internal(self.device(self.args.len() - 2), true);
        m.set_word(self.device(self.args.len() - 1), code as i16);
    }
    fn success(&self, m: &mut Memory) {
        if !matches!(
            self.kind,
            PuKind::Status | PuKind::Mpg | PuKind::Counter | PuKind::Jog
        ) {
            m.set_internal(self.device(self.args.len() - 3), true);
        }
        m.set_internal(self.device(self.args.len() - 2), false);
        m.set_word(self.device(self.args.len() - 1), 0);
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PuCpuModel {
    #[default]
    Disabled,
    Es2,
    Es2E,
    Sa2,
    Sx2,
    Se,
    Se2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JogDisable {
    #[default]
    ImmediateParagraph,
    DecelerateFigure,
}
#[derive(Debug, Clone, Copy)]
pub struct PuProfile {
    pub model: PuCpuModel,
    pub firmware: FirmwareVersion,
    pub command_timeout_us: u64,
    pub jog_disable: JogDisable,
}
impl Default for PuProfile {
    fn default() -> Self {
        Self {
            model: PuCpuModel::Disabled,
            firmware: FirmwareVersion::new(1, 0),
            command_timeout_us: 100000,
            jog_disable: JogDisable::ImmediateParagraph,
        }
    }
}
impl PuProfile {
    fn validate(self) -> Result<(), String> {
        let min = match self.model {
            PuCpuModel::Disabled => {
                return Err("PU açık CPU profili ve yapılandırılmış modül gerektirir".into());
            }
            PuCpuModel::Es2 => FirmwareVersion::new(4, 2),
            PuCpuModel::Es2E => FirmwareVersion::new(1, 48),
            PuCpuModel::Sa2 | PuCpuModel::Sx2 => FirmwareVersion::new(3, 4),
            PuCpuModel::Se => FirmwareVersion::new(2, 14),
            PuCpuModel::Se2 => FirmwareVersion::new(1, 0),
        };
        if self.firmware < min || self.firmware.minor > 99 {
            Err("PU komutları bu CPU firmware sürümünde belgelenmedi".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PuModuleKind {
    E2,
    Sl,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PuExchange {
    External,
    Virtual { latency_us: u64 },
}
#[derive(Debug, Clone)]
pub struct PuAxisConfig {
    pub mode: u8,
    pub start_hz: u32,
    pub acceleration_ms: u32,
    pub deceleration_ms: u32,
    pub maximum_hz: u32,
    pub z_count: i32,
    pub offset: i32,
    pub negative_limit: i32,
    pub positive_limit: i32,
}
impl Default for PuAxisConfig {
    fn default() -> Self {
        Self {
            mode: 1,
            start_hz: 100,
            acceleration_ms: 100,
            deceleration_ms: 100,
            maximum_hz: 100000,
            z_count: 0,
            offset: 0,
            negative_limit: 0,
            positive_limit: 0,
        }
    }
}
#[derive(Debug, Clone)]
pub struct PuAxis {
    pub config: PuAxisConfig,
    pub position: i32,
    pub speed_hz: f64,
    pub busy: bool,
    pub paused: bool,
    pub error: u16,
    pub outputs: [bool; 2],
    /// Explicit module feedback; integer wrap alone is not hardware overflow evidence.
    pub position_overflow: bool,
    state_done: bool,
    state_negative: bool,
    owner: Option<u16>,
    run: Option<AxisRun>,
}
impl Default for PuAxis {
    fn default() -> Self {
        Self {
            config: Default::default(),
            position: 0,
            speed_hz: 0.,
            busy: false,
            paused: false,
            error: 0,
            outputs: [false; 2],
            position_overflow: false,
            state_done: false,
            state_negative: false,
            owner: None,
            run: None,
        }
    }
}
#[derive(Debug, Clone)]
pub struct PuInput {
    pub mode: u8,
    pub filter_ms: u8,
    pub raw: bool,
    pub level: bool,
    pending: Option<(u64, bool)>,
}
impl Default for PuInput {
    fn default() -> Self {
        Self {
            mode: 0,
            filter_ms: 10,
            raw: false,
            level: false,
            pending: None,
        }
    }
}
#[derive(Debug, Clone, Default)]
struct Encoder {
    owner: Option<u16>,
    mode: u16,
    count: i32,
    sample: i32,
    previous: i32,
    next: Option<u64>,
    period_us: u64,
    a: bool,
    b: bool,
    mpg: bool,
}
#[derive(Debug, Clone)]
pub struct VirtualPuModule {
    pub kind: PuModuleKind,
    pub firmware: FirmwareVersion,
    pub exchange: PuExchange,
    pub responding: bool,
    pub axes: [PuAxis; 2],
    pub inputs: [PuInput; 5],
    encoder: Encoder,
    bus_ready: u64,
}
impl VirtualPuModule {
    pub fn e2() -> Self {
        Self {
            kind: PuModuleKind::E2,
            firmware: FirmwareVersion::new(1, 4),
            exchange: PuExchange::Virtual { latency_us: 1 },
            responding: true,
            axes: std::array::from_fn(|_| PuAxis::default()),
            inputs: std::array::from_fn(|_| PuInput::default()),
            encoder: Encoder::default(),
            bus_ready: 0,
        }
    }
    pub fn sl() -> Self {
        Self {
            kind: PuModuleKind::Sl,
            ..Self::e2()
        }
    }
    pub fn counter(&self) -> i32 {
        self.encoder.count
    }
    pub fn period_count(&self) -> i32 {
        self.encoder.sample
    }
    pub fn output(&self, pin: u8) -> bool {
        self.axes
            .get(usize::from(pin / 2))
            .is_some_and(|a| a.outputs[usize::from(pin % 2)])
    }
}
#[derive(Debug, Clone)]
pub struct PuRequest {
    pub id: u64,
    pub pc: u16,
    pub slot: u16,
    pub axis: Option<u8>,
    pub at_us: u64,
    pub action: PuKind,
}
#[derive(Debug, Clone, Copy)]
pub enum PuReply {
    Ack,
    Fault(u16),
}
#[derive(Debug, Clone)]
enum Event {
    Input(u16, u8, bool),
    Encoder(u16, bool, bool),
    Delta(u16, i32),
    Reply(u64, PuReply),
}
#[derive(Debug, Clone)]
struct Binding {
    action: PuAction,
    slot: u16,
    axis: Option<usize>,
    active: bool,
    applied: bool,
    stopping: bool,
    last_zero: bool,
    done: bool,
    values: Vec<i32>,
    id: u64,
    timeout: u64,
}
#[derive(Debug, Clone, Default)]
pub struct PuState {
    pub profile: PuProfile,
    pub now_us: u64,
    pub notices: BTreeSet<String>,
    modules: BTreeMap<u16, VirtualPuModule>,
    bindings: BTreeMap<u16, Binding>,
    events: BTreeMap<(u64, u64), Event>,
    requests: VecDeque<PuRequest>,
    next_id: u64,
    sequence: u64,
}
impl PuState {
    /// Select a fresh CPU profile and its documented power-on mapping relay.
    /// Changing the public profile field directly does not reset CPU memory.
    pub fn set_profile(m: &mut Memory, profile: PuProfile) -> Result<(), String> {
        profile.validate()?;
        if m.pu_module
            .bindings
            .values()
            .any(|b| b.active || b.stopping)
        {
            return Err("Etkin PU işlemleri varken CPU profili değiştirilemez".into());
        }
        m.pu_module.profile = profile;
        Self::apply_power_on_relays(m);
        Ok(())
    }
    pub(crate) fn apply_power_on_relays(m: &mut Memory) {
        match m.pu_module.profile.model {
            PuCpuModel::Disabled => {}
            PuCpuModel::Es2 | PuCpuModel::Es2E => m.set_system(1183, false),
            PuCpuModel::Sa2 | PuCpuModel::Sx2 | PuCpuModel::Se | PuCpuModel::Se2 => {
                m.set_system(1183, true);
            }
        }
    }
    pub fn configure(&mut self, slot: u16, module: VirtualPuModule) -> Result<(), String> {
        if !((slot <= 7 && module.kind == PuModuleKind::E2)
            || ((100..=107).contains(&slot) && module.kind == PuModuleKind::Sl))
        {
            return Err("PU sağE2slot0..7 veya solSLslot100..107 gerekli".into());
        }
        if self.bindings.values().any(|b| b.slot == slot && b.active) {
            return Err("Etkin PU modülü değiştirilemez".into());
        }
        self.modules.insert(slot, module);
        Ok(())
    }
    pub fn module(&self, slot: u16) -> Option<&VirtualPuModule> {
        self.modules.get(&slot)
    }
    pub fn module_mut(&mut self, slot: u16) -> Option<&mut VirtualPuModule> {
        self.modules.get_mut(&slot)
    }
    pub fn position(&self, slot: u16, axis: u8) -> Option<i32> {
        self.modules
            .get(&slot)?
            .axes
            .get(usize::from(axis.checked_sub(1)?))
            .map(|a| a.position)
    }
    pub fn output(&self, slot: u16, pin: u8) -> bool {
        pin < 4 && self.modules.get(&slot).is_some_and(|m| m.output(pin))
    }
    pub fn outgoing(&self) -> &VecDeque<PuRequest> {
        &self.requests
    }
    pub fn pop_request(&mut self) -> Option<PuRequest> {
        self.requests.pop_front()
    }
    fn queue(&mut self, at: u64, event: Event) -> Result<(), String> {
        if at < self.now_us {
            return Err("PU olay zamanı geriye gidemez".into());
        }
        self.sequence = self.sequence.checked_add(1).ok_or("PU olay sayacı taştı")?;
        self.events.insert((at, self.sequence), event);
        Ok(())
    }
    pub fn queue_input(
        &mut self,
        slot: u16,
        input: u8,
        at_us: u64,
        level: bool,
    ) -> Result<(), String> {
        if input >= 5 || !self.modules.contains_key(&slot) {
            return Err("PU giriş/modül mevcut değil".into());
        }
        self.queue(at_us, Event::Input(slot, input, level))
    }
    pub fn queue_encoder_edge(
        &mut self,
        slot: u16,
        at_us: u64,
        a: bool,
        b: bool,
    ) -> Result<(), String> {
        if !self.modules.contains_key(&slot) {
            return Err("PU modülü mevcut değil".into());
        }
        self.queue(at_us, Event::Encoder(slot, a, b))
    }
    /// Explicit post-decoder counts: this seam does not invent physical edges.
    pub fn queue_encoder_delta(&mut self, slot: u16, at_us: u64, delta: i32) -> Result<(), String> {
        if !self.modules.contains_key(&slot) {
            return Err("PU modülü mevcut değil".into());
        }
        self.queue(at_us, Event::Delta(slot, delta))
    }
    pub fn queue_reply(&mut self, at_us: u64, id: u64, reply: PuReply) -> Result<(), String> {
        self.queue(at_us, Event::Reply(id, reply))
    }
    pub fn reset_instruction_state(&mut self) {
        self.bindings.clear();
        self.events.clear();
        self.requests.clear();
        self.now_us = 0;
        for m in self.modules.values_mut() {
            m.bus_ready = 0;
            m.encoder.owner = None;
            m.encoder.next = None;
            for a in &mut m.axes {
                a.owner = None;
                a.run = None;
                a.busy = false;
                a.paused = false;
                a.speed_hz = 0.;
                a.outputs = [false; 2];
            }
            for i in &mut m.inputs {
                i.pending = None;
            }
        }
    }
    pub fn abort(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.pu_module);
        let now = s.now_us;
        s.reset_instruction_state();
        s.now_us = now;
        m.pu_module = s;
    }
    pub fn begin_scan(_m: &mut Memory) {}
    pub fn end_scan(_m: &mut Memory) {}
    pub fn service_controls(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.pu_module);
        s.publish(m);
        m.pu_module = s;
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.events
            .first_key_value()
            .map(|(k, _)| k.0)
            .into_iter()
            .chain(
                self.bindings
                    .values()
                    .filter(|b| b.active && !b.applied)
                    .map(|b| b.timeout),
            )
            .chain(self.modules.values().flat_map(|m| {
                m.inputs
                    .iter()
                    .filter_map(|i| i.pending.map(|p| p.0))
                    .chain(m.encoder.next)
                    .chain(m.axes.iter().filter_map(|a| {
                        a.run.as_ref().and_then(|r| r.next_deadline(a, self.now_us))
                    }))
            }))
            .min()
    }
    pub fn advance(m: &mut Memory, at_us: u64) {
        let mut s = std::mem::take(&mut m.pu_module);
        if at_us < s.now_us {
            limitation(m, "PU saati geriye gidemez");
            m.pu_module = s;
            return;
        }
        let mut work = 0;
        loop {
            let next = s
                .next_deadline_us()
                .filter(|t| *t <= at_us)
                .unwrap_or(at_us);
            let dt = next - s.now_us;
            s.move_axes(dt);
            s.now_us = next;
            s.process(m);
            s.publish(m);
            if next == at_us {
                break;
            }
            work += 1;
            if work > 65536 {
                limitation(m, "PU olay bütçesi doldu; zaman adımı tamamlanmadı");
                break;
            }
        }
        m.pu_module = s;
    }
    fn visit(&mut self, m: &mut Memory, a: &PuAction, power: bool, pc: u16) {
        if !power {
            if let Some(mut b) = self.bindings.remove(&pc) {
                if b.action.kind == PuKind::Jog
                    && b.applied
                    && self.profile.jog_disable == JogDisable::DecelerateFigure
                    && let Some(module) = self.modules.get_mut(&b.slot)
                    && let Some(i) = b.axis
                    && module.axes[i].owner == Some(pc)
                {
                    let axis = &mut module.axes[i];
                    if let Some(mut run) = axis.run.take() {
                        run.ramp(&axis.config, 0.);
                        run.stop_on_zero = true;
                        axis.run = Some(run);
                        b.active = false;
                        b.stopping = true;
                        self.bindings.insert(pc, b);
                        return;
                    }
                }
                self.release(&b, pc);
            }
            return;
        }
        if let Some(mut b) = self.bindings.remove(&pc) {
            if b.stopping {
                if self
                    .modules
                    .get(&b.slot)
                    .and_then(|module| b.axis.map(|i| module.axes[i].busy))
                    .unwrap_or(false)
                {
                    a.error(m, 0x1405);
                    self.bindings.insert(pc, b);
                    return;
                }
            } else {
                if b.active && b.applied {
                    self.update(m, pc, &mut b);
                }
                self.bindings.insert(pc, b);
                self.publish(m);
                return;
            }
        }
        if let Err(e) = self.profile.validate().and_then(|_| a.validate(m)) {
            limitation(m, e);
            return;
        }
        let slot = a.value(m, 0);
        if !(0..=7).contains(&slot) && !(100..=107).contains(&slot) {
            a.error(m, 0x1400);
            return;
        }
        let slot = slot as u16;
        if slot <= 7 && m.module_io.read_slot(slot as u8).is_some() {
            a.error(m, 0x1400);
            limitation(
                m,
                "PU ve analog modül aynı sağ genişleme slotunu paylaşamaz",
            );
            return;
        }
        let Some(module) = self.modules.get(&slot) else {
            a.error(m, 0x1400);
            return;
        };
        let axis = if a.kind.axis() {
            let v = a.value(m, 1);
            if !(1..=2).contains(&v) {
                a.error(
                    m,
                    if matches!(a.kind, PuKind::Configure | PuKind::Limits) {
                        0x1400
                    } else {
                        0x1403
                    },
                );
                return;
            }
            Some(v as usize - 1)
        } else {
            None
        };
        if a.kind == PuKind::Home && slot >= 100 {
            limitation(
                m,
                "DPUZRN metni yalnızsağmodül derken diğer PU tabloları solu da listeler; solhoming açık doğrulama gerektirir",
            );
            return;
        }
        if a.kind == PuKind::Mpg && module.firmware < FirmwareVersion::new(1, 4) {
            a.error(m, 0x1400);
            return;
        }
        if a.kind.motion() && axis.is_some_and(|i| module.axes[i].owner.is_some())
            || a.kind.motion()
                && self
                    .bindings
                    .values()
                    .any(|b| b.active && b.slot == slot && b.axis == axis && b.action.kind.motion())
        {
            a.error(m, 0x1405);
            return;
        }
        if a.kind.encoder()
            && self
                .bindings
                .values()
                .any(|b| b.active && b.slot == slot && b.action.kind.encoder())
        {
            a.error(m, 0x1400);
            limitation(
                m,
                "DPUMPG ve DPUCNT aynı modülün encoder kaynağını birlikte kullanamaz; donanım çakışma kodu ayrıca belgelenmedi",
            );
            return;
        }
        let mut values = Vec::with_capacity(a.args.len());
        for (i, v) in a.args.iter().enumerate() {
            values.push(match v {
                PuOperand::Value(_) | PuOperand::Input(_) => a.value(m, i),
                PuOperand::Data(_) | PuOperand::Bit(_) => 0,
            });
        }
        if a.kind == PuKind::Mpg {
            let rate = f32::from_bits(m.double_word(a.device(5)) as u32);
            if !rate.is_finite() {
                limitation(
                    m,
                    "PU MPG oranı sonluREAL olmalı; NaN/Inf donanım davranışı belgelenmedi",
                );
                return;
            }
            values[5] = rate.to_bits() as i32;
        }
        let Some(id) = self.next_id.checked_add(1) else {
            limitation(m, "PU istek kimliği tükendi");
            return;
        };
        self.next_id = id;
        let timeout = self
            .now_us
            .saturating_add(self.profile.command_timeout_us.max(1));
        let b = Binding {
            action: a.clone(),
            slot,
            axis,
            active: true,
            applied: false,
            stopping: false,
            last_zero: false,
            done: false,
            values,
            id,
            timeout,
        };
        let module = self.modules.get_mut(&slot).unwrap();
        let due = match module.exchange {
            PuExchange::Virtual { latency_us } if module.responding => {
                let due = module
                    .bus_ready
                    .max(self.now_us)
                    .saturating_add(latency_us.max(1));
                module.bus_ready = due;
                Some(due)
            }
            _ => None,
        };
        self.requests.push_back(PuRequest {
            id,
            pc,
            slot,
            axis: axis.map(|a| a as u8 + 1),
            at_us: self.now_us,
            action: a.kind,
        });
        self.bindings.insert(pc, b);
        if let Some(due) = due {
            let _ = self.queue(due, Event::Reply(id, PuReply::Ack));
        }
    }
    fn release(&mut self, b: &Binding, pc: u16) {
        if let Some(module) = self.modules.get_mut(&b.slot) {
            if let Some(i) = b.axis {
                let a = &mut module.axes[i];
                if a.owner == Some(pc) {
                    a.owner = None;
                    a.run = None;
                    a.busy = false;
                    a.paused = false;
                    a.speed_hz = 0.;
                    a.outputs = [false; 2];
                }
            }
            if module.encoder.owner == Some(pc) {
                module.encoder.owner = None;
                module.encoder.next = None;
            }
        }
    }
    fn apply(&mut self, m: &mut Memory, pc: u16, b: &mut Binding) {
        let module = self.modules.get_mut(&b.slot).unwrap();
        let a = &b.action;
        let v = &b.values;
        match a.kind {
            PuKind::Configure => {
                let c = &mut module.axes[b.axis.unwrap()].config;
                c.mode = if (0..=3).contains(&v[2]) {
                    v[2] as u8
                } else {
                    1
                };
                c.start_hz = v[3].clamp(0, 10000) as u32;
                c.acceleration_ms = v[4].clamp(0, 10000) as u32;
                c.deceleration_ms = v[5].clamp(0, 10000) as u32;
                c.maximum_hz = v[6].clamp(100, 200000) as u32;
                c.z_count = v[7].clamp(-100, 100);
                c.offset = v[8].clamp(-10000, 10000);
                a.success(m);
            }
            PuKind::Input => {
                if !(0..=4).contains(&v[1]) {
                    a.error(m, 0x1400);
                    b.active = false;
                    return;
                }
                let i = &mut module.inputs[v[1] as usize];
                i.mode = if (0..=16).contains(&v[2]) {
                    v[2] as u8
                } else {
                    0
                };
                i.filter_ms = if (0..=25).contains(&v[3]) {
                    v[3] as u8
                } else {
                    10
                };
                i.pending = None;
                a.success(m);
            }
            PuKind::Limits => {
                let c = &mut module.axes[b.axis.unwrap()].config;
                c.negative_limit = v[2];
                c.positive_limit = v[3];
                a.success(m);
            }
            PuKind::Status => {
                let axis = &mut module.axes[b.axis.unwrap()];
                if v[2] != 0 {
                    axis.position = 0;
                }
                b.last_zero = v[2] != 0;
                a.success(m);
            }
            PuKind::Counter => {
                let e = &mut module.encoder;
                e.owner = Some(pc);
                e.mode = match v[1] {
                    1 | 2 | 4 | 5 | 6 | 7 => v[1] as u16,
                    _ => 4,
                };
                e.period_us = v[2].clamp(10, 1000) as u64 * 1000;
                e.next = Some(self.now_us.saturating_add(e.period_us));
                e.mpg = false;
                if v[3] != 0 {
                    e.count = 0;
                }
                e.previous = e.count;
                e.sample = 0;
                b.last_zero = v[3] != 0;
                a.success(m);
            }
            _ => {
                let axis = &mut module.axes[b.axis.unwrap()];
                let config = axis.config.clone();
                let mut immediate = false;
                let mut run = match a.kind {
                    PuKind::Pulse => {
                        let speed = v[3].clamp(-200000, 200000);
                        AxisRun::new(
                            RunMode::Train,
                            speed < 0,
                            if v[2] == 0 {
                                None
                            } else {
                                Some(u64::from(v[2].unsigned_abs()))
                            },
                            f64::from(speed.unsigned_abs().min(config.maximum_hz)),
                        )
                    }
                    PuKind::Relative | PuKind::Absolute => {
                        let delta = if a.kind == PuKind::Absolute {
                            i64::from(v[2]) - i64::from(axis.position)
                        } else {
                            i64::from(v[2])
                        };
                        let mut r = AxisRun::new(
                            RunMode::Ramp,
                            delta < 0,
                            Some(delta.unsigned_abs()),
                            f64::from(config.start_hz),
                        );
                        r.ramp(
                            &config,
                            f64::from(v[3].unsigned_abs().min(config.maximum_hz)),
                        );
                        r
                    }
                    PuKind::Jog => {
                        let speed = v[2].clamp(-200000, 200000);
                        let mut r = AxisRun::new(
                            RunMode::Jog,
                            speed < 0,
                            None,
                            f64::from(config.start_hz).min(f64::from(speed.unsigned_abs())),
                        );
                        r.ramp(
                            &config,
                            f64::from(speed.unsigned_abs().min(config.maximum_hz)),
                        );
                        r
                    }
                    PuKind::Home => {
                        let mode = v[2];
                        if mode == 0 || mode == 255 {
                            axis.position = if mode == 0 { 0 } else { v[3] };
                            immediate = true;
                            AxisRun::new(RunMode::Train, false, Some(0), 0.)
                        } else if (1..=8).contains(&mode) {
                            let speed = v[3]
                                .unsigned_abs()
                                .clamp(100, 200000)
                                .min(config.maximum_hz);
                            let jog = v[4].clamp(1, 10000) as u32;
                            let mut r = AxisRun::new(
                                RunMode::Home {
                                    mode: mode as u8,
                                    phase: HomePhase::Approach,
                                    leave_negative: mode % 2 == 1,
                                    jog,
                                    z_left: config.z_count.unsigned_abs(),
                                    dog: false,
                                },
                                v[3] < 0,
                                None,
                                f64::from(jog),
                            );
                            r.ramp(&config, f64::from(speed));
                            r
                        } else {
                            limitation(
                                m,
                                "DPUZRN mode0..8/255 dışında reserved; hareket başlatılmadı",
                            );
                            b.active = false;
                            return;
                        }
                    }
                    PuKind::Mpg => {
                        let rate = f64::from(f32::from_bits(v[5] as u32));
                        let e = &mut module.encoder;
                        e.owner = Some(pc);
                        e.mode = match v[2] {
                            1 | 2 | 4 | 5 | 6 | 7 => v[2] as u16,
                            _ => 4,
                        };
                        e.mpg = true;
                        e.count = 0;
                        e.previous = 0;
                        e.sample = 0;
                        e.period_us = 20000;
                        e.next = Some(self.now_us.saturating_add(20000));
                        let mut r = AxisRun::new(RunMode::Mpg, false, None, 0.);
                        r.mpg_rate = rate;
                        r
                    }
                    _ => unreachable!(),
                };
                axis.owner = Some(pc);
                axis.busy = !immediate;
                axis.error = 0;
                axis.position_overflow = false;
                axis.state_done = false;
                axis.state_negative = run.negative;
                axis.paused = run.speed() == 0. && !immediate;
                run.advance(axis, 0);
                axis.run = (!immediate).then_some(run);
                if immediate || axis.run.as_ref().is_some_and(|r| r.remaining == Some(0)) {
                    axis.busy = false;
                    axis.owner = None;
                    axis.run = None;
                    axis.state_done = true;
                    b.done = true;
                    a.success(m);
                }
            }
        }
        b.applied = true;
        if a.kind == PuKind::Home {
            let active_inputs: Vec<_> = module
                .inputs
                .iter()
                .enumerate()
                .filter_map(|(i, x)| {
                    ((5..=8).contains(&x.mode)
                        && usize::from((x.mode - 1) % 2) == b.axis.unwrap_or(2)
                        && if (x.mode - 1) % 4 >= 2 {
                            !x.level
                        } else {
                            x.level
                        })
                    .then_some((i, x.level))
                })
                .collect();
            for (i, level) in active_inputs {
                Self::input_signal(module, i, !level, level);
            }
        }
    }
    fn update(&mut self, m: &mut Memory, pc: u16, b: &mut Binding) {
        let Some(module) = self.modules.get_mut(&b.slot) else {
            return;
        };
        let action = &b.action;
        if action.kind == PuKind::Status {
            let zero = action.value(m, 2) != 0;
            if zero && !b.last_zero {
                module.axes[b.axis.unwrap()].position = 0;
            }
            b.last_zero = zero;
            return;
        }
        if action.kind == PuKind::Counter {
            let zero = action.value(m, 3) != 0;
            if zero && !b.last_zero {
                module.encoder.count = 0;
                module.encoder.previous = 0;
            }
            b.last_zero = zero;
            return;
        }
        if !action.kind.motion() || b.done {
            return;
        }
        let axis = &mut module.axes[b.axis.unwrap()];
        if axis.owner != Some(pc) {
            return;
        }
        let Some(mut run) = axis.run.take() else {
            return;
        };
        let speed_index = match action.kind {
            PuKind::Pulse | PuKind::Relative | PuKind::Absolute => Some(3),
            PuKind::Jog => Some(2),
            _ => None,
        };
        if let Some(i) = speed_index {
            let raw = action.value(m, i);
            if raw != b.values[i] {
                let max = axis.config.maximum_hz.min(200000);
                let target = raw.unsigned_abs().min(max);
                if matches!(run.mode, RunMode::Train) {
                    if !run.pending_speed.is_some_and(|(_, speed)| speed == 0) {
                        if run.speed() == 0. {
                            if raw != 0 && run.reverse_armed {
                                run.negative = raw < 0;
                                run.reverse_armed = false;
                            }
                            run.set_constant(f64::from(target));
                            run.reverse_armed = raw == 0;
                        } else {
                            run.pending_speed = Some((
                                run.next_pulse_us(self.now_us),
                                raw.clamp(-(max as i32), max as i32),
                            ));
                        }
                        b.values[i] = raw;
                    }
                } else {
                    if matches!(run.mode, RunMode::Jog) && raw != 0 && run.reverse_armed {
                        run.negative = raw < 0;
                        run.reverse_armed = false;
                    }
                    run.ramp(&axis.config, f64::from(target));
                    b.values[i] = raw;
                }
            }
        }
        axis.run = Some(run);
    }
    fn move_axes(&mut self, dt: u64) {
        for module in self.modules.values_mut() {
            for axis in &mut module.axes {
                if let Some(mut run) = axis.run.take() {
                    run.advance(axis, dt);
                    axis.run = Some(run);
                }
            }
        }
    }
    fn process(&mut self, m: &mut Memory) {
        let keys: Vec<_> = self
            .events
            .range(..=(self.now_us, u64::MAX))
            .map(|(k, _)| *k)
            .collect();
        for key in keys {
            let event = self.events.remove(&key).unwrap();
            match event {
                Event::Reply(id, reply) => {
                    if let Some(pc) = self
                        .bindings
                        .iter()
                        .find_map(|(&pc, b)| (b.id == id && b.active && !b.applied).then_some(pc))
                    {
                        let mut b = self.bindings.remove(&pc).unwrap();
                        match reply {
                            PuReply::Ack => self.apply(m, pc, &mut b),
                            PuReply::Fault(code) => {
                                if [0x1400, 0x1402, 0x1403, 0x1405, 0x1406, 0x1407].contains(&code)
                                {
                                    b.action.error(m, code);
                                    b.active = false;
                                } else {
                                    limitation(m, "PU dışcevapta belgelenmeyen hata kodu");
                                }
                            }
                        }
                        self.bindings.insert(pc, b);
                    }
                }
                Event::Input(slot, input, level) => {
                    if let Some(module) = self.modules.get_mut(&slot) {
                        let i = &mut module.inputs[input as usize];
                        i.raw = level;
                        i.pending = Some((
                            self.now_us.saturating_add(u64::from(i.filter_ms) * 1000),
                            level,
                        ));
                    }
                }
                Event::Encoder(slot, a, b) => {
                    if let Some(module) = self.modules.get_mut(&slot) {
                        let e = &mut module.encoder;
                        let old = (u8::from(e.a) << 1) | u8::from(e.b);
                        let new = (u8::from(a) << 1) | u8::from(b);
                        let quad = [0, -1, 1, 0, 1, 0, 0, -1, -1, 0, 0, 1, 0, 1, -1, 0]
                            [usize::from(old * 4 + new)];
                        let delta = match e.mode {
                            1 => {
                                if !e.a && a {
                                    quad
                                } else {
                                    0
                                }
                            }
                            2 => {
                                if e.a != a {
                                    quad
                                } else {
                                    0
                                }
                            }
                            5 => {
                                if !e.a && a {
                                    if b { -1 } else { 1 }
                                } else {
                                    0
                                }
                            }
                            6 => {
                                if !e.a && a {
                                    if b { 1 } else { -1 }
                                } else {
                                    0
                                }
                            }
                            7 => i32::from(!e.a && a),
                            _ => quad,
                        };
                        e.a = a;
                        e.b = b;
                        if e.owner.is_some() {
                            e.count = e.count.wrapping_add(delta);
                        }
                    }
                }
                Event::Delta(slot, delta) => {
                    if let Some(module) = self.modules.get_mut(&slot)
                        && module.encoder.owner.is_some()
                    {
                        module.encoder.count = module.encoder.count.wrapping_add(delta);
                    }
                }
            }
        }
        let timed_out: Vec<_> = self
            .bindings
            .iter()
            .filter_map(|(&pc, b)| {
                (b.active && !b.applied && b.timeout <= self.now_us).then_some(pc)
            })
            .collect();
        for pc in timed_out {
            if let Some(b) = self.bindings.get_mut(&pc) {
                b.action.error(m, 0x1402);
                b.active = false;
            }
        }
        let slots: Vec<_> = self.modules.keys().copied().collect();
        for slot in slots {
            let mut module = self.modules.remove(&slot).unwrap();
            for input in 0..5 {
                if let Some((at, level)) = module.inputs[input].pending
                    && at <= self.now_us
                {
                    let old = module.inputs[input].level;
                    module.inputs[input].level = level;
                    module.inputs[input].pending = None;
                    if old != level {
                        Self::input_signal(&mut module, input, old, level);
                    }
                }
            }
            if module.encoder.next.is_some_and(|at| at <= self.now_us) {
                let e = &mut module.encoder;
                e.sample = e.count.wrapping_sub(e.previous);
                e.previous = e.count;
                e.next = Some(self.now_us.saturating_add(e.period_us));
                if e.mpg {
                    let count = e.count;
                    let sample = e.sample;
                    for axis in &mut module.axes {
                        if let Some(r) = &mut axis.run
                            && matches!(r.mode, RunMode::Mpg)
                        {
                            let target = (f64::from(count) * r.mpg_rate).trunc();
                            if target.is_finite()
                                && target >= i64::MIN as f64
                                && target <= i64::MAX as f64
                            {
                                r.mpg_target = target as i64;
                                let pending = r.mpg_target.saturating_sub(r.mpg_emitted);
                                r.negative = pending < 0;
                                r.remaining = Some(pending.unsigned_abs());
                                let frequency = (f64::from(sample).abs() * 50. * r.mpg_rate.abs())
                                    .clamp(50., 100000.)
                                    .min(f64::from(axis.config.maximum_hz));
                                if pending != 0 {
                                    r.set_constant(if sample == 0 {
                                        r.speed().max(50.)
                                    } else {
                                        frequency
                                    });
                                } else {
                                    r.set_constant(0.);
                                }
                            }
                        }
                    }
                }
            }
            for axis in &mut module.axes {
                if let Some(mut run) = axis.run.take() {
                    if let Some((at, speed)) = run.pending_speed
                        && at <= self.now_us
                    {
                        run.pending_speed = None;
                        if speed == 0 {
                            run.set_constant(0.);
                            run.reverse_armed = true;
                        } else {
                            if run.reverse_armed {
                                run.negative = speed < 0;
                                run.reverse_armed = false;
                            }
                            run.set_constant(f64::from(speed.unsigned_abs()));
                        }
                    }
                    if run.speed() == 0. {
                        run.reverse_armed = true;
                    }
                    let stopped = run.speed() == 0.;
                    if let RunMode::Home {
                        phase,
                        leave_negative,
                        jog,
                        dog,
                        ..
                    } = &mut run.mode
                        && *phase == HomePhase::Brake
                        && stopped
                    {
                        run.negative = *leave_negative;
                        let speed = *jog;
                        *phase = if *dog {
                            HomePhase::Leaving
                        } else {
                            HomePhase::Reenter
                        };
                        run.set_constant(f64::from(speed));
                    }
                    if run.target_speed > 0.
                        && !matches!(run.mode, RunMode::Home { .. })
                        && (axis.config.negative_limit != 0 || axis.config.positive_limit != 0)
                    {
                        let code = if run.negative && axis.position <= axis.config.negative_limit {
                            Some((0x1407, axis.config.negative_limit))
                        } else if !run.negative && axis.position >= axis.config.positive_limit {
                            Some((0x1406, axis.config.positive_limit))
                        } else {
                            None
                        };
                        if let Some((code, bound)) = code {
                            axis.position = bound;
                            axis.error = code;
                            axis.busy = false;
                            axis.paused = false;
                            axis.speed_hz = 0.;
                            axis.outputs = [false; 2];
                            axis.owner = None;
                            continue;
                        }
                    }
                    let complete = run.completed() && !matches!(run.mode, RunMode::Mpg);
                    if complete {
                        axis.state_done = true;
                        let home = matches!(run.mode, RunMode::Home { .. });
                        axis.busy = false;
                        axis.paused = false;
                        axis.speed_hz = 0.;
                        axis.outputs = if axis.config.mode == 1 {
                            [false, run.negative]
                        } else {
                            [false; 2]
                        };
                        if home {
                            axis.position = 0;
                        }
                        if let Some(pc) = axis.owner.take()
                            && let Some(b) = self.bindings.get_mut(&pc)
                        {
                            b.done = true;
                            b.action.success(m);
                        }
                    } else {
                        axis.busy = true;
                        run.advance(axis, 0);
                        axis.run = Some(run);
                    }
                }
            }
            self.modules.insert(slot, module);
        }
    }
    fn publish(&mut self, m: &mut Memory) {
        // PDF 2-88: each physical right-hand slot occupies ten D words.
        // set_profile and CPU restart select the documented M1183 reset value.
        // An explicit M1183 controls publication without changing module state.
        if self.profile.model != PuCpuModel::Disabled && !m.system_bit(1183) {
            for (&slot, module) in &self.modules {
                if slot > 7 || module.kind != PuModuleKind::E2 {
                    continue;
                }
                let base = 9900 + slot * 10;
                if base + 1 > m.data_register_limit.unwrap_or(9999) {
                    continue;
                }
                let inputs = module
                    .inputs
                    .iter()
                    .enumerate()
                    .fold(0u16, |bits, (i, input)| {
                        bits | (u16::from(input.level) << i)
                    });
                let mut states = 0u16;
                for (i, axis) in module.axes.iter().enumerate() {
                    // Bit2's English wording is ambiguous: use output stopped (!busy or
                    // paused), independently of latched completion bit3. No native claim.
                    let flags = u16::from(axis.error != 0 || axis.position_overflow)
                        | (u16::from(axis.busy) << 1)
                        | (u16::from(!axis.busy || axis.paused) << 2)
                        | (u16::from(axis.state_done) << 3)
                        | (u16::from(axis.error == 0x1406) << 4)
                        | (u16::from(axis.error == 0x1407) << 5)
                        | (u16::from(axis.position_overflow) << 6)
                        | (u16::from(axis.state_negative) << 7);
                    states |= flags << (i * 8);
                }
                m.set_word(d(base), inputs as i16);
                m.set_word(d(base + 1), states as i16);
            }
        }

        for b in self
            .bindings
            .values()
            .filter(|b| (b.active || b.stopping) && b.applied)
        {
            let Some(module) = self.modules.get(&b.slot) else {
                continue;
            };
            let a = &b.action;
            match a.kind {
                PuKind::Status => {
                    let axis = &module.axes[b.axis.unwrap()];
                    m.set_double_word(a.device(3), axis.position);
                    m.set_internal(a.device(4), axis.busy);
                    m.set_internal(a.device(5), axis.paused);
                    m.set_internal(a.device(6), axis.error != 0);
                    m.set_word(a.device(7), axis.error as i16);
                }
                PuKind::Counter => {
                    m.set_double_word(a.device(4), module.encoder.count);
                    m.set_double_word(a.device(5), module.encoder.sample);
                }
                PuKind::Mpg => {
                    m.set_double_word(a.device(3), module.encoder.count);
                    m.set_double_word(a.device(4), module.encoder.sample.saturating_mul(50));
                    let axis = &module.axes[b.axis.unwrap()];
                    let emitted = axis.run.as_ref().map_or(0, |r| r.mpg_emitted as i32);
                    m.set_double_word(a.device(6), emitted);
                    m.set_double_word(a.device(7), axis.speed_hz as i32);
                    if axis.error != 0 {
                        a.error(m, axis.error);
                    }
                }
                PuKind::Jog => {
                    let axis = &module.axes[b.axis.unwrap()];
                    m.set_internal(a.device(3), axis.busy);
                    if axis.error != 0 {
                        a.error(m, axis.error);
                    }
                }
                _ => {
                    if let Some(i) = b.axis
                        && module.axes[i].error != 0
                        && a.kind.motion()
                    {
                        a.error(m, module.axes[i].error);
                    }
                }
            }
        }
    }
    fn input_signal(module: &mut VirtualPuModule, input: usize, _old: bool, new: bool) {
        let mode = module.inputs[input].mode;
        if mode == 0 {
            return;
        }
        let index = usize::from((mode - 1) % 2);
        let category = (mode - 1) / 4;
        let active = if (mode - 1) % 4 >= 2 { !new } else { new };
        let axis = &mut module.axes[index];
        let Some(mut run) = axis.run.take() else {
            return;
        };
        if category >= 2
            && active
            && ((category == 2 && run.negative) || (category == 3 && !run.negative))
        {
            axis.error = if category == 2 { 0x1407 } else { 0x1406 };
            if let RunMode::Home {
                phase, jog, dog, ..
            } = &mut run.mode
                && matches!(
                    phase,
                    HomePhase::Approach
                        | HomePhase::Reenter
                        | HomePhase::Brake
                        | HomePhase::Leaving
                )
            {
                let at_dog = *dog;
                let speed = *jog;
                run.negative = !run.negative;
                let peak = run.target_speed;
                run.set_constant(f64::from(speed));
                if !at_dog {
                    run.ramp(&axis.config, peak);
                }
                if let RunMode::Home { phase, .. } = &mut run.mode {
                    *phase = if at_dog {
                        HomePhase::Leaving
                    } else {
                        HomePhase::Approach
                    };
                }
                axis.run = Some(run);
                return;
            }
            axis.owner = None;
            axis.busy = false;
            axis.paused = false;
            axis.speed_hz = 0.;
            axis.outputs = [false; 2];
            return;
        }
        if let RunMode::Home {
            mode: home_mode,
            phase,
            leave_negative,
            jog,
            z_left,
            dog,
        } = &mut run.mode
        {
            match category {
                1 => {
                    *dog = active;
                    if active && matches!(phase, HomePhase::Approach | HomePhase::Reenter) {
                        let desired = *leave_negative;
                        let jog_speed = *jog;
                        if run.negative == desired {
                            *phase = HomePhase::Leaving;
                            run.ramp(&axis.config, f64::from(jog_speed));
                        } else {
                            *phase = HomePhase::Brake;
                            run.ramp(&axis.config, 0.);
                        }
                    } else if !active && *phase == HomePhase::Leaving {
                        Self::after_dog(axis, &mut run);
                    }
                }
                0 if active && *phase == HomePhase::SeekZ => {
                    *z_left = z_left.saturating_sub(1);
                    if *z_left == 0 {
                        let offset = matches!(*home_mode, 7 | 8);
                        Self::home_offset(axis, &mut run, offset);
                    }
                }
                _ => {}
            }
        }
        axis.run = Some(run);
    }
    fn after_dog(axis: &mut PuAxis, run: &mut AxisRun) {
        let (mode, jog) = match run.mode {
            RunMode::Home { mode, jog, .. } => (mode, jog),
            _ => return,
        };
        if matches!(mode, 3 | 4 | 7 | 8) && axis.config.z_count != 0 {
            run.negative = axis.config.z_count < 0;
            run.set_constant(f64::from(jog));
            run.remaining = None;
            if let RunMode::Home { phase, z_left, .. } = &mut run.mode {
                *phase = HomePhase::SeekZ;
                *z_left = axis.config.z_count.unsigned_abs();
            }
        } else {
            Self::home_offset(axis, run, matches!(mode, 5..=8));
        }
    }
    fn home_offset(axis: &mut PuAxis, run: &mut AxisRun, enabled: bool) {
        if enabled && axis.config.offset != 0 {
            run.negative = axis.config.offset < 0;
            run.remaining = Some(u64::from(axis.config.offset.unsigned_abs()));
            let jog = match run.mode {
                RunMode::Home { jog, .. } => jog,
                _ => 0,
            };
            run.set_constant(f64::from(jog));
            if let RunMode::Home { phase, .. } = &mut run.mode {
                *phase = HomePhase::Offset;
            }
        } else {
            run.remaining = Some(0);
            run.set_constant(0.);
        }
    }
}
#[cfg(test)]
#[path = "pu_module_tests.rs"]
mod tests;
