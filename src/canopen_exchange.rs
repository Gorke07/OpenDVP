//! Explicit SDO and raw CAN transactions sharing CanopenNetwork's owner.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::canopen::CanCpu;
use crate::plc::{Area, Device, Memory, Operand};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn bit(index: u16) -> Device {
    Device {
        area: Area::M,
        index,
    }
}
fn span(m: &Memory, base: Device, n: usize) -> Result<(), String> {
    if base.area != Area::D
        || n > 1200
        || usize::from(base.index) + n > usize::from(m.data_register_limit.unwrap_or(9999)) + 1
    {
        Err("CAN iletişim tamponunun tam D aralığı model sınırını aşıyor".into())
    } else {
        Ok(())
    }
}
fn scalar(m: &Memory, v: &CalendarValue) -> Result<i32, String> {
    let mut ds = BTreeSet::new();
    v.devices(&mut ds);
    for dev in ds {
        span(m, dev, 1)?;
    }
    Ok(v.read(m))
}
fn output(m: &Memory, device: Device) -> Result<(), String> {
    if device.area != Area::M
        || !device.writable_bit()
        || device.index > m.relay_limit.unwrap_or(4095)
        || device.to_string().parse::<Device>().is_err()
    {
        Err("CAN completion hedefi geçerli yazılabilir model M adresi olmalı".into())
    } else {
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum ExchangeOperand {
    Value(CalendarValue),
    Device(Device),
}
#[derive(Debug, Clone, PartialEq)]
pub enum ExchangeAction {
    Coprw {
        node: u16,
        request: CalendarValue,
        index: CalendarValue,
        subindex: CalendarValue,
        data: Device,
        done: Device,
    },
    Copwl {
        node: u16,
        table: Device,
        count: CalendarValue,
        done: Device,
        wide: bool,
    },
    Canrs {
        port: CalendarValue,
        control: Device,
        source: Device,
        destination: Device,
        done: Device,
    },
}
impl ExchangeAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let n = match name {
            "COPRW" => 6,
            "COPWL" | "DCOPWL" => 4,
            "CANRS" => 5,
            _ => return Err("COPRW/COPWL/DCOPWL/CANRS gerekli; P biçimi yok".into()),
        };
        if args.len() != n {
            return Err(format!("{name} {n} operand gerektirir"));
        }
        let value = |i: usize| -> Result<CalendarValue, String> {
            if !args[i].starts_with(['K', 'H', 'D']) {
                return Err("CAN sayısal operand K/H/D olmalı".into());
            }
            let v = CalendarValue::parse(&args[i], false)?;
            if !matches!(
                &v,
                CalendarValue::Additional(AdditionalValue::Word(
                    Operand::Constant(_) | Operand::Register(Device { area: Area::D, .. })
                ))
            ) {
                return Err("CAN skaler K/H/D gerektirir".into());
            }
            Ok(v)
        };
        let node = || -> Result<u16, String> {
            if !args[0].starts_with(['K', 'H']) {
                return Err("COPRW/COPWL node yalnız K/H sabit olmalı".into());
            }
            match Operand::parse(&args[0])? {
                Operand::Constant(v) => Ok(v as u16),
                _ => Err("Node sabit gerekli".into()),
            }
        };
        let device = |i: usize, area: Area| -> Result<Device, String> {
            let dev: Device = args[i].parse()?;
            if dev.area != area {
                return Err(format!("Operand {} çıplak {area:?} olmalı", i + 1));
            }
            Ok(dev)
        };
        Ok(match name {
            "COPRW" => Self::Coprw {
                node: node()?,
                request: value(1)?,
                index: value(2)?,
                subindex: value(3)?,
                data: device(4, Area::D)?,
                done: device(5, Area::M)?,
            },
            "COPWL" | "DCOPWL" => Self::Copwl {
                node: node()?,
                table: device(1, Area::D)?,
                count: value(2)?,
                done: device(3, Area::M)?,
                wide: name == "DCOPWL",
            },
            _ => Self::Canrs {
                port: value(0)?,
                control: device(1, Area::D)?,
                source: device(2, Area::D)?,
                destination: device(3, Area::D)?,
                done: device(4, Area::M)?,
            },
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Coprw { .. } => "COPRW",
            Self::Copwl { wide: true, .. } => "DCOPWL",
            Self::Copwl { .. } => "COPWL",
            Self::Canrs { .. } => "CANRS",
        }
    }
    pub fn header(&self) -> Option<u16> {
        match self {
            Self::Coprw { .. } => Some(0xc802),
            Self::Canrs { .. } => Some(0xc602),
            Self::Copwl { wide: true, .. } => Some(0xbd02),
            Self::Copwl { .. } => Some(0xbc02),
        }
    }
    pub fn words(&self) -> u16 {
        match self {
            Self::Coprw { .. } => 13,
            Self::Canrs { .. } => 11,
            Self::Copwl { wide: true, .. } => 17,
            Self::Copwl { .. } => 9,
        }
    }
    pub fn wide(&self) -> bool {
        matches!(self, Self::Copwl { wide: true, .. })
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Coprw { done, .. } | Self::Copwl { done, .. } | Self::Canrs { done, .. } => *done,
        }
    }
    pub fn operands(&self) -> Vec<ExchangeOperand> {
        let v = |x: &CalendarValue| ExchangeOperand::Value(x.clone());
        let dev = ExchangeOperand::Device;
        let node = |n: u16| {
            ExchangeOperand::Value(CalendarValue::parse(&format!("K{}", n as i16), false).unwrap())
        };
        match self {
            Self::Coprw {
                node: n,
                request,
                index,
                subindex,
                data,
                done,
            } => vec![
                node(*n),
                v(request),
                v(index),
                v(subindex),
                dev(*data),
                dev(*done),
            ],
            Self::Copwl {
                node: n,
                table,
                count,
                done,
                ..
            } => vec![node(*n), dev(*table), v(count), dev(*done)],
            Self::Canrs {
                port,
                control,
                source,
                destination,
                done,
            } => vec![
                v(port),
                dev(*control),
                dev(*source),
                dev(*destination),
                dev(*done),
            ],
        }
    }
    pub fn label(&self) -> String {
        format!(
            "{} {}",
            self.name(),
            self.operands()
                .iter()
                .map(|o| match o {
                    ExchangeOperand::Value(v) => v.to_string(),
                    ExchangeOperand::Device(d) => d.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
    pub fn display_value(&self, m: &Memory) -> String {
        format!("{}={}", self.dest(), m.bit(self.dest()))
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for operand in self.operands() {
            match operand {
                ExchangeOperand::Value(v) => v.devices(out),
                ExchangeOperand::Device(d) => {
                    out.insert(d);
                }
            }
        }
        out.extend([1614, 1615, 1616, 1620, 1621, 1622, 1623].map(bit));
        out.extend([1175, 1177, 6000, 6001, 6002].map(d));
        Memory::error_devices(out);
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        let mut add = |base: Device, n: usize| {
            if span(m, base, n).is_ok() {
                out.extend((0..n).map(|i| d(base.index + i as u16)));
            }
        };
        match self {
            Self::Coprw { data, .. } => add(*data, 2),
            Self::Copwl {
                table, count, wide, ..
            } => {
                if let Ok(n) = scalar(m, count)
                    && (1..=100).contains(&n)
                {
                    add(*table, n as usize * if *wide { 4 } else { 3 });
                }
            }
            Self::Canrs {
                control,
                source,
                destination,
                ..
            } => {
                let words = if m
                    .can_exchange
                    .latched_standard
                    .unwrap_or(m.system_bit(1620))
                {
                    2
                } else {
                    3
                };
                add(*control, words);
                add(*source, 8);
                add(*destination, if words == 2 { 10 } else { 11 });
            }
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let now = m.timing_io.now_us;
        if now != m.can_exchange.now_us {
            CanExchangeState::advance(m, now);
        }
        let mut s = std::mem::take(&mut m.can_exchange);
        s.visit(m, self, power, pc);
        m.can_exchange = s;
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SdoPacking {
    #[default]
    LowWordLittleEndianCandidate,
    HighWordBigEndianCandidate,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Broadcast29Layout {
    #[default]
    Unresolved,
    Canonical11Candidate,
    PrintedGap12Candidate,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BroadcastDeadline {
    #[default]
    IdleSinceLastPacketCandidate,
    AbsoluteFromReceiveStart,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BroadcastExpiry {
    #[default]
    CompletionAndError,
    CompletionOnlyCandidate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawCanCpu {
    Es2C,
    Se2,
    Sa2,
    Sx2,
    SeOldMcu,
    SeNewMcu,
}
#[derive(Debug, Clone, Copy)]
pub struct RawCanPort {
    pub cpu: RawCanCpu,
    pub firmware_major: u16,
    pub firmware_minor: u8,
    pub module_firmware: Option<(u16, u8)>,
    pub module_master_without_slaves: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanFrame {
    pub id: u32,
    pub standard: bool,
    pub data: Vec<u8>,
}
impl CanFrame {
    fn validate(&self) -> Result<(), String> {
        if self.id > if self.standard { 0x7ff } else { 0x1fffffff } || self.data.len() > 8 {
            Err("CAN frame ID genişliği/0..8byte geçersiz".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdoRequest {
    pub id: u64,
    pub epoch: u64,
    pub pc: u16,
    pub node: u8,
    pub index: u16,
    pub subindex: u8,
    pub write: Option<Vec<u8>>,
    pub at_us: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCanRequest {
    pub id: u64,
    pub epoch: u64,
    pub pc: u16,
    pub port: u16,
    pub frame: CanFrame,
    pub at_us: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdoResult {
    WriteAck,
    Data(Vec<u8>),
    Abort {
        sdo_code: u32,
        plc_code: Option<u16>,
    },
}
#[derive(Debug, Clone)]
enum Event {
    Sdo {
        request: SdoRequest,
        result: SdoResult,
    },
    Frame {
        port: u16,
        frame: CanFrame,
    },
    Tx {
        id: u64,
        epoch: u64,
    },
}
#[derive(Debug, Clone)]
struct Row {
    index: u16,
    subindex: u8,
    write: Option<Vec<u8>>,
}
#[derive(Debug, Clone)]
struct SdoJob {
    pc: u16,
    node: u8,
    done: Device,
    rows: VecDeque<Row>,
    pending: Option<SdoRequest>,
    destination: Option<Device>,
    list: bool,
    ready: bool,
    complete: bool,
    deadline: Option<u64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RawPhase {
    Transmit,
    Receive,
    Complete,
}
#[derive(Debug, Clone)]
struct RawJob {
    pc: u16,
    port: u16,
    done: Device,
    destination: Device,
    standard: bool,
    filter: u32,
    slave: bool,
    broadcast: bool,
    send: Option<CanFrame>,
    phase: RawPhase,
    tx: Option<RawCanRequest>,
    deadline: Option<u64>,
    timeout_us: Option<u64>,
    count: usize,
    complete: bool,
}
#[derive(Debug, Clone)]
enum Job {
    Sdo(SdoJob),
    Raw(RawJob),
}
impl Job {
    fn pc(&self) -> u16 {
        match self {
            Self::Sdo(j) => j.pc,
            Self::Raw(j) => j.pc,
        }
    }
    fn done(&self) -> Device {
        match self {
            Self::Sdo(j) => j.done,
            Self::Raw(j) => j.done,
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct CanExchangeState {
    pub now_us: u64,
    pub notices: BTreeSet<String>,
    pub packing: SdoPacking,
    pub broadcast29: Broadcast29Layout,
    pub broadcast_deadline: BroadcastDeadline,
    pub broadcast_expiry: BroadcastExpiry,
    pub sdo_timeout_us: Option<u64>,
    pub last_sdo_abort: Option<u32>,
    raw_ports: BTreeMap<u16, RawCanPort>,
    active: Option<Job>,
    completed: BTreeSet<u16>,
    failed: BTreeSet<u16>,
    sequence: u64,
    epoch: u64,
    latched_standard: Option<bool>,
    events: BTreeMap<(u64, u64), Event>,
    sdo_outgoing: VecDeque<SdoRequest>,
    raw_outgoing: VecDeque<RawCanRequest>,
}
impl CanExchangeState {
    pub fn configure_raw_port(&mut self, port: u16, p: RawCanPort) -> Result<(), String> {
        let version = (p.firmware_major, p.firmware_minor);
        let valid = match p.cpu {
            RawCanCpu::Es2C => version >= (3, 60),
            RawCanCpu::Se2 => version >= (1, 0),
            RawCanCpu::Sa2 | RawCanCpu::Sx2 => version >= (3, 2),
            RawCanCpu::SeOldMcu => ((1, 88)..=(1, 95)).contains(&version),
            RawCanCpu::SeNewMcu => version >= (2, 14),
        };
        if self.active.is_some()
            || !valid
            || p.firmware_minor > 99
            || !(port == 0 && matches!(p.cpu, RawCanCpu::Es2C | RawCanCpu::Se2)
                || (100..=107).contains(&port)
                    && p.module_firmware.is_some_and(|v| v >= (1, 40))
                    && p.module_master_without_slaves)
        {
            return Err(
                "CANRS port/CPU/firmware veya COPM-SL>=1.40 açık yapılandırması geçersiz".into(),
            );
        }
        self.raw_ports.insert(port, p);
        Ok(())
    }
    pub fn outgoing_sdo(&self) -> &VecDeque<SdoRequest> {
        &self.sdo_outgoing
    }
    pub fn outgoing_frames(&self) -> &VecDeque<RawCanRequest> {
        &self.raw_outgoing
    }
    pub fn pending_sdo(&self) -> Option<&SdoRequest> {
        if let Some(Job::Sdo(j)) = &self.active {
            j.pending.as_ref()
        } else {
            None
        }
    }
    pub fn begin_scan(_m: &mut Memory) {}
    pub fn end_scan(_m: &mut Memory) {}
    pub fn reset_instruction_state(&mut self) {
        self.active = None;
        self.completed.clear();
        self.failed.clear();
        self.events.clear();
        self.sdo_outgoing.clear();
        self.raw_outgoing.clear();
        self.latched_standard = None;
        self.now_us = 0;
        self.epoch = self.epoch.wrapping_add(1);
    }
    pub fn abort(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.can_exchange);
        if let Some(j) = &s.active {
            m.canopen.release_exchange(j.pc());
        }
        let now = s.now_us;
        s.reset_instruction_state();
        s.now_us = now;
        m.can_exchange = s;
    }
    pub fn restart_cpu_clock(&mut self, elapsed: u64) -> Result<(), String> {
        if elapsed < self.now_us {
            return Err("CAN exchange reset zamanı geriye gidemez".into());
        }
        self.reset_instruction_state();
        Ok(())
    }
    fn event(&mut self, at: u64, e: Event) -> Result<(), String> {
        if at < self.now_us || self.events.len() >= 4096 {
            return Err("CAN exchange gelecek zaman/4096olay sınırı gerekli".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("CAN exchange sıra taşması")?;
        self.events.insert((at, self.sequence), e);
        Ok(())
    }
    pub fn queue_sdo_reply(
        &mut self,
        at: u64,
        request: SdoRequest,
        result: SdoResult,
    ) -> Result<(), String> {
        if matches!(&result,SdoResult::Data(v) if v.len()>1024) {
            return Err("SDO reply tamponu1024byte sınırında".into());
        }
        self.event(at, Event::Sdo { request, result })
    }
    pub fn queue_frame(&mut self, at: u64, port: u16, frame: CanFrame) -> Result<(), String> {
        frame.validate()?;
        if !self.raw_ports.contains_key(&port) {
            return Err("RX CAN portu yapılandırılmamış".into());
        }
        self.event(at, Event::Frame { port, frame })
    }
    pub fn queue_transmitted(&mut self, at: u64, id: u64, epoch: u64) -> Result<(), String> {
        self.event(at, Event::Tx { id, epoch })
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        let deadline = match &self.active {
            Some(Job::Sdo(j)) => j.deadline,
            Some(Job::Raw(j)) => j.deadline,
            None => None,
        };
        self.events
            .first_key_value()
            .map(|(k, _)| k.0)
            .into_iter()
            .chain(deadline)
            .min()
    }
    fn fail(&mut self, m: &mut Memory, pc: u16, node: u8, code: Option<u16>, reason: &str) {
        m.set_system(1616, true);
        m.set_word(d(6000), i16::from(node));
        m.set_word(d(6002), pc as i16);
        if let Some(c) = code {
            m.set_word(d(6001), c as i16);
        } else {
            self.notices
                .insert("CAN hata kodu eşlemesi bilinmiyor; D6001 korunuyor".into());
        }
        self.notices.insert(reason.into());
        m.canopen.release_exchange(pc);
        self.failed.insert(pc);
    }
    fn next_id(&mut self) -> Result<u64, String> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("CAN exchange sıra taşması")?;
        Ok(self.sequence)
    }
    fn emit_sdo(&mut self, j: &mut SdoJob) -> Result<(), String> {
        let Some(row) = j.rows.pop_front() else {
            j.complete = true;
            return Ok(());
        };
        let request = SdoRequest {
            id: self.next_id()?,
            epoch: self.epoch,
            pc: j.pc,
            node: j.node,
            index: row.index,
            subindex: row.subindex,
            write: row.write,
            at_us: self.now_us,
        };
        if self.sdo_outgoing.len() >= 4096 {
            self.sdo_outgoing.pop_front();
        }
        self.sdo_outgoing.push_back(request.clone());
        j.pending = Some(request);
        j.ready = false;
        j.deadline = self
            .sdo_timeout_us
            .map(|v| self.now_us.checked_add(v).ok_or("SDO deadline taşması"))
            .transpose()?;
        Ok(())
    }
    fn emit_raw(&mut self, j: &mut RawJob) -> Result<(), String> {
        let Some(frame) = j.send.clone() else {
            j.phase = RawPhase::Receive;
            return Ok(());
        };
        let request = RawCanRequest {
            id: self.next_id()?,
            epoch: self.epoch,
            pc: j.pc,
            port: j.port,
            frame,
            at_us: self.now_us,
        };
        if self.raw_outgoing.len() >= 4096 {
            self.raw_outgoing.pop_front();
        }
        self.raw_outgoing.push_back(request.clone());
        j.tx = Some(request);
        j.phase = RawPhase::Transmit;
        j.deadline = None;
        Ok(())
    }
    fn bytes(&self, m: &Memory, base: Device, n: usize) -> Vec<u8> {
        let words = (0..n.div_ceil(2))
            .map(|i| m.word(d(base.index + i as u16)) as u16)
            .collect::<Vec<_>>();
        let mut b = match self.packing {
            SdoPacking::LowWordLittleEndianCandidate => words
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
            SdoPacking::HighWordBigEndianCandidate => words
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect::<Vec<_>>(),
        };
        b.truncate(n);
        b
    }
    fn visit(&mut self, m: &mut Memory, a: &ExchangeAction, power: bool, pc: u16) {
        if !power {
            self.completed.remove(&pc);
            self.failed.remove(&pc);
            if self.active.as_ref().is_some_and(|j| j.pc() == pc) {
                let job = self.active.take().unwrap();
                m.set_internal(job.done(), false);
                m.canopen.release_exchange(pc);
            } else if matches!(a, ExchangeAction::Copwl { .. }) {
                m.set_internal(a.dest(), false);
            }
            return;
        }
        if let Some(mut job) = self.active.take() {
            if job.pc() != pc {
                self.active = Some(job);
                return;
            }
            match &mut job {
                Job::Sdo(j) => {
                    if j.complete {
                        m.set_internal(j.done, true);
                        if !j.list {
                            m.canopen.release_exchange(pc);
                            self.completed.insert(pc);
                            return;
                        }
                    } else if j.ready
                        && let Err(e) = self.emit_sdo(j)
                    {
                        m.execution_limitations.insert(e);
                        m.canopen.release_exchange(pc);
                        return;
                    }
                }
                Job::Raw(j) => {
                    if j.complete {
                        m.set_internal(j.done, true);
                        m.canopen.release_exchange(pc);
                        self.completed.insert(pc);
                        return;
                    }
                }
            }
            self.active = Some(job);
            return;
        }
        if self.failed.contains(&pc) {
            return;
        }
        if self.completed.contains(&pc) {
            if !m.bit(a.dest()) {
                self.completed.remove(&pc);
            } else {
                return;
            }
        }
        let result = self.prepare(m, a, pc);
        let mut job = match result {
            Ok(j) => j,
            Err(e) => {
                m.execution_limitations
                    .insert(format!("{} STEP{pc}: {e}", a.name()));
                return;
            }
        };
        let port = if let Job::Raw(j) = &job { j.port } else { 0 };
        if !m.canopen.reserve_exchange(port, pc) {
            return;
        }
        m.set_internal(a.dest(), false);
        let sent = match &mut job {
            Job::Sdo(j) => self.emit_sdo(j),
            Job::Raw(j) => {
                m.set_system(1623, false);
                if j.broadcast {
                    m.set_word(d(1175), 0);
                }
                self.latched_standard = Some(j.standard);
                if j.slave {
                    j.deadline = j.timeout_us.and_then(|v| self.now_us.checked_add(v));
                    Ok(())
                } else {
                    self.emit_raw(j)
                }
            }
        };
        if let Err(e) = sent {
            m.execution_limitations.insert(e);
            m.canopen.release_exchange(pc);
            return;
        }
        self.active = Some(job);
    }
    fn prepare(&mut self, m: &mut Memory, a: &ExchangeAction, pc: u16) -> Result<Job, String> {
        output(m, a.dest())?;
        match a {
            ExchangeAction::Coprw { node, done, .. } | ExchangeAction::Copwl { node, done, .. } => {
                let profile = m
                    .canopen
                    .profile()
                    .ok_or("SDO açık CAN CPU profili gerektirir")?;
                let modern = profile.cpu == CanCpu::Se2
                    || (profile.firmware_major, profile.firmware_minor) >= (3, 60);
                let list = matches!(a, ExchangeAction::Copwl { .. });
                if list && !modern {
                    return Err("COPWL ES2-C>=3.60/SE2 gerektirir".into());
                }
                let signed = *node as i16;
                let node = if modern {
                    signed.clamp(1, 127) as u8
                } else {
                    if !(1..=8).contains(&signed) {
                        m.operation_error(0xe1a, pc.wrapping_sub(m.native_instruction_pc()));
                        return Err("Eski COPRW node1..8 gerektirir".into());
                    }
                    signed as u8
                };
                let peer = m
                    .canopen
                    .node(node)
                    .ok_or("SDO için açık node peer gerekli")?;
                if !peer.online {
                    return Err("SDO node çevrimdışı".into());
                }
                if m.system_bit(1614) && (!m.system_bit(1615) || !peer.initialized) {
                    return Err("Delta SDO için gerçek INITC tamamlanmalı".into());
                }
                span(m, d(6000), 3)?;
                let mut rows = VecDeque::new();
                let mut destination = None;
                if let ExchangeAction::Copwl {
                    table, count, wide, ..
                } = a
                {
                    let n = scalar(m, count)?;
                    if !(1..=100).contains(&n) {
                        return Err(
                            "COPWL count1..100; dış değerin clamp/hatası tanımlanmadı".into()
                        );
                    }
                    let stride = if *wide { 4 } else { 3 };
                    span(m, *table, n as usize * stride)?;
                    for i in 0..n as u16 {
                        let base = table.index + i * stride as u16;
                        let sub = m.word(d(base + 1)) as u16;
                        if sub > 255 {
                            return Err("SDO subindex8bit sınırı aşıyor".into());
                        }
                        rows.push_back(Row {
                            index: m.word(d(base)) as u16,
                            subindex: sub as u8,
                            write: Some(self.bytes(m, d(base + 2), if *wide { 4 } else { 2 })),
                        });
                    }
                } else if let ExchangeAction::Coprw {
                    request,
                    index,
                    subindex,
                    data,
                    ..
                } = a
                {
                    let code = scalar(m, request)? as u16;
                    let index = scalar(m, index)? as u16;
                    let sub = scalar(m, subindex)?;
                    if !(0..=255).contains(&sub) {
                        return Err("SDO subindex0..255 olmalı".into());
                    }
                    let n: usize = match code {
                        0x23 => 4,
                        0x2b => 2,
                        0x2f => 1,
                        0x40 => 0,
                        _ => return Err("COPRW request23/2B/2F/40 olmalı".into()),
                    };
                    span(m, *data, if n == 0 { 1 } else { n.div_ceil(2) })?;
                    if n == 0 {
                        destination = Some(*data);
                    }
                    rows.push_back(Row {
                        index,
                        subindex: sub as u8,
                        write: if n == 0 {
                            None
                        } else {
                            Some(self.bytes(m, *data, n))
                        },
                    });
                }
                self.notices.insert("SDO açık typed request/reply; packing aday profili, en fazla4byte expedited read; segmented SDO ve raw wire eşitliği açık".into());
                Ok(Job::Sdo(SdoJob {
                    pc,
                    node,
                    done: *done,
                    rows,
                    pending: None,
                    destination,
                    list,
                    ready: false,
                    complete: false,
                    deadline: None,
                }))
            }
            ExchangeAction::Canrs {
                port,
                control,
                source,
                destination,
                done,
            } => {
                let port = scalar(m, port)?;
                if !(0..=u16::MAX as i32).contains(&port)
                    || !self.raw_ports.contains_key(&(port as u16))
                {
                    return Err("CANRS açık built-in0 veya COPM-SL100..107 port gerektirir".into());
                }
                let standard = self.latched_standard.unwrap_or(m.system_bit(1620));
                let header = if standard { 2 } else { 3 };
                span(m, *control, header)?;
                span(m, *destination, header)?;
                let id = if standard {
                    u32::from(m.word(*control) as u16)
                } else {
                    u32::from(m.word(*control) as u16)
                        | (u32::from(m.word(d(control.index + 1)) as u16) << 16)
                };
                let length = m.word(d(control.index + header as u16 - 1)).clamp(0, 8) as usize;
                span(m, *source, length)?;
                let broadcast = m.system_bit(1622);
                if broadcast && !standard && self.broadcast29 == Broadcast29Layout::Unresolved {
                    return Err("CANRS29bit broadcast image gap/stride çelişkisi için açık layout seçilmeli".into());
                }
                let timeout = m.word(d(1177));
                if !(0..=3000).contains(&timeout) {
                    return Err("CANRS timeout0..3000ms; dış davranış tanımlanmıyor".into());
                }
                let slave = m.system_bit(1621) || length == 0;
                let effective = if !slave && broadcast && timeout == 0 {
                    200
                } else {
                    timeout
                };
                let filter = if standard {
                    u32::from(m.word(*destination) as u16)
                } else {
                    u32::from(m.word(*destination) as u16)
                        | (u32::from(m.word(d(destination.index + 1)) as u16) << 16)
                };
                let send = if length == 0 {
                    None
                } else {
                    let frame = CanFrame {
                        id,
                        standard,
                        data: (0..length)
                            .map(|i| m.word(d(source.index + i as u16)) as u8)
                            .collect(),
                    };
                    frame.validate()?;
                    Some(frame)
                };
                self.notices.insert("CANRS ilk powered visit protokol latch; length0 rolü broadcast bitini korur; completion sonraki ziyaret; broadcast deadline/layout açık yerel profiller".into());
                Ok(Job::Raw(RawJob {
                    pc,
                    port: port as u16,
                    done: *done,
                    destination: *destination,
                    standard,
                    filter,
                    slave,
                    broadcast,
                    send,
                    phase: if slave {
                        RawPhase::Receive
                    } else {
                        RawPhase::Transmit
                    },
                    tx: None,
                    deadline: None,
                    timeout_us: (effective > 0).then_some(effective as u64 * 1000),
                    count: 0,
                    complete: false,
                }))
            }
        }
    }
    pub fn advance(m: &mut Memory, at: u64) {
        let mut s = std::mem::take(&mut m.can_exchange);
        if at < s.now_us {
            m.execution_limitations
                .insert("CAN exchange saati geriye gidiyor".into());
            m.can_exchange = s;
            return;
        }
        let mut steps = 0;
        while let Some(next) = s.next_deadline_us() {
            if next > at {
                break;
            }
            s.now_us = next;
            steps += 1;
            if steps > 8192 {
                m.execution_limitations
                    .insert("CAN exchange olay sınırı aşıldı".into());
                break;
            }
            s.expire(m);
            let keys = s
                .events
                .range(..=(next, u64::MAX))
                .map(|(k, _)| *k)
                .collect::<Vec<_>>();
            for key in keys {
                let e = s.events.remove(&key).unwrap();
                s.receive(m, e);
            }
        }
        if steps <= 8192 {
            s.now_us = at;
        }
        m.can_exchange = s;
    }
    fn expire(&mut self, m: &mut Memory) {
        let Some(mut job) = self.active.take() else {
            return;
        };
        let due = match &job {
            Job::Sdo(j) => j.deadline.is_some_and(|v| v <= self.now_us),
            Job::Raw(j) => j.deadline.is_some_and(|v| v <= self.now_us),
        };
        if !due {
            self.active = Some(job);
            return;
        }
        match &mut job {
            Job::Sdo(j) => {
                self.fail(m, j.pc, j.node, None, "Açık SDO yerel timeout doldu");
            }
            Job::Raw(j) => {
                j.deadline = None;
                j.complete = true;
                j.phase = RawPhase::Complete;
                if !j.broadcast || self.broadcast_expiry == BroadcastExpiry::CompletionAndError {
                    m.set_system(1623, true);
                }
                self.active = Some(job);
            }
        }
    }
    fn receive(&mut self, m: &mut Memory, event: Event) {
        let Some(mut job) = self.active.take() else {
            self.notices
                .insert("Owner dışı CAN exchange olayı atıldı".into());
            return;
        };
        let mut retain = true;
        match (&mut job, event) {
            (Job::Sdo(j), Event::Sdo { request, result }) => {
                if !j.pending.as_ref().is_some_and(|p| p == &request) {
                    self.notices
                        .insert("Başka/eski SDO request yanıtı atıldı; deadline korunuyor".into());
                } else {
                    match result {
                        SdoResult::Abort { sdo_code, plc_code } => {
                            self.last_sdo_abort = Some(sdo_code);
                            self.fail(m, j.pc, j.node, plc_code, "SDO peer abort döndürdü");
                            retain = false;
                        }
                        SdoResult::Data(bytes) if request.write.is_none() => {
                            if bytes.is_empty() || bytes.len() > 4 {
                                m.execution_limitations.insert("SDO read1..4byte expedited sınırı; segmented reply henüz desteklenmiyor".into());
                                m.canopen.release_exchange(j.pc);
                                retain = false;
                            } else if let Some(base) = j.destination {
                                if let Err(e) = span(m, base, bytes.len().div_ceil(2)) {
                                    m.execution_limitations.insert(e);
                                    m.canopen.release_exchange(j.pc);
                                    retain = false;
                                } else {
                                    for (i, chunk) in bytes.chunks(2).enumerate() {
                                        let target = d(base.index + i as u16);
                                        let value = if chunk.len() == 1 {
                                            (m.word(target) as u16 & 0xff00) | u16::from(chunk[0])
                                        } else {
                                            match self.packing {
                                                SdoPacking::LowWordLittleEndianCandidate => {
                                                    u16::from_le_bytes([chunk[0], chunk[1]])
                                                }
                                                SdoPacking::HighWordBigEndianCandidate => {
                                                    u16::from_be_bytes([chunk[0], chunk[1]])
                                                }
                                            }
                                        };
                                        m.set_word(target, value as i16);
                                    }
                                    j.pending = None;
                                    j.deadline = None;
                                    j.complete = true;
                                }
                            }
                        }
                        SdoResult::WriteAck if request.write.is_some() => {
                            j.pending = None;
                            j.deadline = None;
                            if j.rows.is_empty() {
                                j.complete = true;
                            } else {
                                j.ready = true;
                            }
                        }
                        _ => {
                            self.fail(
                                m,
                                j.pc,
                                j.node,
                                None,
                                "SDO read/write reply türü request ile uyuşmuyor",
                            );
                            retain = false;
                        }
                    }
                }
            }
            (Job::Raw(j), Event::Tx { id, epoch }) => {
                if j.tx
                    .as_ref()
                    .is_some_and(|t| t.id == id && t.epoch == epoch)
                {
                    j.tx = None;
                    if j.slave {
                        j.complete = true;
                        j.phase = RawPhase::Complete;
                    } else {
                        j.phase = RawPhase::Receive;
                        j.deadline = j.timeout_us.and_then(|v| self.now_us.checked_add(v));
                    }
                }
            }
            (Job::Raw(j), Event::Frame { port, frame }) => {
                if j.phase == RawPhase::Receive
                    && port == j.port
                    && frame.standard == j.standard
                    && (j.broadcast || j.filter == 0 || frame.id == j.filter)
                {
                    if j.count >= 100 {
                        m.set_system(1623, true);
                        j.complete = true;
                        j.phase = RawPhase::Complete;
                        j.deadline = None;
                    } else {
                        let stride = if j.standard {
                            10
                        } else if j.broadcast
                            && self.broadcast29 == Broadcast29Layout::PrintedGap12Candidate
                        {
                            12
                        } else {
                            11
                        };
                        let offset = if j.broadcast { j.count * stride } else { 0 };
                        let required = if j.standard {
                            2 + frame.data.len()
                        } else if j.broadcast
                            && self.broadcast29 == Broadcast29Layout::PrintedGap12Candidate
                        {
                            4 + frame.data.len()
                        } else {
                            3 + frame.data.len()
                        };
                        if let Err(e) = span(m, j.destination, offset + required) {
                            m.execution_limitations.insert(e);
                            m.canopen.release_exchange(j.pc);
                            retain = false;
                        } else {
                            let base = j.destination.index + offset as u16;
                            m.set_word(d(base), frame.id as i16);
                            let length = if j.standard {
                                base + 1
                            } else {
                                m.set_word(d(base + 1), (frame.id >> 16) as i16);
                                base + 2
                            };
                            m.set_word(d(length), frame.data.len() as i16);
                            let data = if !j.standard
                                && j.broadcast
                                && self.broadcast29 == Broadcast29Layout::PrintedGap12Candidate
                            {
                                base + 4
                            } else {
                                length + 1
                            };
                            for (i, b) in frame.data.iter().enumerate() {
                                let target = d(data + i as u16);
                                m.set_word(
                                    target,
                                    ((m.word(target) as u16 & 0xff00) | u16::from(*b)) as i16,
                                );
                            }
                            j.count += 1;
                            if j.broadcast {
                                m.set_word(d(1175), j.count as i16);
                                if self.broadcast_deadline
                                    == BroadcastDeadline::IdleSinceLastPacketCandidate
                                {
                                    j.deadline =
                                        j.timeout_us.and_then(|v| self.now_us.checked_add(v));
                                }
                            } else if j.slave && j.send.is_some() {
                                if let Err(e) = self.emit_raw(j) {
                                    m.execution_limitations.insert(e);
                                    m.canopen.release_exchange(j.pc);
                                    retain = false;
                                }
                            } else {
                                j.complete = true;
                                j.phase = RawPhase::Complete;
                                j.deadline = None;
                            }
                        }
                    }
                }
            }
            _ => {
                self.notices
                    .insert("CAN exchange olay türü etkin işlemle eşleşmedi".into());
            }
        }
        if retain {
            self.active = Some(job);
        }
    }
}
#[cfg(test)]
#[path = "canopen_exchange_tests.rs"]
mod tests;
