//! Explicit virtual serial lifecycle. No OS port or default/echo peer exists.
use crate::calendar_operand::CalendarValue;
use crate::interrupt::InterruptId;
use crate::plc::{Area, Device, Memory};
use crate::serial_protocol::{self, Frame, Reply, WireMode};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn limit(m: &mut Memory, text: impl Into<String>) {
    m.execution_limitations.insert(text.into());
}
#[derive(Debug, Clone, PartialEq)]
pub enum SerialOperand {
    Value(CalendarValue),
    Data(Device),
}
#[derive(Debug, Clone, PartialEq)]
pub enum SerialIoAction {
    Raw {
        source: Device,
        send: CalendarValue,
        dest: Device,
        receive: CalendarValue,
    },
    Read {
        station: CalendarValue,
        address: CalendarValue,
        count: CalendarValue,
    },
    Write {
        station: CalendarValue,
        address: CalendarValue,
        value: CalendarValue,
    },
    Exchange {
        station: CalendarValue,
        function: CalendarValue,
        address: CalendarValue,
        buffer: Device,
        count: CalendarValue,
    },
}
impl SerialIoAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let expected = match name {
            "RS" => 4,
            "MODRD" | "MODWR" => 3,
            "MODRW" => 5,
            _ => return Err("RS/MODRD/MODWR/MODRW gerekli; P/D biçimi belgelenmedi".into()),
        };
        if args.len() != expected {
            return Err(format!("{name} {expected} operand gerektirir"));
        }
        let value = |i: usize| -> Result<CalendarValue, String> {
            if !args[i].starts_with(['K', 'H', 'D']) {
                return Err("Seri komut sayısal operandı yalnız K/H/D olmalı".into());
            }
            CalendarValue::parse(&args[i], false)
        };
        let data = |i: usize| -> Result<Device, String> {
            let v: Device = args[i].parse()?;
            if v.area != Area::D {
                return Err("Seri tampon çıplak D register olmalı".into());
            }
            Ok(v)
        };
        Ok(match name {
            "RS" => Self::Raw {
                source: data(0)?,
                send: value(1)?,
                dest: data(2)?,
                receive: value(3)?,
            },
            "MODRD" => Self::Read {
                station: value(0)?,
                address: value(1)?,
                count: value(2)?,
            },
            "MODWR" => Self::Write {
                station: value(0)?,
                address: value(1)?,
                value: value(2)?,
            },
            _ => Self::Exchange {
                station: value(0)?,
                function: value(1)?,
                address: value(2)?,
                buffer: data(3)?,
                count: value(4)?,
            },
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Raw { .. } => "RS",
            Self::Read { .. } => "MODRD",
            Self::Write { .. } => "MODWR",
            Self::Exchange { .. } => "MODRW",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Raw { .. } => 0xb000,
            Self::Read { .. } => 0xc200,
            Self::Write { .. } => 0xc400,
            Self::Exchange { .. } => 0x4c01,
        }
    }
    pub fn words(&self) -> u16 {
        match self {
            Self::Raw { .. } => 9,
            Self::Read { .. } | Self::Write { .. } => 7,
            Self::Exchange { .. } => 11,
        }
    }
    pub fn wide(&self) -> bool {
        false
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Raw { dest, .. } => *dest,
            Self::Exchange { buffer, .. } => *buffer,
            _ => d(1070),
        }
    }
    pub fn operands(&self) -> Vec<SerialOperand> {
        use SerialOperand::{Data as D, Value as V};
        match self {
            Self::Raw {
                source,
                send,
                dest,
                receive,
            } => vec![D(*source), V(send.clone()), D(*dest), V(receive.clone())],
            Self::Read {
                station,
                address,
                count,
            } => vec![V(station.clone()), V(address.clone()), V(count.clone())],
            Self::Write {
                station,
                address,
                value,
            } => vec![V(station.clone()), V(address.clone()), V(value.clone())],
            Self::Exchange {
                station,
                function,
                address,
                buffer,
                count,
            } => vec![
                V(station.clone()),
                V(function.clone()),
                V(address.clone()),
                D(*buffer),
                V(count.clone()),
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
                    SerialOperand::Data(d) => d.to_string(),
                    SerialOperand::Value(v) => v.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
    pub fn display_value(&self, m: &Memory) -> String {
        let active = m.serial_io.active_count();
        format!("{}: {active} etkin port", self.name())
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for v in self.operands() {
            match v {
                SerialOperand::Data(v) => {
                    out.insert(v);
                }
                SerialOperand::Value(v) => v.devices(out),
            }
        }
        for port in [SerialPort::Com1, SerialPort::Com2, SerialPort::Com3] {
            let p = flags(port);
            out.extend([
                Device {
                    area: Area::M,
                    index: p.request,
                },
                Device {
                    area: Area::M,
                    index: p.done,
                },
                Device {
                    area: Area::M,
                    index: p.ready,
                },
                Device {
                    area: Area::M,
                    index: p.retain,
                },
                Device {
                    area: Area::M,
                    index: p.rtu,
                },
                d(p.protocol),
                d(p.timeout),
                d(p.error_code),
            ]);
        }
        out.extend([
            Device {
                area: Area::M,
                index: 1127,
            },
            Device {
                area: Area::M,
                index: 1129,
            },
            Device {
                area: Area::M,
                index: 1140,
            },
            Device {
                area: Area::M,
                index: 1141,
            },
            Device {
                area: Area::M,
                index: 1161,
            },
            Device {
                area: Area::M,
                index: 1263,
            },
            d(1168),
        ]);
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Self::Raw {
            source,
            send,
            dest,
            receive,
        } = self
        {
            for (base, count) in [(*source, send.read(m)), (*dest, receive.read(m))] {
                if (0..=255).contains(&count) {
                    for i in 0..count as u16 {
                        if let Some(v) = base.index.checked_add(i)
                            && v <= m.data_register_limit.unwrap_or(9999)
                        {
                            out.insert(d(v));
                        }
                    }
                }
            }
        }
        for n in 1050..=1055 {
            out.insert(d(n));
        }
        for n in 1070..=1099 {
            out.insert(d(n));
        }
        for n in 1256..=1311 {
            out.insert(d(n));
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let now = m.timing_io.now_us;
        if now != m.serial_io.now_us {
            SerialBus::advance(m, now);
        }
        let mut s = std::mem::take(&mut m.serial_io);
        s.visit(m, self, power, pc);
        m.serial_io = s;
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SerialPort {
    Com1,
    Com2,
    Com3,
}
impl SerialPort {
    pub fn number(self) -> u8 {
        match self {
            Self::Com1 => 1,
            Self::Com2 => 2,
            Self::Com3 => 3,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialCpu {
    Es2,
    Ss2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerialProfile {
    pub cpu: SerialCpu,
    pub firmware_major: u16,
    pub firmware_minor: u8,
    pub has_com3: bool,
}
impl Default for SerialProfile {
    fn default() -> Self {
        Self {
            cpu: SerialCpu::Es2,
            firmware_major: 4,
            firmware_minor: 2,
            has_com3: false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortInterface {
    Rs232,
    Rs485,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortConfig {
    pub interface: PortInterface,
    pub turnaround_us: u64,
}
impl PortConfig {
    pub fn rs232() -> Self {
        Self {
            interface: PortInterface::Rs232,
            turnaround_us: 1000,
        }
    }
    pub fn rs485() -> Self {
        Self {
            interface: PortInterface::Rs485,
            turnaround_us: 1000,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoilValuePolicy {
    #[default]
    ExamplesOneOn,
    HeaderZeroOn,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum H17ReadPointerPolicy {
    #[default]
    DiagramReadIndex,
    ProseSecondIndex,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BitWritePacking {
    #[default]
    DocumentedPerProtocol,
    PackedWords,
    LowByteWords,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FalseContactPolicy {
    #[default]
    CancelActive,
    ContinueLatched,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RsTimeoutCodePolicy {
    #[default]
    PreserveExcludedRegister,
    WriteK1Candidate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModrdCountPolicy {
    #[default]
    StrictPrinted2To6,
    SingleRegisterCandidate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RtuTimingPolicy {
    #[default]
    StandardV102,
    DeltaPrintedGapCandidate,
}
#[derive(Debug, Clone, Copy)]
struct Flags {
    request: u16,
    ready: u16,
    done: u16,
    error: u16,
    retain: u16,
    rtu: u16,
    protocol: u16,
    timeout: u16,
    error_code: u16,
    end_match: u16,
}
fn flags(port: SerialPort) -> Flags {
    match port {
        SerialPort::Com1 => Flags {
            request: 1312,
            ready: 1313,
            done: 1314,
            error: 1315,
            retain: 1138,
            rtu: 1139,
            protocol: 1036,
            timeout: 1249,
            error_code: 1250,
            end_match: 1167,
        },
        SerialPort::Com2 => Flags {
            request: 1122,
            ready: 1124,
            done: 1123,
            error: 1140,
            retain: 1120,
            rtu: 1143,
            protocol: 1120,
            timeout: 1129,
            error_code: 1130,
            end_match: 1168,
        },
        SerialPort::Com3 => Flags {
            request: 1316,
            ready: 1317,
            done: 1318,
            error: 1319,
            retain: 1136,
            rtu: 1320,
            protocol: 1109,
            timeout: 1252,
            error_code: 1253,
            end_match: 1169,
        },
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxFrame {
    pub port: SerialPort,
    pub pc: u16,
    pub started_at_us: u64,
    pub completed_at_us: u64,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionPhase {
    Transmitting,
    Turnaround,
    Receiving,
}
#[derive(Debug, Clone)]
pub struct TransactionStatus {
    pub pc: u16,
    pub phase: TransactionPhase,
    pub sent_bytes: usize,
    pub received_bytes: usize,
}
#[derive(Debug, Clone)]
enum ReceivePlan {
    Raw {
        dest: Device,
        count: usize,
        low_only: bool,
        silence_us: Option<u64>,
    },
    Modbus {
        request: Frame,
        mode: WireMode,
        raw_dest: Option<Device>,
        converted: Option<Device>,
        raw_packed: bool,
        raw_window: usize,
        broadcast: bool,
    },
}
enum FactoryAction<'a> {
    Drive(&'a crate::drive_comm::DriveAction),
    Servo(&'a crate::servo_comm::ServoAction),
}
impl FactoryAction<'_> {
    fn name(&self) -> &'static str {
        match self {
            Self::Drive(a) => a.name(),
            Self::Servo(a) => a.name(),
        }
    }
}
#[derive(Debug, Clone)]
struct Transaction {
    pc: u16,
    phase: TransactionPhase,
    tx: Vec<u8>,
    tx_at: u64,
    tx_end: u64,
    rx_at: u64,
    character_us: u64,
    frame_gap_us: u64,
    inter_character_us: u64,
    rtu_frame_broken: bool,
    timeout_at: Option<u64>,
    timeout_us: Option<u64>,
    rx: Vec<u8>,
    last_rx: Option<u64>,
    plan: ReceivePlan,
    protocol: u16,
    drive: bool,
    servo: bool,
}
impl Transaction {
    fn clear_received_frame(&mut self) {
        self.rx.clear();
        self.last_rx = None;
        self.rtu_frame_broken = false;
    }
}
#[derive(Debug, Clone)]
struct PortState {
    config: PortConfig,
    active: Option<Transaction>,
    latched_protocol: Option<u16>,
}
#[derive(Debug, Clone, Default)]
pub struct SerialBus {
    pub now_us: u64,
    pub notices: BTreeSet<String>,
    pub coil_value_policy: CoilValuePolicy,
    pub h17_read_pointer_policy: H17ReadPointerPolicy,
    pub bit_write_packing: BitWritePacking,
    pub false_contact_policy: FalseContactPolicy,
    pub rs_timeout_code_policy: RsTimeoutCodePolicy,
    pub modrd_count_policy: ModrdCountPolicy,
    pub rtu_timing_policy: RtuTimingPolicy,
    pub drive_profile: crate::drive_comm::DriveProfile,
    pub servo_profile: crate::servo_comm::ServoProfile,
    external_owners: BTreeMap<SerialPort, u16>,
    profile: SerialProfile,
    ports: BTreeMap<SerialPort, PortState>,
    rx_events: BTreeMap<(u64, u64), (SerialPort, Vec<u8>)>,
    sequence: u64,
    outgoing: VecDeque<TxFrame>,
}
impl SerialBus {
    /// A CPU restart starts a new timestamp epoch. Preserve only explicitly
    /// configured peripherals/profiles; old scheduled bytes and frames cannot
    /// be interpreted as responses to new instructions at CPU time zero.
    pub fn reset_instruction_state(&mut self) {
        for port in self.ports.values_mut() {
            port.active = None;
            port.latched_protocol = None;
        }
        self.rx_events.clear();
        self.external_owners.clear();
        self.outgoing.clear();
        self.now_us = 0;
        self.sequence = 0;
    }
    pub fn restart_cpu_clock(&mut self, elapsed_cpu_us: u64) -> Result<(), String> {
        if elapsed_cpu_us < self.now_us {
            return Err("Seri CPU reset zamanı işlenmiş sanal zamandan önce olamaz".into());
        }
        self.reset_instruction_state();
        Ok(())
    }
    pub fn profile(&self) -> SerialProfile {
        self.profile
    }
    pub fn set_profile(&mut self, profile: SerialProfile) -> Result<(), String> {
        if profile.firmware_minor > 99
            || profile.cpu == SerialCpu::Ss2 && profile.has_com3
            || self.active_count() != 0
            || !profile.has_com3 && self.ports.contains_key(&SerialPort::Com3)
        {
            return Err(
                "Seri CPU profili etkin işlem/COM3 yapılandırmasıyla değiştirilemez".into(),
            );
        }
        self.profile = profile;
        Ok(())
    }
    pub fn configure(&mut self, port: SerialPort, config: PortConfig) -> Result<(), String> {
        if port == SerialPort::Com3 && !self.profile.has_com3 {
            return Err("COM3 bu açık CPU donanım profilinde yok".into());
        }
        if port != SerialPort::Com1 && config.interface != PortInterface::Rs485 {
            return Err("COM2/COM3 RS485 arabirimi gerektirir".into());
        }
        if self.ports.contains_key(&port) {
            return Err("Seri port önce açıkça kaldırılmalı".into());
        }
        if config.turnaround_us > 1_000_000 {
            return Err("Sanal seri dönüş beklemesi1s sınırını aşıyor".into());
        }
        self.ports.insert(
            port,
            PortState {
                config,
                active: None,
                latched_protocol: None,
            },
        );
        self.notices
            .insert("Sanal seri port: açık TX/RX kuyruğu; fiziksel hat/otomatik echo yok".into());
        Ok(())
    }
    pub fn remove(&mut self, port: SerialPort) -> Result<(), String> {
        if self.ports.get(&port).is_some_and(|p| p.active.is_some())
            || self.external_owners.contains_key(&port)
        {
            return Err("Etkin seri port önce iptal edilmeli".into());
        }
        self.ports.remove(&port);
        Ok(())
    }
    pub fn configured_ports(&self) -> impl Iterator<Item = (SerialPort, &PortConfig)> {
        self.ports.iter().map(|(&p, s)| (p, &s.config))
    }
    pub fn active_count(&self) -> usize {
        self.ports.values().filter(|v| v.active.is_some()).count() + self.external_owners.len()
    }
    /// Shared ownership for compound protocols such as DMV. A reservation does
    /// not manufacture serial frames or a successful peripheral response.
    pub fn reserve_external(&mut self, port: SerialPort, owner_pc: u16) -> Result<(), String> {
        let state = self.ports.get(&port).ok_or("Seri port yapılandırılmamış")?;
        if state.active.is_some()
            || self
                .external_owners
                .get(&port)
                .is_some_and(|pc| *pc != owner_pc)
        {
            return Err("Seri port başka bir komut tarafından kullanılıyor".into());
        }
        self.external_owners.insert(port, owner_pc);
        Ok(())
    }
    pub fn release_external(&mut self, owner_pc: u16) {
        self.external_owners.retain(|_, pc| *pc != owner_pc);
    }
    pub fn external_owner(&self, port: SerialPort) -> Option<u16> {
        self.external_owners.get(&port).copied()
    }
    pub fn transaction(&self, port: SerialPort) -> Option<TransactionStatus> {
        let t = self.ports.get(&port)?.active.as_ref()?;
        Some(TransactionStatus {
            pc: t.pc,
            phase: t.phase,
            sent_bytes: if t.phase == TransactionPhase::Transmitting {
                ((self.now_us.saturating_sub(t.tx_at)) / t.character_us).min(t.tx.len() as u64)
                    as usize
            } else {
                t.tx.len()
            },
            received_bytes: t.rx.len(),
        })
    }
    pub fn outgoing_frames(&self) -> &VecDeque<TxFrame> {
        &self.outgoing
    }
    pub fn pop_tx_frame(&mut self) -> Option<TxFrame> {
        self.outgoing.pop_front()
    }
    pub fn queue_received(
        &mut self,
        port: SerialPort,
        at_us: u64,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        if !self.ports.contains_key(&port)
            || at_us < self.now_us
            || bytes.is_empty()
            || bytes.len() > 1024
            || self.rx_events.len() >= 4096
        {
            return Err("Seri RX olayı yapılandırılmış port, gelecek zaman,1..1024byte ve4096olay sınırı gerektirir".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Seri olay sırası taştı")?;
        self.rx_events.insert((at_us, self.sequence), (port, bytes));
        Ok(())
    }
    pub fn begin_scan(m: &mut Memory) {
        Self::service_controls(m);
    }
    pub fn end_scan(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.serial_io);
        for (&port, state) in &mut s.ports {
            let f = flags(port);
            if !m.system_bit(f.retain) {
                let value = m.word(d(f.protocol)) as u16;
                if state.active.as_ref().is_some_and(|t| t.protocol != value) {
                    s.notices.insert("Etkin seri işlem protokolü snapshot; retainOFF END değişikliği sonraki işlemde uygulanır, fiziksel orta-paket değişim kanıtı yok".into());
                }
                state.latched_protocol = Some(value);
            }
        }
        m.serial_io = s;
    }
    pub fn service_controls(m: &mut Memory) {
        if m.system_bit(1125) {
            let mut s = std::mem::take(&mut m.serial_io);
            if let Some(state) = s.ports.get_mut(&SerialPort::Com2) {
                state.active = None;
                idle(m, SerialPort::Com2);
                m.set_system(1127, false);
                m.set_system(1123, false);
            }
            m.serial_io = s;
        }
    }
    pub fn abort(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.serial_io);
        for (&port, state) in &mut s.ports {
            state.active = None;
            idle(m, port);
            m.set_system(flags(port).request, false);
        }
        m.serial_io = s;
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.ports
            .values()
            .filter_map(|p| p.active.as_ref())
            .filter_map(|t| match t.phase {
                TransactionPhase::Transmitting => Some(t.tx_end),
                TransactionPhase::Turnaround => Some(t.rx_at),
                TransactionPhase::Receiving => {
                    let silence = match t.plan {
                        ReceivePlan::Raw {
                            silence_us: Some(s),
                            ..
                        } => t.last_rx.and_then(|v| v.checked_add(s)?.checked_add(1)),
                        ReceivePlan::Modbus {
                            mode: WireMode::Rtu | WireMode::RtuSwappedCrc,
                            ..
                        } => t.last_rx.and_then(|v| v.checked_add(t.frame_gap_us)),
                        _ => None,
                    };
                    t.timeout_at.into_iter().chain(silence).min()
                }
            })
            .chain(self.rx_events.first_key_value().map(|(k, _)| k.0))
            .min()
    }
    pub fn advance(m: &mut Memory, at_us: u64) {
        let mut s = std::mem::take(&mut m.serial_io);
        if at_us < s.now_us {
            limit(m, "Seri saat geriye gidiyor");
            m.serial_io = s;
            return;
        }
        let mut steps = 0;
        loop {
            let next = s.next_deadline_us();
            if next.is_none_or(|v| v > at_us) {
                break;
            }
            let at = next.unwrap();
            s.now_us = at;
            steps += 1;
            if steps > 8192 {
                limit(
                    m,
                    "Seri olay sınırı aşıldı; daha küçük sanal zaman adımı gerekli",
                );
                break;
            }
            s.service_due(m);
            let keys: Vec<_> = s
                .rx_events
                .range(..=(at, u64::MAX))
                .map(|(k, _)| *k)
                .collect();
            for key in keys {
                let (port, bytes) = s.rx_events.remove(&key).unwrap();
                s.receive(m, port, &bytes);
            }
            s.service_due(m);
        }
        if steps <= 8192 {
            s.now_us = at_us;
            s.service_due(m);
        }
        m.serial_io = s;
    }
    /// Drive commands share the exact same port owner, clock and byte scheduler.
    pub(crate) fn visit_drive(
        &mut self,
        m: &mut Memory,
        action: &crate::drive_comm::DriveAction,
        power: bool,
        pc: u16,
    ) {
        self.visit_factory(m, FactoryAction::Drive(action), power, pc);
    }
    pub(crate) fn visit_servo(
        &mut self,
        m: &mut Memory,
        action: &crate::servo_comm::ServoAction,
        power: bool,
        pc: u16,
    ) {
        self.visit_factory(m, FactoryAction::Servo(action), power, pc);
    }
    fn visit_factory(&mut self, m: &mut Memory, action: FactoryAction<'_>, power: bool, pc: u16) {
        use crate::drive_comm::{DriveCarrier, DriveCommand, DrivePortPolicy};
        let drive = matches!(&action, FactoryAction::Drive(_));
        let ports = if drive {
            self.drive_profile.ports
        } else {
            self.servo_profile.ports
        };
        if !power {
            if self.false_contact_policy == FalseContactPolicy::CancelActive {
                for (&port, state) in &mut self.ports {
                    if state
                        .active
                        .as_ref()
                        .is_some_and(|t| t.pc == pc && t.drive == drive)
                    {
                        state.active = None;
                        idle(m, port);
                        m.set_system(flags(port).request, false);
                    }
                }
            }
            return;
        }
        let selected = match ports {
            DrivePortPolicy::Com2Only => m.system_bit(1122).then_some(SerialPort::Com2),
            DrivePortPolicy::Com3ThenCom2Candidate => [SerialPort::Com3, SerialPort::Com2]
                .into_iter()
                .find(|p| m.system_bit(flags(*p).request)),
        };
        let Some(port) = selected else {
            return;
        };
        if port == SerialPort::Com2 && m.system_bit(1125) {
            return;
        }
        let Some(state) = self.ports.get(&port) else {
            limit(
                m,
                "Sürücü komutu açık yapılandırılmış RS485 port gerektirir",
            );
            return;
        };
        if state.active.is_some() || self.external_owners.contains_key(&port) {
            return;
        }
        if state.config.interface != PortInterface::Rs485 {
            limit(m, "Sürücü komutu RS485 arayüz gerektirir");
            return;
        }
        if m.system_bit(if port == SerialPort::Com2 {
            1127
        } else {
            flags(port).done
        }) {
            return;
        }
        let result = (|| -> Result<Transaction, String> {
            let f = flags(port);
            let protocol = if m.system_bit(f.retain) {
                state
                    .latched_protocol
                    .unwrap_or(m.word(d(f.protocol)) as u16)
            } else {
                m.word(d(f.protocol)) as u16
            };
            let character_us = character_us(port, protocol)?;
            let (inter_character_us, frame_gap_us) = rtu_timing(protocol, self.rtu_timing_policy);
            let timeout = m.word(d(f.timeout));
            if timeout < 0 {
                return Err("Seri zaman aşımı negatif olamaz".into());
            }
            let timeout_us = (timeout > 0).then_some(
                if port == SerialPort::Com2 {
                    timeout as u64
                } else {
                    timeout.max(50) as u64
                } * 1000,
            );
            let mode = if m.system_bit(f.rtu) {
                WireMode::Rtu
            } else {
                WireMode::Ascii
            };
            let (request, converted) = match &action {
                FactoryAction::Drive(a) => {
                    let request = a.prepare(m, self.drive_profile)?;
                    let converted = if a.command == DriveCommand::Rdst {
                        span(m, d(1070), 5)?;
                        Some(d(1070))
                    } else {
                        None
                    };
                    (request, converted)
                }
                FactoryAction::Servo(a) => a.prepare(m, self.servo_profile)?,
            };
            let tx = serial_protocol::encode(mode, &request)?;
            let tx_end = self
                .now_us
                .checked_add(
                    character_us
                        .checked_mul(tx.len() as u64)
                        .ok_or("Sürücü TX süresi taştı")?,
                )
                .ok_or("Sürücü TX zamanı taştı")?;
            let rx_at = tx_end
                .checked_add(state.config.turnaround_us)
                .ok_or("Sürücü dönüş zamanı taştı")?;
            if timeout_us.is_some_and(|v| rx_at.checked_add(v).is_none()) {
                return Err("Sürücü zaman aşımı zamanı taştı".into());
            }
            let broadcast = request.station == 0;
            Ok(Transaction {
                pc,
                phase: TransactionPhase::Transmitting,
                tx,
                tx_at: self.now_us,
                tx_end,
                rx_at,
                character_us,
                frame_gap_us,
                inter_character_us,
                rtu_frame_broken: false,
                timeout_at: None,
                timeout_us,
                rx: Vec::new(),
                last_rx: None,
                plan: ReceivePlan::Modbus {
                    request,
                    mode,
                    raw_dest: None,
                    converted,
                    raw_packed: false,
                    raw_window: 0,
                    broadcast,
                },
                protocol,
                drive,
                servo: !drive,
            })
        })();
        let t = match result {
            Ok(v) => v,
            Err(e) => {
                limit(m, format!("{} STEP{pc}: {e}", action.name()));
                return;
            }
        };
        if drive && self.drive_profile.carrier == DriveCarrier::FactoryRegisterCandidate {
            self.notices.insert("Sürücü FactoryRegisterCandidate: REV/STOP/JOG FC06/FC16 taşıyıcısı özgün DVP paket eşitliği değildir".into());
        }
        if drive {
            self.notices.insert("Sürücü M1177 polaritesi ve port politikası açık profildir; Tam TX outgoing_frames içinde; doğrulanmamış handy debug bankası yazılmaz".into());
        } else {
            self.notices.insert("ASDRW StandardModbusCandidate: FC03/06/16 taşıyıcısı açık sanal ASDA profili; özgün servo paket/donanım eşitliği değildir".into());
        }
        if self.outgoing.len() >= 256 {
            self.outgoing.pop_front();
        }
        self.outgoing.push_back(TxFrame {
            port,
            pc,
            started_at_us: self.now_us,
            completed_at_us: t.tx_end,
            bytes: t.tx.clone(),
        });
        m.set_system(flags(port).ready, false);
        if port == SerialPort::Com2 {
            m.set_system(1121, false);
            m.set_system(1128, true);
            m.set_word(d(1122), t.tx.len() as i16);
        }
        let state = self.ports.get_mut(&port).unwrap();
        state.latched_protocol = Some(t.protocol);
        state.active = Some(t);
    }
    fn visit(&mut self, m: &mut Memory, a: &SerialIoAction, power: bool, pc: u16) {
        if !power {
            if self.false_contact_policy == FalseContactPolicy::CancelActive {
                for (&port, state) in &mut self.ports {
                    if state.active.as_ref().is_some_and(|t| t.pc == pc) {
                        state.active = None;
                        idle(m, port);
                        m.set_system(flags(port).request, false);
                    }
                }
            }
            return;
        }
        let selected = if matches!(
            a,
            SerialIoAction::Read { .. } | SerialIoAction::Write { .. }
        ) {
            m.system_bit(1122).then_some(SerialPort::Com2)
        } else {
            [SerialPort::Com1, SerialPort::Com3, SerialPort::Com2]
                .into_iter()
                .find(|p| m.system_bit(flags(*p).request))
        };
        let Some(port) = selected else {
            return;
        };
        if port == SerialPort::Com2 && m.system_bit(1125) {
            return;
        }
        let f = flags(port);
        let Some(state) = self.ports.get(&port) else {
            limit(
                m,
                format!("{} açık sanal port yapılandırması yok", a.name()),
            );
            return;
        };
        if state.active.is_some() || self.external_owners.contains_key(&port) {
            return;
        }
        let done = if port == SerialPort::Com2 && !matches!(a, SerialIoAction::Raw { .. }) {
            1127
        } else {
            f.done
        };
        if m.system_bit(done) {
            return;
        }
        if let Err(e) = self.start(m, a, port, pc) {
            limit(m, format!("{} STEP{pc}: {e}", a.name()));
        }
    }
    fn start(
        &mut self,
        m: &mut Memory,
        a: &SerialIoAction,
        port: SerialPort,
        pc: u16,
    ) -> Result<(), String> {
        let f = flags(port);
        let state = self.ports.get(&port).unwrap();
        let config = state.config;
        let protocol = if m.system_bit(f.retain) {
            state
                .latched_protocol
                .unwrap_or(m.word(d(f.protocol)) as u16)
        } else {
            m.word(d(f.protocol)) as u16
        };
        let character_us = character_us(port, protocol)?;
        let (inter_character_us, frame_gap_us) = rtu_timing(protocol, self.rtu_timing_policy);
        let timeout = m.word(d(f.timeout));
        if timeout < 0 {
            return Err("Seri zaman aşımı negatif olamaz".into());
        }
        let timeout_us = (timeout > 0).then_some(
            if port == SerialPort::Com2 {
                timeout as u64
            } else {
                timeout.max(50) as u64
            } * 1000,
        );
        let mode = if m.system_bit(f.rtu) {
            WireMode::Rtu
        } else {
            WireMode::Ascii
        };
        let (tx, plan) = match a {
            SerialIoAction::Raw {
                source,
                send,
                dest,
                receive,
            } => {
                let send = scalar(m, send)?;
                let receive = scalar(m, receive)?;
                if !(0..=255).contains(&send) || !(0..=255).contains(&receive) {
                    return Err("RS uzunlukları byte olarak0..255 olmalı".into());
                }
                let low = port != SerialPort::Com2 || m.system_bit(1161);
                span(
                    m,
                    *source,
                    if low {
                        send as usize
                    } else {
                        (send as usize).div_ceil(2)
                    },
                )?;
                span(
                    m,
                    *dest,
                    if low {
                        receive as usize
                    } else {
                        (receive as usize).div_ceil(2)
                    },
                )?;
                let silence = port == SerialPort::Com2 && m.system_bit(1263);
                if silence
                    && ((self.profile.firmware_major, self.profile.firmware_minor) < (4, 2)
                        || receive == 0
                        || receive >= 255)
                {
                    return Err(
                        "RS sessizlik sonlandırma ES2/SS2V4.02+, receive1..254 gerektirir".into(),
                    );
                }
                let gap = m.word(d(1168));
                if silence && gap <= 0 {
                    return Err("RS sessizlik aralığı pozitif ms olmalı".into());
                }
                let mut tx = read_bytes(m, *source, send as usize, low);
                if port == SerialPort::Com2 && send > 0 {
                    let framing = framing(m, protocol);
                    if let Some(v) = framing.0 {
                        tx.insert(0, v);
                    }
                    tx.extend(framing.1);
                }
                let plan = ReceivePlan::Raw {
                    dest: *dest,
                    count: receive as usize,
                    low_only: low,
                    silence_us: silence.then_some(gap as u64 * 1000),
                };
                (tx, plan)
            }
            _ => self.build_modbus(m, a, port, mode)?,
        };
        let tx_time = character_us
            .checked_mul(tx.len() as u64)
            .ok_or("Seri TX süresi taştı")?;
        let tx_end = self
            .now_us
            .checked_add(tx_time)
            .ok_or("Seri zaman çizgisi taştı")?;
        let rx_at = tx_end
            .checked_add(if tx.is_empty() {
                0
            } else {
                config.turnaround_us
            })
            .ok_or("Seri dönüş zamanı taştı")?;
        if timeout_us.is_some_and(|v| rx_at.checked_add(v).is_none()) {
            return Err("Seri zaman aşımı son tarihi sanal saat sınırını aşıyor".into());
        }
        let phase = if tx.is_empty() {
            TransactionPhase::Receiving
        } else {
            TransactionPhase::Transmitting
        };
        let t = Transaction {
            pc,
            phase,
            tx: tx.clone(),
            tx_at: self.now_us,
            tx_end,
            rx_at,
            character_us,
            frame_gap_us,
            inter_character_us,
            rtu_frame_broken: false,
            timeout_at: if tx.is_empty() {
                timeout_us.and_then(|v| self.now_us.checked_add(v))
            } else {
                None
            },
            timeout_us,
            rx: Vec::new(),
            last_rx: None,
            plan,
            protocol,
            drive: false,
            servo: false,
        };
        self.publish_tx(m, a, port, &t)?;
        if !tx.is_empty() {
            if self.outgoing.len() >= 256 {
                self.outgoing.pop_front();
            }
            self.outgoing.push_back(TxFrame {
                port,
                pc,
                started_at_us: self.now_us,
                completed_at_us: tx_end,
                bytes: tx,
            });
        }
        m.set_system(f.ready, phase == TransactionPhase::Receiving);
        if port == SerialPort::Com2 {
            m.set_system(1121, phase != TransactionPhase::Transmitting);
            m.set_system(1128, true);
            m.set_word(d(1122), t.tx.len() as i16);
            if let ReceivePlan::Raw {
                count, silence_us, ..
            } = t.plan
            {
                m.set_word(
                    d(1123),
                    if silence_us.is_some() {
                        0
                    } else {
                        count as i16
                    },
                );
            }
        }
        let state = self.ports.get_mut(&port).unwrap();
        state.latched_protocol = Some(protocol);
        state.active = Some(t);
        self.service_due(m);
        Ok(())
    }
    fn build_modbus(
        &mut self,
        m: &Memory,
        a: &SerialIoAction,
        port: SerialPort,
        mode: WireMode,
    ) -> Result<(Vec<u8>, ReceivePlan), String> {
        if let SerialIoAction::Exchange { function, .. } = a
            && !matches!(scalar(m, function)?, 1 | 2 | 3 | 4 | 5 | 6 | 15 | 16 | 23)
        {
            return Err(
                "MODRW fonksiyonu belgeli kodlardan biri olmalı; daraltılarak çevrilemez".into(),
            );
        }
        let (station, function, address, buffer, count) = match a {
            SerialIoAction::Read {
                station,
                address,
                count,
            } => (
                scalar(m, station)?,
                3,
                scalar(m, address)? as u16,
                None,
                scalar(m, count)?,
            ),
            SerialIoAction::Write {
                station,
                address,
                value,
            } => (
                scalar(m, station)?,
                6,
                scalar(m, address)? as u16,
                None,
                scalar(m, value)?,
            ),
            SerialIoAction::Exchange {
                station,
                function,
                address,
                buffer,
                count,
            } => (
                scalar(m, station)?,
                scalar(m, function)? as u8,
                scalar(m, address)? as u16,
                Some(*buffer),
                scalar(m, count)?,
            ),
            _ => unreachable!(),
        };
        if !(0..=254).contains(&station)
            || !matches!(function, 1 | 2 | 3 | 4 | 5 | 6 | 15 | 16 | 23)
        {
            return Err("Modbus istasyonu0..254 ve belgeli fonksiyon gerekli".into());
        }
        if station == 0 && matches!(function, 1 | 2 | 3 | 4 | 23) {
            return Err("Modbus okuma broadcast istasyon0 kullanamaz".into());
        }
        let version = (self.profile.firmware_major, self.profile.firmware_minor);
        let threshold = match (function, self.profile.cpu) {
            (1, SerialCpu::Es2) => (3, 28),
            (1, SerialCpu::Ss2) => (3, 24),
            (4, SerialCpu::Es2) => (2, 60),
            (4, SerialCpu::Ss2) => (2, 40),
            (23, SerialCpu::Es2) => (3, 20),
            (23, SerialCpu::Ss2) => (3, 0),
            _ => (0, 0),
        };
        if version < threshold {
            return Err("Modbus fonksiyonu seçilen CPU firmware'inde belgelenmedi".into());
        }
        let handy = matches!(
            a,
            SerialIoAction::Read { .. } | SerialIoAction::Write { .. }
        );
        if matches!(a, SerialIoAction::Read { .. }) && count == 1 {
            if self.modrd_count_policy == ModrdCountPolicy::StrictPrinted2To6 {
                return Err("MODRD basılı aralık1<n<=6; n1 için açık SingleRegisterCandidate profili gerekli".into());
            }
            self.notices.insert("MODRD tek-register adayı: standart Modbus'a uygundur; PDF basılı1<n sınırıyla çelişir".into());
        }
        let mut data = Vec::new();
        let mut read_count = 0usize;
        let mut actual_address = address;
        let mut raw_dest = None;
        let mut converted = None;
        if function == 23 {
            let SerialIoAction::Exchange {
                address: address_arg,
                count: count_arg,
                buffer: params,
                ..
            } = a
            else {
                return Err("H17 MODRW gerektirir".into());
            };
            let addr_base = address_arg
                .base()
                .ok_or("H17 adresleri D/D+1 çiftinden okunmalı")?;
            let count_base = count_arg
                .base()
                .ok_or("H17 sayıları D/D+1 çiftinden okunmalı")?;
            span(m, addr_base, 2)?;
            span(m, count_base, 2)?;
            span(m, *params, 2)?;
            actual_address = m.word(addr_base) as u16;
            let write_addr = m.word(d(addr_base.index + 1)) as u16;
            let read_index = m.word(*params) as u16;
            let write_index = m.word(d(params.index + 1)) as u16;
            let nr = m.word(count_base);
            let nw = m.word(d(count_base.index + 1));
            if !(1..=16).contains(&nr) || !(1..=16).contains(&nw) {
                return Err("H17 iki veri sayısı1..16 olmalı".into());
            }
            let read_base = d(
                if port != SerialPort::Com2
                    && self.h17_read_pointer_policy == H17ReadPointerPolicy::ProseSecondIndex
                {
                    write_index
                } else {
                    read_index
                },
            );
            let write_base = d(write_index);
            span(m, write_base, nw as usize)?;
            read_count = nr as usize;
            push_word(&mut data, actual_address);
            push_word(&mut data, nr as u16);
            push_word(&mut data, write_addr);
            push_word(&mut data, nw as u16);
            data.push(nw as u8 * 2);
            for n in 0..nw as u16 {
                push_word(&mut data, m.word(d(write_base.index + n)) as u16);
            }
            if port == SerialPort::Com2 {
                raw_dest = Some(read_base);
                converted = Some(d(1296));
            } else {
                converted = Some(read_base);
            }
            self.notices.insert("H17: varsayılan diagram-read-index; S+1 prose çelişkisi açık alternatif profilde korunur".into());
        } else {
            push_word(&mut data, actual_address);
            match function {
                1..=4 => {
                    let max = if handy {
                        6
                    } else if matches!(function, 1 | 2) {
                        64
                    } else if port == SerialPort::Com2 && mode == WireMode::Ascii {
                        8
                    } else {
                        16
                    };
                    if count < 1 || count > max {
                        return Err("Modbus okuma sayısı port/protokol sınırını aşıyor".into());
                    }
                    read_count = count as usize;
                    push_word(&mut data, count as u16);
                    if handy {
                        raw_dest = Some(d(1070));
                        if mode == WireMode::Ascii {
                            converted = Some(d(1050));
                        }
                    } else if port == SerialPort::Com2 {
                        raw_dest = buffer;
                        converted = Some(d(1296));
                    } else {
                        converted = buffer;
                    }
                }
                5 => {
                    if !matches!(count, 0 | 1) {
                        return Err("H05 n0/1 olmalı".into());
                    }
                    let on = match self.coil_value_policy {
                        CoilValuePolicy::ExamplesOneOn => count == 1,
                        CoilValuePolicy::HeaderZeroOn => count == 0,
                    };
                    push_word(&mut data, if on { 0xff00 } else { 0 });
                    self.notices.insert(
                        "H05: örnekler1=ON; genelbaşlık0=ON çelişkisi namedcoilpolicy ile ayrılır"
                            .into(),
                    );
                }
                6 => {
                    let value = if handy {
                        count as u16
                    } else {
                        let base = buffer.ok_or("H06 veri D tamponu eksik")?;
                        span(m, base, 1)?;
                        m.word(base) as u16
                    };
                    push_word(&mut data, value);
                }
                15 | 16 => {
                    let max = if function == 15 {
                        64
                    } else if port == SerialPort::Com2 && mode == WireMode::Ascii {
                        8
                    } else {
                        16
                    };
                    if count < 1 || count > max {
                        return Err("Modbus yazma sayısı port/protokol sınırını aşıyor".into());
                    }
                    let base = buffer.ok_or("Modbus yazma tamponu eksik")?;
                    push_word(&mut data, count as u16);
                    if function == 16 {
                        span(m, base, count as usize)?;
                        data.push(count as u8 * 2);
                        for n in 0..count as u16 {
                            push_word(&mut data, m.word(d(base.index + n)) as u16);
                        }
                    } else {
                        let bytes = (count as usize).div_ceil(8);
                        let low = match self.bit_write_packing {
                            BitWritePacking::DocumentedPerProtocol => mode != WireMode::Ascii,
                            BitWritePacking::PackedWords => false,
                            BitWritePacking::LowByteWords => true,
                        };
                        span(m, base, if low { bytes } else { bytes.div_ceil(2) })?;
                        data.push(bytes as u8);
                        let mut payload = read_bytes(m, base, bytes, low);
                        if count % 8 != 0 {
                            *payload.last_mut().unwrap() &= (1u8 << (count % 8)) - 1;
                        }
                        data.extend(payload);
                    }
                }
                _ => unreachable!(),
            }
        }
        // MODWR shares COM2's fixed acknowledgement bank with MODRW writes.
        if port == SerialPort::Com2 && matches!(function, 5 | 6 | 15 | 16) {
            raw_dest = Some(d(1070));
        }
        let request = Frame {
            station: station as u8,
            function,
            data,
        };
        let tx = serial_protocol::encode(mode, &request)?;
        let response_data = if matches!(function, 1 | 2) {
            read_count.div_ceil(8)
        } else {
            read_count * 2
        };
        let response_len = if matches!(function, 1 | 2 | 3 | 4 | 23) {
            response_data + 5
        } else {
            8
        };
        if let Some(base) = raw_dest {
            let words = if mode == WireMode::Ascii {
                response_len - 1
            } else {
                response_len
            };
            span(m, base, words)?;
        }
        if let Some(base) = converted {
            span(
                m,
                base,
                if matches!(function, 1 | 2) {
                    read_count.div_ceil(16)
                } else {
                    read_count
                },
            )?;
        }
        if actual_address as usize + read_count > 65536 {
            return Err("Modbus okuma adres aralığı16bit sınırını aşıyor".into());
        }
        Ok((
            tx,
            ReceivePlan::Modbus {
                request,
                mode,
                raw_dest,
                converted,
                raw_packed: mode == WireMode::Ascii,
                raw_window: if raw_dest == Some(d(1070)) { 16 } else { 1024 },
                broadcast: station == 0,
            },
        ))
    }
    fn publish_tx(
        &mut self,
        m: &mut Memory,
        a: &SerialIoAction,
        port: SerialPort,
        t: &Transaction,
    ) -> Result<(), String> {
        if port != SerialPort::Com2 || matches!(a, SerialIoAction::Raw { .. }) {
            return Ok(());
        }
        let first = if matches!(a, SerialIoAction::Exchange { .. }) {
            1256
        } else {
            1089
        };
        let cap = if first == 1256 { 40 } else { 11 };
        let mode = match t.plan {
            ReceivePlan::Modbus { mode, .. } => mode,
            _ => return Ok(()),
        };
        let body = body_bytes(mode, &t.tx);
        let words = if mode == WireMode::Ascii {
            body.len().div_ceil(2)
        } else {
            body.len()
        };
        if words > cap {
            self.notices.insert("Modbus TX debug penceresi kapasitesiyle sınırlıdır; tam TX çerçevesi outgoing_frames içinde korunur".into());
        }
        span(m, d(first), words.min(cap))?;
        write_bytes(m, d(first), &body, mode != WireMode::Ascii, cap);
        Ok(())
    }
    fn service_due(&mut self, m: &mut Memory) {
        let ports: Vec<_> = self.ports.keys().copied().collect();
        for port in ports {
            let Some(mut t) = self.ports.get_mut(&port).unwrap().active.take() else {
                continue;
            };
            let f = flags(port);
            let mut finished = false;
            if t.phase == TransactionPhase::Transmitting && self.now_us >= t.tx_end {
                m.set_system(f.request, false);
                if port == SerialPort::Com2 {
                    m.set_system(1121, true);
                    m.set_word(d(1122), 0);
                }
                t.phase = TransactionPhase::Turnaround;
                if matches!(
                    t.plan,
                    ReceivePlan::Modbus {
                        broadcast: true,
                        ..
                    } | ReceivePlan::Raw { count: 0, .. }
                ) {
                    self.complete(m, port, &t);
                    finished = true;
                }
            }
            if !finished && t.phase == TransactionPhase::Turnaround && self.now_us >= t.rx_at {
                t.phase = TransactionPhase::Receiving;
                t.timeout_at = t.timeout_us.and_then(|v| t.rx_at.checked_add(v));
                m.set_system(f.ready, true);
            }
            if !finished && t.phase == TransactionPhase::Receiving {
                if matches!(t.plan, ReceivePlan::Raw { count: 0, .. }) {
                    self.complete(m, port, &t);
                    finished = true;
                }
                let silence = match t.plan {
                    ReceivePlan::Raw {
                        silence_us: Some(v),
                        ..
                    } => t
                        .last_rx
                        .is_some_and(|at| self.now_us > at.saturating_add(v)),
                    _ => false,
                };
                let rtu_gap = matches!(
                    t.plan,
                    ReceivePlan::Modbus {
                        mode: WireMode::Rtu | WireMode::RtuSwappedCrc,
                        ..
                    }
                ) && t
                    .last_rx
                    .is_some_and(|at| self.now_us >= at.saturating_add(t.frame_gap_us));
                if silence {
                    if self.commit_raw(m, port, &t).is_ok() {
                        self.complete(m, port, &t);
                    }
                    finished = true;
                } else if rtu_gap && !t.rx.is_empty() {
                    if t.timeout_at.is_some_and(|v| self.now_us >= v) {
                        transaction_error(m, port, &t, Failure::Timeout);
                        idle(m, port);
                        finished = true;
                    } else {
                        finished = self.finish_modbus(m, port, &t);
                        if !finished {
                            t.clear_received_frame();
                        }
                    }
                } else if t.timeout_at.is_some_and(|v| self.now_us >= v) {
                    if port == SerialPort::Com2
                        && matches!(t.plan, ReceivePlan::Raw { .. })
                        && self.rs_timeout_code_policy
                            == RsTimeoutCodePolicy::PreserveExcludedRegister
                    {
                        m.set_system(1129, true);
                        self.notices.insert("RS timeout: M1129 kesin; PDF D1130'ı RS'den dışladığı için varsayılan profil önceki D1130'ı korur".into());
                    } else {
                        transaction_error(m, port, &t, Failure::Timeout);
                    }
                    idle(m, port);
                    finished = true;
                }
            }
            if !finished {
                self.ports.get_mut(&port).unwrap().active = Some(t);
            }
        }
    }
    fn receive(&mut self, m: &mut Memory, port: SerialPort, bytes: &[u8]) {
        let Some(mut t) = self.ports.get_mut(&port).and_then(|p| p.active.take()) else {
            self.notices.insert(
                "Etkin işlem dışındaki seri RX verisi eşleştirilmedi; sonraki isteğe taşınmadı"
                    .into(),
            );
            return;
        };
        if t.phase != TransactionPhase::Receiving {
            self.notices.insert(
                "RX verisi TX/dönüş beklemesi sırasında geldi; açık yerel profil bu veriyi almaz"
                    .into(),
            );
            self.ports.get_mut(&port).unwrap().active = Some(t);
            return;
        }
        if t.rx.len() + bytes.len() > 1024 {
            transaction_error(m, port, &t, Failure::Length);
            idle(m, port);
            return;
        }
        let is_rtu = matches!(
            t.plan,
            ReceivePlan::Modbus {
                mode: WireMode::Rtu | WireMode::RtuSwappedCrc,
                ..
            }
        );
        if is_rtu {
            if self.now_us.checked_add(t.frame_gap_us).is_none() {
                limit(m, "RTU çerçeve sonu sanal zaman sınırını aşıyor");
                idle(m, port);
                return;
            }
            if t.last_rx
                .is_some_and(|last| self.now_us.saturating_sub(last) > t.inter_character_us)
            {
                t.rtu_frame_broken = true;
            }
        }
        t.rx.extend(bytes);
        t.last_rx = Some(self.now_us);
        let mut finished = false;
        match t.plan {
            ReceivePlan::Raw {
                count, silence_us, ..
            } => {
                if port == SerialPort::Com2 {
                    m.set_word(
                        d(1123),
                        if silence_us.is_some() {
                            t.rx.len() as i16
                        } else {
                            count.saturating_sub(t.rx.len()) as i16
                        },
                    );
                }
                if silence_us.is_none() {
                    for &v in bytes {
                        if v == m.word(d(flags(port).end_match)) as u8 {
                            let id = InterruptId::Communication {
                                port: port.number(),
                            };
                            if m.interrupts.definitions().contains_key(&id) {
                                let _ = m.interrupts.request(id, self.now_us);
                            }
                        }
                    }
                }
                if t.rx.len() >= count {
                    t.rx.truncate(count);
                    if self.commit_raw(m, port, &t).is_ok() {
                        self.complete(m, port, &t);
                    }
                    finished = true;
                }
            }
            ReceivePlan::Modbus { mode, .. } => {
                // RTU length is only a validation check. The complete frame
                // is not available until the actual t3.5 silence boundary.
                let boundary = mode == WireMode::Ascii && t.rx.ends_with(b"\r\n");
                if boundary {
                    finished = self.finish_modbus(m, port, &t);
                    if !finished {
                        t.clear_received_frame();
                    }
                }
            }
        }
        if !finished {
            self.ports.get_mut(&port).unwrap().active = Some(t);
        }
    }
    fn commit_raw(&self, m: &mut Memory, port: SerialPort, t: &Transaction) -> Result<(), String> {
        let ReceivePlan::Raw {
            dest,
            low_only,
            silence_us,
            ..
        } = t.plan
        else {
            return Ok(());
        };
        span(
            m,
            dest,
            if low_only {
                t.rx.len()
            } else {
                t.rx.len().div_ceil(2)
            },
        )?;
        write_bytes(m, dest, &t.rx, low_only, usize::MAX);
        if port == SerialPort::Com2 {
            m.set_word(
                d(1123),
                if silence_us.is_some() {
                    t.rx.len() as i16
                } else {
                    0
                },
            );
            if silence_us.is_some() {
                m.set_system(1263, false);
            }
        }
        Ok(())
    }
    fn finish_modbus(&mut self, m: &mut Memory, port: SerialPort, t: &Transaction) -> bool {
        if t.rtu_frame_broken {
            transaction_error(m, port, t, Failure::Length);
            self.notices.insert(
                "RTU çerçevesi t1.5 karakter aralığını aştı; çerçevenin tamamı atıldı".into(),
            );
            idle(m, port);
            return true;
        }
        let ReceivePlan::Modbus {
            request,
            mode,
            raw_dest,
            converted,
            raw_packed,
            raw_window,
            ..
        } = &t.plan
        else {
            return true;
        };
        if expected_modbus_len(&t.rx, *mode).is_some_and(|n| n != t.rx.len()) {
            transaction_error(m, port, t, Failure::Length);
            idle(m, port);
            return true;
        }
        let frame = match serial_protocol::decode(*mode, &t.rx) {
            Ok(v) => v,
            Err(e) => {
                let malformed_ascii = *mode == WireMode::Ascii
                    && (!t.rx.starts_with(b":")
                        || !t.rx.ends_with(b"\r\n")
                        || t.rx[1..t.rx.len().saturating_sub(2)]
                            .iter()
                            .any(|v| !v.is_ascii_hexdigit()));
                transaction_error(
                    m,
                    port,
                    t,
                    if malformed_ascii {
                        Failure::Data
                    } else {
                        Failure::Checksum
                    },
                );
                self.notices
                    .insert(format!("Seri yanıt doğrulaması başarısız: {e}"));
                idle(m, port);
                return true;
            }
        };
        if frame.station != request.station {
            self.notices.insert("Modbus: beklenmeyen istasyon çerçevesi atıldı; özgün yanıt zaman aşımı işlemeye devam ediyor".into());
            return false;
        }
        let reply = match serial_protocol::validate_response(request, &frame) {
            Ok(v) => v,
            Err(e) => {
                let length_error = frame.station == request.station
                    && frame.function == request.function
                    && matches!(request.function, 1..=4 | 23)
                    && request.data.len() >= 4
                    && {
                        let count = u16::from_be_bytes([request.data[2], request.data[3]]) as usize;
                        let expected = if matches!(request.function, 1 | 2) {
                            count.div_ceil(8)
                        } else {
                            count * 2
                        };
                        frame.data.len() != expected + 1
                            || frame.data.first().copied() != Some(expected as u8)
                    };
                transaction_error(
                    m,
                    port,
                    t,
                    if length_error {
                        Failure::Length
                    } else {
                        Failure::Data
                    },
                );
                self.notices
                    .insert(format!("Seri fonksiyon/veri yanıtı uyuşmuyor: {e}"));
                idle(m, port);
                return true;
            }
        };
        if let Reply::Exception(code) = reply {
            transaction_error(m, port, t, Failure::Exception(code));
            idle(m, port);
            return true;
        }
        let words = match &reply {
            Reply::Registers(v) => v.clone(),
            Reply::Bits(v) => v
                .chunks(16)
                .map(|bits| {
                    bits.iter()
                        .enumerate()
                        .fold(0u16, |word, (i, b)| word | (u16::from(*b) << i))
                })
                .collect(),
            _ => Vec::new(),
        };
        let body = body_bytes(*mode, &t.rx);
        if let Some(base) = raw_dest {
            let n = if *raw_packed {
                body.len().div_ceil(2)
            } else {
                body.len()
            };
            if let Err(e) = span(m, *base, n) {
                limit(m, e);
                idle(m, port);
                return true;
            }
        }
        if let Some(base) = converted
            && let Err(e) = span(m, *base, words.len())
        {
            limit(m, e);
            idle(m, port);
            return true;
        }
        if let Some(base) = raw_dest {
            write_bytes(m, *base, &body, !*raw_packed, usize::MAX);
            if port == SerialPort::Com2 && body.len() > *raw_window {
                self.notices.insert("Tam Modbus geri-bildirim çerçevesi ardışık D spanına yazıldı; PDF kısa debug-window tablosu tüm maksimum paketleri açıklamaz".into());
            }
        }
        if let Some(base) = converted {
            if port == SerialPort::Com2 && !t.servo && *mode == WireMode::Ascii {
                m.set_system(1131, true);
            }
            for (i, &word) in words.iter().enumerate() {
                m.set_word(d(base.index + i as u16), word as i16);
            }
            if port == SerialPort::Com2 && !t.servo {
                m.set_system(1131, false);
            }
        }
        if port == SerialPort::Com2 && !t.drive {
            m.set_system(1140, false);
            m.set_system(1141, false);
        }
        self.complete(m, port, t);
        true
    }
    fn complete(&self, m: &mut Memory, port: SerialPort, t: &Transaction) {
        let f = flags(port);
        m.set_system(f.request, false);
        idle(m, port);
        let done = if port == SerialPort::Com2 && matches!(t.plan, ReceivePlan::Modbus { .. }) {
            1127
        } else {
            f.done
        };
        m.set_system(done, true);
    }
}
#[derive(Debug, Clone, Copy)]
enum Failure {
    Timeout,
    Checksum,
    Data,
    Length,
    Exception(u8),
}

fn transaction_error(m: &mut Memory, port: SerialPort, t: &Transaction, failure: Failure) {
    if !t.drive {
        error(m, port, failure);
        return;
    }
    if port != SerialPort::Com2 {
        error(m, port, failure);
        return;
    }
    // Drive chapters name D1130/M1129, but do not extend MOD* M1140/M1141.
    let code = match failure {
        Failure::Timeout => {
            m.set_system(1129, true);
            1
        }
        Failure::Checksum => 2,
        Failure::Exception(v) => i16::from(v),
        Failure::Data => 4,
        Failure::Length => 5,
    };
    m.set_word(d(1130), code);
}

fn error(m: &mut Memory, port: SerialPort, failure: Failure) {
    let f = flags(port);
    let code = match failure {
        Failure::Timeout => 1,
        Failure::Checksum => 2,
        Failure::Exception(_) => 3,
        Failure::Data => 4,
        Failure::Length => 5,
    };
    if port == SerialPort::Com2 {
        match failure {
            Failure::Timeout => {
                m.set_system(1129, true);
                m.set_word(d(1130), 1);
            }
            Failure::Exception(v) => {
                m.set_system(1141, true);
                m.set_word(d(1130), i16::from(v));
            }
            _ => {
                m.set_system(1140, true);
            }
        }
    } else {
        m.set_system(f.error, true);
        m.set_word(d(f.error_code), code);
    }
}
fn idle(m: &mut Memory, port: SerialPort) {
    m.set_system(flags(port).ready, false);
    if port == SerialPort::Com2 {
        m.set_system(1121, true);
        m.set_system(1128, false);
    }
}
fn span(m: &Memory, base: Device, words: usize) -> Result<(), String> {
    if base.area != Area::D
        || base.index > m.data_register_limit.unwrap_or(9999)
        || words > 1024
        || words > 0
            && usize::from(base.index) + words - 1
                > usize::from(m.data_register_limit.unwrap_or(9999))
    {
        Err("Seri tamponun tam D aralığı model belleğini aşıyor".into())
    } else {
        Ok(())
    }
}
fn scalar(m: &Memory, v: &CalendarValue) -> Result<i32, String> {
    let mut set = BTreeSet::new();
    v.devices(&mut set);
    for device in set {
        span(m, device, 1)?;
    }
    Ok(v.read(m))
}
fn read_bytes(m: &Memory, base: Device, count: usize, low: bool) -> Vec<u8> {
    (0..count)
        .map(|i| {
            let v = m.word(d(base.index + if low { i } else { i / 2 } as u16)) as u16;
            if low || i % 2 == 0 {
                v as u8
            } else {
                (v >> 8) as u8
            }
        })
        .collect()
}
fn write_bytes(m: &mut Memory, base: Device, bytes: &[u8], low: bool, word_cap: usize) {
    for (i, &byte) in bytes.iter().enumerate() {
        let offset = if low { i } else { i / 2 };
        if offset >= word_cap {
            break;
        }
        let target = d(base.index + offset as u16);
        let old = m.word(target) as u16;
        let value = if low || i % 2 == 0 {
            (old & 0xff00) | u16::from(byte)
        } else {
            (old & 0x00ff) | (u16::from(byte) << 8)
        };
        m.set_word(target, value as i16);
    }
}
fn push_word(out: &mut Vec<u8>, word: u16) {
    out.extend(word.to_be_bytes());
}
fn body_bytes(mode: WireMode, bytes: &[u8]) -> Vec<u8> {
    if mode == WireMode::Ascii && bytes.len() >= 3 {
        bytes[1..bytes.len() - 2].to_vec()
    } else {
        bytes.to_vec()
    }
}
fn expected_modbus_len(bytes: &[u8], mode: WireMode) -> Option<usize> {
    if mode == WireMode::Ascii {
        return None;
    }
    if bytes.len() < 2 {
        return None;
    }
    if bytes[1] & 0x80 != 0 {
        return Some(5);
    }
    match bytes[1] {
        1..=4 | 23 => bytes.get(2).map(|&v| 5 + usize::from(v)),
        5 | 6 | 15 | 16 => Some(8),
        _ => None,
    }
}
fn character_us(port: SerialPort, protocol: u16) -> Result<u64, String> {
    let baud = match (protocol >> 4) & 15 {
        4 => 600,
        5 => 1200,
        6 => 2400,
        7 => 4800,
        8 => 9600,
        9 => 19200,
        10 => 38400,
        11 => 57600,
        12 => 115200,
        13 if port != SerialPort::Com1 => 500000,
        14 if port != SerialPort::Com1 => 31250,
        15 if port != SerialPort::Com1 => 921000,
        _ => return Err("Seri protokol baud alanı yapılandırılmadı/geçersiz".into()),
    };
    let parity = (protocol >> 1) & 3;
    if parity == 2 {
        return Err("Seri parity alanı ayrılmış".into());
    }
    let bits = 1
        + if protocol & 1 != 0 { 8 } else { 7 }
        + u64::from(parity != 0)
        + if protocol & 8 != 0 { 2 } else { 1 };
    Ok((bits * 1_000_000u64).div_ceil(baud))
}
fn rtu_frame_gap_us(protocol: u16) -> u64 {
    // The literal RTU timing table (PDF412) rather than an invented 4-char law.
    match (protocol >> 4) & 15 {
        4 => 21_000,
        5 => 10_000,
        6 => 5_000,
        7 => 3_000,
        8 => 2_000,
        _ => 1_000,
    }
}
fn rtu_timing(protocol: u16, policy: RtuTimingPolicy) -> (u64, u64) {
    if policy == RtuTimingPolicy::DeltaPrintedGapCandidate {
        // The Delta table does not specify an inter-character discard timer.
        return (u64::MAX, rtu_frame_gap_us(protocol));
    }
    let baud = match (protocol >> 4) & 15 {
        4 => 600u64,
        5 => 1200,
        6 => 2400,
        7 => 4800,
        8 => 9600,
        9 => 19200,
        10 => 38400,
        11 => 57600,
        12 => 115200,
        13 => 500000,
        14 => 31250,
        _ => 921000,
    };
    if baud > 19200 {
        (750, 1750)
    } else {
        // RTU standard characters have11bits (8data+start+parity/2stops).
        // >t1.5: floor permits1718us, rejects1719us at9600.
        // >=t3.5: ceil means EOF cannot be recognized before4011us.
        (16_500_000 / baud, 38_500_000u64.div_ceil(baud))
    }
}
fn framing(m: &Memory, protocol: u16) -> (Option<u8>, Vec<u8>) {
    let chars = if m.system_bit(1130) {
        if m.system_bit(1126) {
            [0x3a, 0x0d, 0x0a]
        } else {
            [2, 3, 0]
        }
    } else {
        [
            m.word(d(1124)) as u8,
            m.word(d(1125)) as u8,
            m.word(d(1126)) as u8,
        ]
    };
    let stx = (protocol & 0x100 != 0).then_some(chars[0]);
    let mut end = Vec::new();
    if protocol & 0x200 != 0 {
        end.push(chars[1]);
    }
    if protocol & 0x400 != 0 && !(m.system_bit(1130) && !m.system_bit(1126)) {
        end.push(chars[2]);
    }
    (stx, end)
}
#[cfg(test)]
#[path = "serial_io_tests.rs"]
mod tests;
