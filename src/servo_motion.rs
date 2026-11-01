//! Continuous speed, feedback-driven homing and explicit CAN emergency reception.
use super::*;
#[derive(Debug, Clone, PartialEq)]
pub enum ServoNextAction {
    Speed {
        node: i32,
        speed: CalendarValue,
        wide: bool,
    },
    Home {
        node: i32,
        first: CalendarValue,
        second: CalendarValue,
        wide: bool,
    },
    HomeMode {
        node: CalendarValue,
        mode: CalendarValue,
        ok: Device,
        error: Device,
    },
    Emergency {
        node: CalendarValue,
        destination: Device,
        length: Device,
        ok: Device,
        error: Device,
    },
}
impl ServoNextAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let wide = matches!(name, "DPLSVC" | "DZRNC");
        let n = match name {
            "PLSVC" | "DPLSVC" => 2,
            "ZRNC" | "DZRNC" => 3,
            "ZRNM" => 4,
            "EMER" => 5,
            _ => return Err("Servo sonraki komut biçimi geçersiz".into()),
        };
        if args.len() != n {
            return Err(format!("{name} {n} operand gerektirir"));
        }
        let scalar = |i: usize| -> Result<CalendarValue, String> {
            if !args[i].starts_with(['K', 'H', 'D']) {
                return Err("Servo operand K/H/D olmalı".into());
            }
            let v = CalendarValue::parse(&args[i], wide)?;
            if !matches!(
                &v,
                CalendarValue::Additional(AdditionalValue::Word(
                    Operand::Constant(_) | Operand::Register(Device { area: Area::D, .. })
                )) | CalendarValue::Additional(AdditionalValue::Double(
                    WideOperand::Constant(_) | WideOperand::Register(Device { area: Area::D, .. })
                ))
            ) {
                return Err("Servo skaler operand gerekli".into());
            }
            Ok(v)
        };
        let device = |i: usize, area: Area| -> Result<Device, String> {
            let v: Device = args[i].parse()?;
            if v.area != area || area == Area::M && !v.writable_bit() {
                return Err(format!("Servo operand{} çıplak{area:?} olmalı", i + 1));
            }
            Ok(v)
        };
        if matches!(name, "PLSVC" | "DPLSVC" | "ZRNC" | "DZRNC") {
            let node = match scalar(0)? {
                CalendarValue::Additional(AdditionalValue::Word(Operand::Constant(v))) => {
                    i32::from(v)
                }
                CalendarValue::Additional(AdditionalValue::Double(WideOperand::Constant(v))) => v,
                _ => return Err("PLSVC/ZRNC node yalnız K/H olmalı".into()),
            };
            if n == 2 {
                Ok(Self::Speed {
                    node,
                    speed: scalar(1)?,
                    wide,
                })
            } else {
                Ok(Self::Home {
                    node,
                    first: scalar(1)?,
                    second: scalar(2)?,
                    wide,
                })
            }
        } else if name == "ZRNM" {
            Ok(Self::HomeMode {
                node: scalar(0)?,
                mode: scalar(1)?,
                ok: device(2, Area::M)?,
                error: device(3, Area::M)?,
            })
        } else {
            Ok(Self::Emergency {
                node: scalar(0)?,
                destination: device(1, Area::D)?,
                length: device(2, Area::D)?,
                ok: device(3, Area::M)?,
                error: device(4, Area::M)?,
            })
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Speed { wide: false, .. } => "PLSVC",
            Self::Speed { .. } => "DPLSVC",
            Self::Home { wide: false, .. } => "ZRNC",
            Self::Home { .. } => "DZRNC",
            Self::HomeMode { .. } => "ZRNM",
            Self::Emergency { .. } => "EMER",
        }
    }
    pub fn header(&self) -> u16 {
        match self.name() {
            "PLSVC" => 0xb802,
            "DPLSVC" => 0xb902,
            "ZRNC" => 0xba02,
            "DZRNC" => 0xbb02,
            "EMER" => 0xc202,
            _ => 0xc402,
        }
    }
    pub fn words(&self) -> u16 {
        match self.name() {
            "PLSVC" => 5,
            "DPLSVC" => 9,
            "ZRNC" => 7,
            "DZRNC" => 13,
            "EMER" => 11,
            _ => 9,
        }
    }
    pub fn wide(&self) -> bool {
        matches!(
            self,
            Self::Speed { wide: true, .. } | Self::Home { wide: true, .. }
        )
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Speed { node, .. } | Self::Home { node, .. } => {
                relay(1623 + (*node).clamp(1, 8) as u16)
            }
            Self::HomeMode { ok, .. } | Self::Emergency { ok, .. } => *ok,
        }
    }
    pub fn operands(&self) -> Vec<CoordinatedOperand> {
        let v = |x: &CalendarValue| CoordinatedOperand::Value(x.clone());
        let dev = CoordinatedOperand::Device;
        let node = |n: i32| {
            CoordinatedOperand::Value(CalendarValue::parse(&format!("K{n}"), self.wide()).unwrap())
        };
        match self {
            Self::Speed { node: n, speed, .. } => vec![node(*n), v(speed)],
            Self::Home {
                node: n,
                first,
                second,
                ..
            } => vec![node(*n), v(first), v(second)],
            Self::HomeMode {
                node,
                mode,
                ok,
                error,
            } => vec![v(node), v(mode), dev(*ok), dev(*error)],
            Self::Emergency {
                node,
                destination,
                length,
                ok,
                error,
            } => vec![
                v(node),
                dev(*destination),
                dev(*length),
                dev(*ok),
                dev(*error),
            ],
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for x in self.operands() {
            match x {
                CoordinatedOperand::Value(v) => v.devices(out),
                CoordinatedOperand::Device(d) => {
                    out.insert(d);
                }
            }
        }
        out.extend([1614, 1615, 1616].map(relay));
        out.extend([6000, 6001, 6002].map(d));
        if let Self::Speed { node, .. } | Self::Home { node, .. } = self {
            let id = (*node).clamp(1, 8) as u16 - 1;
            out.extend([1624 + id, 1632 + id, 1640 + id].map(relay));
            out.extend([d(6032 + 2 * id), d(6033 + 2 * id)]);
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        match self {
            Self::Speed { node, .. } | Self::Home { node, .. } => format!(
                "PUU={}",
                m.double_word(d(6032 + 2 * ((*node).clamp(1, 8) as u16 - 1)))
            ),
            Self::Emergency { length, .. } => format!("EMCY {} mesaj", m.word(*length)),
            _ => format!("{}={}", self.dest(), m.bit(self.dest())),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeEvent {
    Origin(bool),
    PositiveLimit(bool),
    NegativeLimit(bool),
    ZPulse,
    Collision,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EmergencyMethod {
    #[default]
    Instruction,
    ExternalMethod,
}
#[derive(Debug, Clone, Copy)]
struct HomeConfig {
    mode: u16,
    setting: u32,
    origin: i32,
}
#[derive(Debug, Clone, Copy)]
struct Mode {
    reverse: bool,
    method: u8,
    z: u8,
    reverse_limit: bool,
    exact: bool,
}
impl Mode {
    fn decode(raw: u16) -> Result<Self, String> {
        let x = (raw & 15) as u8;
        let y = ((raw >> 4) & 15) as u8;
        let z = (raw >> 8) & 15;
        let w = raw >> 12;
        if x > 10 || y > if x >= 9 { 1 } else { 2 } || z > 1 || w > 1 {
            return Err("Homing WZYX mode peer tarafından kabul edildi ama yerel olay algoritması bu nibble'ı tanımlamıyor".into());
        }
        Ok(Self {
            reverse: matches!(x, 1 | 3 | 5 | 7 | 10),
            method: x,
            z: y,
            reverse_limit: z == 1,
            exact: w == 0,
        })
    }
}
#[derive(Debug, Clone, Copy)]
struct Velocity {
    position: f64,
    velocity: f64,
    target: f64,
    acceleration: f64,
    deceleration: f64,
    at: u64,
}
impl Velocity {
    fn advance(&mut self, at: u64) -> Result<(), String> {
        let mut dt = at.saturating_sub(self.at) as f64 / 1e6;
        for _ in 0..4 {
            if dt <= 0. {
                break;
            }
            if (self.velocity - self.target).abs() < 1e-12 {
                self.position += self.target * dt;
                break;
            }
            let interim = if self.velocity * self.target < 0. {
                0.
            } else {
                self.target
            };
            let a = if interim.abs() > self.velocity.abs() {
                self.acceleration
            } else {
                self.deceleration
            };
            let sign = (interim - self.velocity).signum();
            let needed = (interim - self.velocity).abs() / a;
            let part = dt.min(needed);
            self.position += self.velocity * part + 0.5 * sign * a * part * part;
            self.velocity += sign * a * part;
            dt -= part;
            if part >= needed {
                self.velocity = interim;
            }
        }
        self.at = at;
        if self.position.is_finite() && self.velocity.is_finite() {
            Ok(())
        } else {
            Err("Kalibre servo sürekli hareketi sonlu sayı sınırını aştı".into())
        }
    }
    fn stopped(self) -> bool {
        self.target == 0. && self.velocity.abs() < 1e-9
    }
    fn stop_deadline(self) -> Option<u64> {
        if self.target == 0. && self.velocity.abs() > 1e-9 {
            Some(
                self.at.saturating_add(
                    (self.velocity.abs() / self.deceleration * 1e6)
                        .ceil()
                        .max(1.) as u64,
                ),
            )
        } else {
            None
        }
    }
}
#[derive(Debug, Clone)]
struct SpeedRun {
    node: u8,
    path: Velocity,
    raw: i32,
    power: bool,
    flag_stop: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HomeStage {
    Seek,
    ForwardZ,
    FirstBrake,
    ReturnSearch,
    FinalBrake,
    ExactReturn,
    Stopped,
    Done,
}
#[derive(Debug, Clone)]
struct HomeRun {
    node: u8,
    path: Velocity,
    mode: Mode,
    config: HomeConfig,
    second: i32,
    stage: HomeStage,
    left: bool,
    reference: Option<f64>,
    return_path: Option<Trajectory>,
    restart_after_false: bool,
}
#[derive(Debug, Clone)]
struct ModeWait {
    ok: Device,
    error: Device,
    suppressed: bool,
}
#[derive(Debug, Clone)]
struct EmergencyReader {
    pc: u16,
    node: u8,
    destination: Device,
    length: Device,
    ok: Device,
    error: Device,
    count: usize,
}
#[derive(Debug, Clone)]
enum Event {
    Home { node: u8, event: HomeEvent },
    Emergency { node: u8, data: [u8; 8] },
}
#[derive(Debug, Clone, Default)]
pub(super) struct ServoMotionState {
    speeds: BTreeMap<u16, SpeedRun>,
    homes: BTreeMap<u16, HomeRun>,
    configs: BTreeMap<u8, HomeConfig>,
    mode_wait: BTreeMap<u16, ModeWait>,
    events: BTreeMap<(u64, u64), Event>,
    sequence: u64,
    origin_levels: BTreeMap<u8, bool>,
    limits: BTreeMap<(u8, bool), bool>,
    mailboxes: BTreeMap<u8, VecDeque<[u8; 8]>>,
    reader: Option<EmergencyReader>,
    emergency_method: EmergencyMethod,
    cancelled: BTreeSet<u16>,
}
impl ServoMotionState {
    pub fn node_busy(&self, node: u8) -> bool {
        self.speeds
            .values()
            .any(|r| r.node == node && (!r.path.stopped() || r.power))
            || self
                .homes
                .values()
                .any(|r| r.node == node && !matches!(r.stage, HomeStage::Done | HomeStage::Stopped))
    }
    pub fn configure_homing(
        &mut self,
        node: u8,
        mode: u16,
        setting: u32,
        origin: i32,
    ) -> Result<(), String> {
        if !(1..=8).contains(&node) || self.node_busy(node) {
            return Err("Homing parametresi boş1..8node gerektirir".into());
        }
        self.configs.insert(
            node,
            HomeConfig {
                mode,
                setting,
                origin,
            },
        );
        Ok(())
    }
    pub fn set_emergency_method(&mut self, method: EmergencyMethod) -> Result<(), String> {
        if self.reader.is_some() {
            return Err("Etkin EMER varken ikinci okuma yöntemi seçilemez".into());
        }
        self.emergency_method = method;
        Ok(())
    }
    pub fn queue_home_event(
        &mut self,
        now: u64,
        at: u64,
        node: u8,
        event: HomeEvent,
    ) -> Result<(), String> {
        if !(1..=8).contains(&node) {
            return Err("Homing feedback node1..8 gerekli".into());
        }
        self.enqueue(now, at, Event::Home { node, event })
    }
    pub fn queue_emergency(
        &mut self,
        now: u64,
        at: u64,
        node: u8,
        data: [u8; 8],
    ) -> Result<(), String> {
        if !(1..=127).contains(&node) {
            return Err("EMCY mevcut1..127node gerekli".into());
        }
        self.enqueue(now, at, Event::Emergency { node, data })
    }
    fn enqueue(&mut self, now: u64, at: u64, event: Event) -> Result<(), String> {
        if at < now || self.events.len() >= 4096 {
            return Err("Servo feedback gelecek zaman ve4096olay sınırı gerektirir".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Servo feedback sıra taşması")?;
        self.events.insert((at, self.sequence), event);
        Ok(())
    }
    pub fn reset_instruction_state(&mut self) {
        self.speeds.clear();
        self.homes.clear();
        self.mode_wait.clear();
        self.cancelled.clear();
        self.reader = None;
        self.events.clear();
        self.mailboxes.clear();
    }
    pub fn abort(&mut self) {
        self.speeds.clear();
        self.homes.clear();
        self.mode_wait.clear();
        self.cancelled.clear();
        self.reader = None;
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.events
            .first_key_value()
            .map(|(k, _)| k.0)
            .into_iter()
            .chain(self.speeds.values().filter_map(|r| r.path.stop_deadline()))
            .chain(self.homes.values().filter_map(|r| match r.stage {
                HomeStage::FirstBrake | HomeStage::FinalBrake => r.path.stop_deadline(),
                HomeStage::ExactReturn => r.return_path.map(Trajectory::deadline),
                _ => None,
            }))
            .min()
    }
    fn log(m: &mut Memory, node: u8, pc: u16, code: Option<u16>) {
        m.set_system(1616, true);
        m.set_word(d(6000), i16::from(node));
        m.set_word(d(6002), pc as i16);
        if let Some(code) = code {
            m.set_word(d(6001), code as i16);
        }
    }
    fn mode_profile(m: &Memory, modern: bool) -> Result<(), String> {
        let p = m.canopen.profile().ok_or("CAN CPU profili yok")?;
        if modern && p.cpu == CanCpu::Es2C && (p.firmware_major, p.firmware_minor) < (3, 60) {
            return Err("Bu komut ES2-C>=3.60 veya SE2 gerektirir".into());
        }
        Ok(())
    }
    fn flags(m: &Memory, ok: Device, error: Device) -> Result<(), String> {
        for dev in [ok, error] {
            if dev.area != Area::M
                || !dev.writable_bit()
                || dev.index > m.relay_limit.unwrap_or(4095)
                || dev.to_string().parse::<Device>().is_err()
            {
                return Err("Servo Ok/Err yazılabilir model M adresi olmalı".into());
            }
        }
        Ok(())
    }
    fn ready(parent: &CoordinatedState, m: &Memory, node: u8, enable: bool) -> Result<(), String> {
        let n = m
            .canopen
            .node(node)
            .ok_or("Servo node açıkça yapılandırılmalı")?;
        if !m.system_bit(1614)
            || !m.system_bit(1615)
            || !n.online
            || !n.initialized
            || enable && !n.servo_enabled
        {
            return Err("Gerçek INITC/ASDON ve çevrimiçi node gerekli".into());
        }
        span(m, d(6000), 3)?;
        if enable {
            let p = parent
                .plants
                .get(&node)
                .ok_or("Servo PUU kalibrasyonu yok")?;
            if p.calibration.is_none() || p.acceleration_ms.is_none() || p.deceleration_ms.is_none()
            {
                return Err("Servo kalibrasyonu ve gerçek CASD ACK gerekli".into());
            }
            span(m, d(6032 + 2 * u16::from(node - 1)), 2)?;
        }
        Ok(())
    }
    fn velocity(
        parent: &CoordinatedState,
        node: u8,
        speed: i32,
        at: u64,
    ) -> Result<Velocity, String> {
        let p = parent.plants.get(&node).ok_or("Servo kalibrasyonu yok")?;
        let c = p.calibration.ok_or("Servo kalibrasyonu yok")?;
        let a = c.reference() * 1000. / f64::from(p.acceleration_ms.ok_or("CASD yok")?);
        let b = c.reference() * 1000. / f64::from(p.deceleration_ms.ok_or("CASD yok")?);
        let target = c.rate(speed);
        if ![a, b, target].iter().all(|v| v.is_finite()) || a <= 0. || b <= 0. {
            return Err("Servo ivme/hız kalibrasyonu sonlu ve pozitif olmalı".into());
        }
        Ok(Velocity {
            position: f64::from(p.position),
            velocity: 0.,
            target,
            acceleration: a,
            deceleration: b,
            at,
        })
    }
    fn send(
        parent: &mut CoordinatedState,
        m: &mut Memory,
        node: u8,
        pc: u16,
        intent: ServoIntent,
    ) -> Result<bool, String> {
        if parent.pending.is_some() {
            return Ok(false);
        }
        if !m.canopen.reserve_exchange(0, pc) {
            return Ok(false);
        }
        parent.sequence = parent
            .sequence
            .checked_add(1)
            .ok_or("Servo request sayacı taştı")?;
        let request = ServoRequest {
            id: parent.sequence,
            epoch: parent.epoch,
            pc,
            node,
            at_us: parent.now_us,
            intent,
        };
        if parent.outgoing.len() >= 4096 {
            parent.outgoing.pop_front();
        }
        parent.outgoing.push_back(request.clone());
        parent.pending = Some(Pending { request });
        Ok(true)
    }
    pub fn visit(
        &mut self,
        parent: &mut CoordinatedState,
        m: &mut Memory,
        action: &ServoNextAction,
        power: bool,
        pc: u16,
    ) {
        if !power {
            parent.completed.remove(&pc);
            if parent.pending.as_ref().is_some_and(|p| {
                p.request.pc == pc
                    && matches!(
                        p.request.intent,
                        ServoIntent::Speed { .. } | ServoIntent::Homing { .. }
                    )
            }) {
                self.cancelled.insert(pc);
            }
            if let Some(run) = self.speeds.get_mut(&pc) {
                run.power = false;
                run.path.target = 0.;
            }
            if let Some(run) = self.homes.get_mut(&pc) {
                run.path.target = 0.;
                run.stage = HomeStage::Stopped;
                run.restart_after_false = true;
            }
            if let Some(wait) = self.mode_wait.get_mut(&pc) {
                wait.suppressed = true;
            }
            if self.reader.as_ref().is_some_and(|r| r.pc == pc) {
                self.reader = None;
            }
            return;
        }
        if parent.completed.contains(&pc) {
            return;
        }
        let result = (|| -> Result<(), String> {
            match action {
                ServoNextAction::Speed { node, speed, .. } => {
                    if !(1..=8).contains(node) {
                        return Ok(());
                    }
                    let node = *node as u8;
                    Self::ready(parent, m, node, true)?;
                    let raw = value(m, speed)?;
                    if !(-60000..=60000).contains(&raw) {
                        return Err("PLSVC hız aralığı-60000..60000".into());
                    }
                    if !matches!(
                        parent.plants[&node].calibration,
                        Some(ServoCalibration::Rotary { .. })
                    ) {
                        return Err("PLSVC linear motor hız birimi bu detayda tanımlanmıyor".into());
                    }
                    if let Some(r) = self.speeds.get_mut(&pc) {
                        let flag = m.system_bit(1631 + u16::from(node));
                        let resume = r.flag_stop && !flag;
                        let changed = r.raw != raw || !r.power || resume;
                        // Keep rearm pending until its own speed ACK. Another
                        // CAN owner can prevent send() from queuing this visit.
                        if flag {
                            return Ok(());
                        }
                        if changed {
                            Self::send(parent, m, node, pc, ServoIntent::Speed { speed: raw })?;
                        }
                        return Ok(());
                    }
                    if self.node_busy(node)
                        || parent
                            .servo_runs
                            .values()
                            .any(|r| r.node == node && !r.done)
                    {
                        return Ok(());
                    }
                    Self::velocity(parent, node, raw, parent.now_us)?;
                    Self::send(parent, m, node, pc, ServoIntent::Speed { speed: raw })?;
                }
                ServoNextAction::Home {
                    node,
                    first,
                    second,
                    wide,
                } => {
                    if !(1..=8).contains(node) {
                        return Ok(());
                    }
                    let node = *node as u8;
                    if self.homes.get(&pc).is_some_and(|r| {
                        r.restart_after_false && r.stage == HomeStage::Stopped && r.path.stopped()
                    }) {
                        self.homes.remove(&pc);
                    } else if self.homes.contains_key(&pc) {
                        return Ok(());
                    }
                    Self::ready(parent, m, node, true)?;
                    Self::mode_profile(m, *wide)?;
                    let profile = m.canopen.profile().unwrap();
                    let modern = profile.cpu == CanCpu::Se2
                        || (profile.firmware_major, profile.firmware_minor) >= (3, 60);
                    if modern && profile.servo != CanServoModel::A3M {
                        return Err("Yeni ZRNC detayında yalnız ASDA-A3 belirtiliyor".into());
                    }
                    let config=*self.configs.get(&node).ok_or("Homing P5-04/P6-00/P6-01 explicitservo parametreleri veya ZRNM ACK gerekli")?;
                    Mode::decode(config.mode)?;
                    if config.setting != 0 {
                        return Err(
                            "P6-00 nonzero homing setting bitleri seçili PDF'de açıklanmıyor"
                                .into(),
                        );
                    }
                    let fast = value(m, first)?;
                    let slow = value(m, second)?;
                    let linear = matches!(
                        parent.plants[&node].calibration,
                        Some(ServoCalibration::Linear { .. })
                    );
                    let max = if modern {
                        if linear { 15999999 } else { 60000 }
                    } else {
                        20000
                    };
                    let slowmax = if modern { max } else { 5000 };
                    if !(1..=max).contains(&fast) || !(1..=slowmax).contains(&slow) {
                        return Err("ZRNC birinci/ikinci hız motor/firmware aralığı dışında".into());
                    }
                    if self.node_busy(node)
                        || parent
                            .servo_runs
                            .values()
                            .any(|r| r.node == node && !r.done)
                    {
                        return Ok(());
                    }
                    Self::velocity(parent, node, fast, parent.now_us)?;
                    Self::send(
                        parent,
                        m,
                        node,
                        pc,
                        ServoIntent::Homing {
                            first: fast,
                            second: slow,
                            wide: *wide,
                        },
                    )?;
                }
                ServoNextAction::HomeMode {
                    node,
                    mode,
                    ok,
                    error,
                } => {
                    Self::mode_profile(m, true)?;
                    Self::flags(m, *ok, *error)?;
                    let n = value(m, node)?;
                    if !(1..=8).contains(&n) {
                        m.set_internal(*error, true);
                        Self::log(m, n as u8, pc, Some(5));
                        return Ok(());
                    }
                    Self::ready(parent, m, n as u8, false)?;
                    let mode = value(m, mode)? as u16;
                    if Self::send(parent, m, n as u8, pc, ServoIntent::HomingMode { mode })? {
                        self.mode_wait.insert(
                            pc,
                            ModeWait {
                                ok: *ok,
                                error: *error,
                                suppressed: false,
                            },
                        );
                        m.set_internal(*ok, false);
                    }
                }
                ServoNextAction::Emergency {
                    node,
                    destination,
                    length,
                    ok,
                    error,
                } => {
                    Self::mode_profile(m, true)?;
                    Self::flags(m, *ok, *error)?;
                    span(m, *length, 1)?;
                    span(m, *destination, 20)?;
                    span(m, d(6000), 3)?;
                    if self.emergency_method != EmergencyMethod::Instruction {
                        return Err(
                            "EMER ile ikinci Emergency okuma yöntemi aynı anda kullanılamaz".into(),
                        );
                    }
                    let n = value(m, node)?;
                    if !(1..=127).contains(&n)
                        || !m.canopen.node(n as u8).is_some_and(|peer| peer.online)
                    {
                        m.set_internal(*error, true);
                        Self::log(m, n as u8, pc, None);
                        parent.notices.insert("EMER yok/çevrimdışı node timeout; sayısal errorcode eşlemesi uydurulmadı".into());
                        return Ok(());
                    }
                    if let Some(r) = &self.reader {
                        if r.pc != pc {
                            return Ok(());
                        }
                    } else {
                        self.reader = Some(EmergencyReader {
                            pc,
                            node: n as u8,
                            destination: *destination,
                            length: *length,
                            ok: *ok,
                            error: *error,
                            count: 0,
                        });
                        m.set_internal(*ok, false);
                        m.set_word(*length, 0);
                    }
                    self.drain_emergency(parent, m)?;
                }
            }
            Ok(())
        })();
        if let Err(reason) = result {
            m.execution_limitations
                .insert(format!("{} STEP{pc}: {reason}", action.name()));
        }
    }
    pub fn reply(
        &mut self,
        parent: &mut CoordinatedState,
        m: &mut Memory,
        request: ServoRequest,
        reply: ServoReply,
    ) {
        if let ServoReply::Fault { code } = reply {
            Self::log(m, request.node, request.pc, Some(code));
            if let Some(wait) = self.mode_wait.remove(&request.pc) {
                m.set_internal(wait.error, true);
            }
            parent.completed.insert(request.pc);
            return;
        }
        if self.cancelled.remove(&request.pc) {
            parent.notices.insert("Kaynak OFF iken gelen servo ACK yerel hareket başlatmadı; uzak servo iptal/stop protokolü ayrıca doğrulanmalı".into());
            return;
        }
        match request.intent {
            ServoIntent::Speed { speed } => {
                if let Some(run) = self.speeds.get_mut(&request.pc) {
                    run.path.target = parent.plants[&request.node]
                        .calibration
                        .unwrap()
                        .rate(speed);
                    run.raw = speed;
                    run.power = true;
                    run.flag_stop = false;
                } else {
                    match Self::velocity(parent, request.node, speed, parent.now_us) {
                        Ok(path) => {
                            self.speeds.insert(
                                request.pc,
                                SpeedRun {
                                    node: request.node,
                                    path,
                                    raw: speed,
                                    power: true,
                                    flag_stop: false,
                                },
                            );
                        }
                        Err(e) => {
                            m.execution_limitations.insert(e);
                        }
                    }
                }
            }
            ServoIntent::HomingMode { mode } => {
                let previous = self
                    .configs
                    .get(&request.node)
                    .copied()
                    .unwrap_or(HomeConfig {
                        mode,
                        setting: 0,
                        origin: 0,
                    });
                self.configs
                    .insert(request.node, HomeConfig { mode, ..previous });
                if let Some(wait) = self.mode_wait.remove(&request.pc)
                    && !wait.suppressed
                {
                    m.set_internal(wait.ok, true);
                }
                parent.completed.insert(request.pc);
            }
            ServoIntent::Homing { first, second, .. } => {
                let Some(config) = self.configs.get(&request.node).copied() else {
                    return;
                };
                let Ok(mut mode) = Mode::decode(config.mode) else {
                    return;
                };
                let profile = m.canopen.profile().unwrap();
                if profile.cpu == CanCpu::Es2C
                    && (profile.firmware_major, profile.firmware_minor) < (3, 60)
                {
                    mode.exact = false;
                }
                let sign = if mode.reverse { -1 } else { 1 };
                match Self::velocity(parent, request.node, first * sign, parent.now_us) {
                    Ok(path) => {
                        m.set_system(1623 + u16::from(request.node), false);
                        if mode.method == 8 {
                            if let Some(p) = parent.plants.get_mut(&request.node) {
                                p.position = config.origin;
                                m.set_double_word_raw(
                                    d(6032 + 2 * u16::from(request.node - 1)),
                                    config.origin,
                                );
                            }
                            m.set_system(1623 + u16::from(request.node), true);
                            parent.completed.insert(request.pc);
                        } else {
                            self.homes.insert(
                                request.pc,
                                HomeRun {
                                    node: request.node,
                                    path,
                                    mode,
                                    config,
                                    second,
                                    stage: HomeStage::Seek,
                                    left: false,
                                    reference: None,
                                    return_path: None,
                                    restart_after_false: false,
                                },
                            );
                        }
                    }
                    Err(e) => {
                        m.execution_limitations.insert(e);
                    }
                }
            }
            _ => unreachable!(),
        }
        parent.notices.insert("Servo speed/homing calibrated local profile: explicit peer ACK and timestamped physical-node sensor events; no elapsed-only homing success".into());
    }
    pub fn service_controls(&mut self, parent: &mut CoordinatedState, m: &mut Memory) {
        for run in self.speeds.values_mut() {
            if !m
                .canopen
                .node(run.node)
                .is_some_and(|n| n.online && n.servo_enabled)
            {
                run.path.target = 0.;
                run.power = false;
            }
            if m.system_bit(1631 + u16::from(run.node)) {
                run.path.target = 0.;
                run.flag_stop = true;
            }
        }
        for run in self.homes.values_mut() {
            if m.system_bit(1631 + u16::from(run.node))
                || !m
                    .canopen
                    .node(run.node)
                    .is_some_and(|n| n.online && n.servo_enabled)
            {
                run.path.target = 0.;
                run.stage = HomeStage::Stopped;
            }
        }
        let _ = parent;
    }
    fn publish(parent: &mut CoordinatedState, m: &mut Memory, node: u8, position: f64) {
        if let Some(p) = parent.plants.get_mut(&node) {
            p.position = position.round() as i64 as i32;
            m.set_double_word_raw(d(6032 + 2 * u16::from(node - 1)), p.position);
        }
    }
    pub fn advance(&mut self, parent: &mut CoordinatedState, m: &mut Memory, at: u64) {
        for run in self.speeds.values_mut() {
            if let Err(e) = run.path.advance(at) {
                m.execution_limitations.insert(e);
                run.path.target = 0.;
                run.path.velocity = 0.;
            }
            Self::publish(parent, m, run.node, run.path.position);
        }
        for run in self.homes.values_mut() {
            if matches!(run.stage, HomeStage::Done) {
                continue;
            }
            if run.stage == HomeStage::ExactReturn {
                let (position, _, done) = run.return_path.unwrap().sample(at);
                run.path.position = position;
                run.path.at = at;
                if done {
                    run.path.position = f64::from(run.config.origin);
                    run.stage = HomeStage::Done;
                    m.set_system(1623 + u16::from(run.node), true);
                }
            } else {
                if let Err(e) = run.path.advance(at) {
                    m.execution_limitations.insert(e);
                    run.stage = HomeStage::Stopped;
                    run.path.target = 0.;
                    run.path.velocity = 0.;
                }
                if run.path.stopped() {
                    match run.stage {
                        HomeStage::FirstBrake => {
                            let c = parent.plants[&run.node].calibration.unwrap();
                            run.path.target = c.rate(if run.mode.reverse {
                                run.second
                            } else {
                                -run.second
                            });
                            run.stage = HomeStage::ReturnSearch;
                            run.left = matches!(run.mode.method, 4 | 5 | 9 | 10);
                        }
                        HomeStage::FinalBrake => {
                            if run.mode.exact {
                                if let Some(reference) = run.reference {
                                    match Trajectory::new(
                                        run.path.position,
                                        reference,
                                        parent.plants[&run.node]
                                            .calibration
                                            .unwrap()
                                            .rate(run.second),
                                        run.path.acceleration,
                                        run.path.deceleration,
                                        at,
                                    ) {
                                        Ok(path) => {
                                            run.return_path = Some(path);
                                            run.stage = HomeStage::ExactReturn;
                                        }
                                        Err(e) => {
                                            m.execution_limitations.insert(e);
                                            run.stage = HomeStage::Stopped;
                                        }
                                    }
                                }
                            } else {
                                run.path.position = f64::from(run.config.origin)
                                    + run.path.position
                                    - run.reference.unwrap_or(run.path.position);
                                run.stage = HomeStage::Done;
                                m.set_system(1623 + u16::from(run.node), true);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Self::publish(parent, m, run.node, run.path.position);
        }
        let keys = self
            .events
            .range(..=(at, u64::MAX))
            .map(|(k, _)| *k)
            .collect::<Vec<_>>();
        for key in keys {
            match self.events.remove(&key).unwrap() {
                Event::Emergency { node, data } => {
                    let box_ = self.mailboxes.entry(node).or_default();
                    if box_.len() < 32 {
                        box_.push_back(data);
                    } else {
                        parent
                            .notices
                            .insert("EMCY node mailbox32sınırı; yeni paket saklanmadı".into());
                    }
                }
                Event::Home { node, event } => self.home_event(parent, node, event),
            }
        }
        if let Err(e) = self.drain_emergency(parent, m) {
            m.execution_limitations.insert(e);
        }
    }
    fn home_event(&mut self, parent: &mut CoordinatedState, node: u8, event: HomeEvent) {
        let old_origin = *self.origin_levels.get(&node).unwrap_or(&false);
        match event {
            HomeEvent::Origin(level) => {
                self.origin_levels.insert(node, level);
            }
            HomeEvent::PositiveLimit(level) => {
                self.limits.insert((node, true), level);
            }
            HomeEvent::NegativeLimit(level) => {
                self.limits.insert((node, false), level);
            }
            _ => {}
        }
        for run in self.homes.values_mut().filter(|r| r.node == node) {
            if matches!(
                run.stage,
                HomeStage::Done | HomeStage::Stopped | HomeStage::ExactReturn
            ) {
                continue;
            }
            let sensor = match run.mode.method {
                0 => matches!(event, HomeEvent::PositiveLimit(true)),
                1 => matches!(event, HomeEvent::NegativeLimit(true)),
                2 | 3 => matches!(event, HomeEvent::Origin(true)) && !old_origin,
                4 | 5 => event == HomeEvent::ZPulse,
                6 | 7 => matches!(event, HomeEvent::Origin(false)) && old_origin,
                9 | 10 => event == HomeEvent::Collision,
                _ => false,
            };
            match run.stage {
                HomeStage::Seek if sensor => {
                    run.reference = Some(run.path.position);
                    if run.mode.z == 1 {
                        if matches!(run.mode.method, 4 | 5 | 9 | 10) {
                            run.path.target = 0.;
                            run.stage = HomeStage::FinalBrake;
                        } else {
                            run.stage = HomeStage::ForwardZ;
                        }
                    } else if run.mode.z == 2 && matches!(run.mode.method, 4 | 5) {
                        run.path.target = 0.;
                        run.stage = HomeStage::FinalBrake;
                    } else {
                        run.path.target = 0.;
                        run.stage = HomeStage::FirstBrake;
                    }
                }
                HomeStage::ForwardZ if event == HomeEvent::ZPulse => {
                    run.reference = Some(run.path.position);
                    run.path.target = 0.;
                    run.stage = HomeStage::FinalBrake;
                }
                HomeStage::ReturnSearch => {
                    let leaving = match run.mode.method {
                        0 => event == HomeEvent::PositiveLimit(false),
                        1 => event == HomeEvent::NegativeLimit(false),
                        2 | 3 => event == HomeEvent::Origin(false),
                        6 | 7 => event == HomeEvent::Origin(true),
                        _ => false,
                    };
                    if leaving {
                        run.left = true;
                        if run.mode.z == 2 {
                            run.reference = Some(run.path.position);
                            run.path.target = 0.;
                            run.stage = HomeStage::FinalBrake;
                        }
                    }
                    if run.left && run.mode.z == 0 && event == HomeEvent::ZPulse {
                        run.reference = Some(run.path.position);
                        run.path.target = 0.;
                        run.stage = HomeStage::FinalBrake;
                    }
                }
                _ => {}
            }
            let adverse = matches!(event, HomeEvent::PositiveLimit(true)) && run.path.target > 0.
                || matches!(event, HomeEvent::NegativeLimit(true)) && run.path.target < 0.;
            if adverse && !sensor {
                if run.mode.reverse_limit {
                    run.path.target = -run.path.target;
                } else {
                    run.path.target = 0.;
                    run.stage = HomeStage::Stopped;
                    parent.notices.insert(
                        "Homing limit-stop; origin bulunmadığından completion üretilmedi".into(),
                    );
                }
            }
        }
    }
    fn drain_emergency(
        &mut self,
        parent: &mut CoordinatedState,
        m: &mut Memory,
    ) -> Result<(), String> {
        let Some(reader) = &mut self.reader else {
            return Ok(());
        };
        if !m.canopen.node(reader.node).is_some_and(|n| n.online) {
            m.set_internal(reader.error, true);
            return Ok(());
        }
        let Some(box_) = self.mailboxes.get_mut(&reader.node) else {
            return Ok(());
        };
        let n = box_.len().min(5 - reader.count);
        if n == 0 {
            return Ok(());
        }
        span(m, reader.destination, ((reader.count + n) * 4) as u16)?;
        span(m, reader.length, 1)?;
        let mut packets = Vec::new();
        for _ in 0..n {
            packets.push(box_.pop_front().unwrap());
        }
        for (packet, bytes) in packets.iter().enumerate() {
            for (i, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
                m.set_word(
                    d(reader.destination.index + ((reader.count + packet) * 4 + i) as u16),
                    u16::from_le_bytes([pair[0], pair[1]]) as i16,
                );
            }
        }
        reader.count += n;
        m.set_word(reader.length, reader.count as i16);
        m.set_internal(reader.ok, true);
        parent.notices.insert("EMER FIFO5messagebatch, data-then-Len aliasorder ve completionhold yerelprofil; diğerEmergency yöntemiexclusive".into());
        Ok(())
    }
}
