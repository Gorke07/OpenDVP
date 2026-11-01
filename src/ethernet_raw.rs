//! ETHRS packet-level virtual socket engine (PDF533..543). No OS sockets.
//! TCP Packet events are explicit messages, never guessed TCP segment framing.
use crate::calendar_operand::CalendarValue;
use crate::file_memory::FirmwareVersion;
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn span(m: &Memory, base: Device, count: u16) -> bool {
    base.area == Area::D
        && count > 0
        && u32::from(base.index) + u32::from(count) - 1
            <= u32::from(m.data_register_limit.unwrap_or(9999))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Endpoint {
    pub ip: [u8; 4],
    pub port: u16,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketConfig {
    pub tcp: bool,
    pub local_port: u16,
    pub remote: Endpoint,
    pub server: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RawEthernetModel {
    #[default]
    Disabled,
    Se12,
    Se26,
    Se2,
    Es2E,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode4Revision {
    #[default]
    Conservative160,
    Detail146,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Se12Revision {
    Detail183,
    #[default]
    Conservative188,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RawActivation {
    #[default]
    EachVisitAfterComplete,
    RisingOnly,
}
#[derive(Debug, Clone, Copy)]
pub struct RawEthernetProfile {
    pub model: RawEthernetModel,
    pub firmware: FirmwareVersion,
    pub keep_alive_us: u64,
    pub es2e_mode4: Mode4Revision,
    pub se12_revision: Se12Revision,
    pub activation: RawActivation,
    pub udp_falling_done: bool,
}
impl Default for RawEthernetProfile {
    fn default() -> Self {
        Self {
            model: RawEthernetModel::Disabled,
            firmware: FirmwareVersion::new(1, 0),
            keep_alive_us: 30000,
            es2e_mode4: Mode4Revision::Conservative160,
            se12_revision: Se12Revision::Conservative188,
            activation: RawActivation::EachVisitAfterComplete,
            udp_falling_done: true,
        }
    }
}
impl RawEthernetProfile {
    fn validate(&self, mode: u16) -> Result<(), String> {
        let min = match self.model {
            RawEthernetModel::Disabled => {
                return Err("ETHRS açık CPU/Ethernet profili gerektirir".into());
            }
            RawEthernetModel::Es2E => FirmwareVersion::new(1, 8),
            RawEthernetModel::Se12 => FirmwareVersion::new(
                1,
                if self.se12_revision == Se12Revision::Conservative188 {
                    88
                } else {
                    83
                },
            ),
            _ => FirmwareVersion::new(1, 0),
        };
        if self.firmware.minor > 99 || self.firmware < min {
            return Err("ETHRS seçili CPU firmware sürümünde belgelenmedi".into());
        }
        if mode == 4 {
            let min = match self.model {
                RawEthernetModel::Es2E => FirmwareVersion::new(
                    1,
                    if self.es2e_mode4 == Mode4Revision::Conservative160 {
                        60
                    } else {
                        46
                    },
                ),
                RawEthernetModel::Se12 => FirmwareVersion::new(1, 95),
                _ => FirmwareVersion::new(1, 0),
            };
            if self.firmware < min {
                return Err("ETHRS mode4 seçili firmware/politikada mevcut değil".into());
            }
        }
        Ok(())
    }
    fn receive_only_client(&self) -> bool {
        match self.model {
            RawEthernetModel::Es2E => self.firmware >= FirmwareVersion::new(1, 20),
            RawEthernetModel::Se12 => self.firmware >= FirmwareVersion::new(1, 92),
            RawEthernetModel::Se26 | RawEthernetModel::Se2 => true,
            _ => false,
        }
    }
    fn sender_ip(&self) -> bool {
        self.receive_only_client()
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum RawEthernetOperand {
    Value(CalendarValue),
    Word(Device),
    Bit(Device),
}
#[derive(Debug, Clone, PartialEq)]
pub struct RawEthernetAction {
    pub connection: Device,
    pub mode: Device,
    pub source: Device,
    pub length: CalendarValue,
    pub destination: Device,
    pub done: Device,
}
impl RawEthernetAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if name != "ETHRS" || args.len() != 6 {
            return Err("ETHRS altı operand gerektirir; P/32bit biçimi yok".into());
        }
        let word = |i: usize| -> Result<Device, String> {
            let v: Device = args[i].parse()?;
            if v.area != Area::D {
                return Err("ETHRS bağlantı/mod/kaynak/hedef doğrudan D olmalı".into());
            }
            Ok(v)
        };
        let length = CalendarValue::parse(&args[3], false)?;
        if !args[3].starts_with(['K', 'H', 'D']) || length.base().is_some_and(|v| v.area != Area::D)
        {
            return Err("ETHRS uzunluk K/H/D olmalı".into());
        }
        let done: Device = args[5].parse()?;
        if done.area != Area::M || !done.writable_bit() {
            return Err("ETHRS completion yazılabilir M olmalı".into());
        }
        Ok(Self {
            connection: word(0)?,
            mode: word(1)?,
            source: word(2)?,
            length,
            destination: word(4)?,
            done,
        })
    }
    pub fn name(&self) -> &'static str {
        "ETHRS"
    }
    pub fn header(&self) -> u16 {
        0xc002
    }
    pub fn words(&self) -> u16 {
        13
    }
    pub fn wide(&self) -> bool {
        false
    }
    pub fn dest(&self) -> Device {
        self.destination
    }
    pub fn operands(&self) -> Vec<RawEthernetOperand> {
        vec![
            RawEthernetOperand::Word(self.connection),
            RawEthernetOperand::Word(self.mode),
            RawEthernetOperand::Word(self.source),
            RawEthernetOperand::Value(self.length.clone()),
            RawEthernetOperand::Word(self.destination),
            RawEthernetOperand::Bit(self.done),
        ]
    }
    pub fn label(&self) -> String {
        format!(
            "ETHRS {} {} {} {} {} {}",
            self.connection, self.mode, self.source, self.length, self.destination, self.done
        )
    }
    pub fn display_value(&self, m: &Memory) -> String {
        format!(
            "ETHRS Done={} Active={} Error={}",
            m.bit(self.done),
            m.system_bit(1197),
            m.system_bit(1198)
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for i in 0..5 {
            if let Some(n) = self.connection.index.checked_add(i) {
                out.insert(d(n));
            }
        }
        out.extend([self.mode, self.source, self.destination, self.done]);
        self.length.devices(out);
        out.extend([1196, 1197, 1198].map(Memory::system_device));
        out.extend([1176, 1227, 1228].map(d));
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        let mode = m.word(self.mode);
        if (1..=3).contains(&mode) {
            if let Some(n) = self.mode.index.checked_add(1) {
                out.insert(d(n));
            }
            for i in 0..=200 {
                if let Some(n) = self.destination.index.checked_add(i)
                    && span(m, d(n), 1)
                {
                    out.insert(d(n));
                }
            }
        }
        if mode != 3 {
            let n = self.length.read(m).clamp(0, 200) as u16;
            for i in 0..n {
                if let Some(n) = self.source.index.checked_add(i)
                    && span(m, d(n), 1)
                {
                    out.insert(d(n));
                }
            }
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let mut s = std::mem::take(&mut m.ethernet_raw);
        s.controls(m);
        s.visit(m, self, power, pc);
        m.ethernet_raw = s;
    }
    fn prepare(&self, m: &Memory, profile: RawEthernetProfile) -> Result<Prepared, PrepareError> {
        if !span(m, self.connection, 5) || !span(m, self.mode, 1) {
            return Err(PrepareError::Code(0x2003));
        }
        let mode = m.word(self.mode) as u16;
        profile.validate(mode).map_err(PrepareError::Unsupported)?;
        let tcp_word = m.word(d(self.connection.index + 4)) as u16;
        if tcp_word > 1 {
            return Err(PrepareError::Code(0x2003));
        }
        let tcp = tcp_word == 1;
        if mode > 4 {
            return Err(PrepareError::Code(if tcp { 0x6202 } else { 0x620a }));
        }
        let low = (m.word(d(self.connection.index + 1)) as u16).to_be_bytes();
        let high = (m.word(d(self.connection.index + 2)) as u16).to_be_bytes();
        let remote = Endpoint {
            ip: [high[0], high[1], low[0], low[1]],
            port: m.word(d(self.connection.index + 3)) as u16,
        };
        let local = m.word(self.connection) as u16;
        let server = mode >= 2;
        if !server && (remote.ip == [0; 4] || remote.port == 0) || server && local == 0 {
            return Err(PrepareError::Code(if tcp { 0x6200 } else { 0x6209 }));
        }
        let reserved = if tcp {
            [502, 44818].contains(&local)
        } else {
            [67, 68, 2222, 44818, 20006, 20008].contains(&local)
        };
        if reserved {
            return Err(PrepareError::Code(0x600c));
        }
        if self.done.index > m.relay_limit.unwrap_or(4095) || !self.done.writable_bit() {
            return Err(PrepareError::Code(0x2003));
        }
        let receives = (1..=3).contains(&mode);
        if receives && !span(m, self.destination, 201) {
            return Err(PrepareError::Code(if tcp { 0x6208 } else { 0x6211 }));
        }
        let timeout_us = if receives {
            if !span(m, self.mode, 2) {
                return Err(PrepareError::Code(0x2003));
            }
            let ms = m.word(d(self.mode.index + 1));
            if ms == 0 && mode >= 2 {
                None
            } else if (100..=32000).contains(&ms) {
                Some(ms as u64 * 1000)
            } else {
                return Err(PrepareError::Code(0x2003));
            }
        } else {
            None
        };
        // Mode3 neither dereferences S3 nor reads/validates S4's register value.
        let tx = if mode == 3 {
            Vec::new()
        } else {
            if let Some(v) = self.length.base()
                && !span(m, v, 1)
            {
                return Err(PrepareError::Code(if tcp { 0x6204 } else { 0x620d }));
            }
            let raw = self.length.read(m);
            if raw == 0 && !(mode == 1 && profile.receive_only_client()) {
                return Err(PrepareError::Code(if tcp { 0x6204 } else { 0x620d }));
            }
            let len = if raw == 0 {
                0
            } else {
                raw.clamp(1, 200) as u16
            };
            if len > 0 && !span(m, self.source, len) {
                return Err(PrepareError::Code(if tcp { 0x6205 } else { 0x620e }));
            }
            (0..len)
                .map(|i| m.word(d(self.source.index + i)) as u8)
                .collect()
        };
        Ok(Prepared {
            config: SocketConfig {
                tcp,
                local_port: local,
                remote,
                server,
            },
            mode,
            tx,
            destination: self.destination,
            done: self.done,
            timeout_us,
            sender_ip: profile.sender_ip(),
        })
    }
}
#[derive(Debug)]
enum PrepareError {
    Code(u16),
    Unsupported(String),
}
#[derive(Debug, Clone)]
struct Prepared {
    config: SocketConfig,
    mode: u16,
    tx: Vec<u8>,
    destination: Device,
    done: Device,
    timeout_us: Option<u64>,
    sender_ip: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawRequestKind {
    Open { config: SocketConfig },
    Send { peer: Endpoint, bytes: Vec<u8> },
    Receive,
    Close,
}
#[derive(Debug, Clone)]
pub struct RawRequest {
    pub id: u64,
    pub epoch: u64,
    pub pc: u16,
    pub at_us: u64,
    pub kind: RawRequestKind,
}
#[derive(Debug, Clone)]
pub enum RawEvent {
    Opened { peer: Option<Endpoint> },
    Sent,
    Packet { peer: Endpoint, bytes: Vec<u8> },
    Closed,
    Fault(u16),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Open,
    Send,
    Receive,
    Close,
}
#[derive(Debug, Clone)]
struct Pending {
    id: u64,
    pc: u16,
    phase: Phase,
    prepared: Option<Prepared>,
    done: Device,
    deadline: Option<u64>,
}
#[derive(Debug, Clone)]
struct Connection {
    config: SocketConfig,
    peer: Option<Endpoint>,
    pc: u16,
    done: Device,
    completed_at: Option<u64>,
}
#[derive(Debug, Clone)]
pub struct RawEthernetState {
    pub profile: RawEthernetProfile,
    pub link_up: bool,
    pub now_us: u64,
    pub notices: BTreeSet<String>,
    pub epoch: u64,
    connection: Option<Connection>,
    pending: Option<Pending>,
    events: BTreeMap<(u64, u64), (u64, u64, RawEvent)>,
    outgoing: VecDeque<RawRequest>,
    next_id: u64,
    sequence: u64,
    last_keep: bool,
    latched: BTreeSet<u16>,
    blocked: Option<(u16, bool, bool)>,
}
impl Default for RawEthernetState {
    fn default() -> Self {
        Self {
            profile: Default::default(),
            link_up: true,
            now_us: 0,
            notices: BTreeSet::new(),
            epoch: 0,
            connection: None,
            pending: None,
            events: BTreeMap::new(),
            outgoing: VecDeque::new(),
            next_id: 0,
            sequence: 0,
            last_keep: false,
            latched: BTreeSet::new(),
            blocked: None,
        }
    }
}
impl RawEthernetState {
    /// Local virtual bind ownership, including an open awaiting its peer event.
    pub fn reserved_ports(&self) -> BTreeSet<(bool, u16)> {
        self.connection
            .iter()
            .map(|c| &c.config)
            .chain(
                self.pending
                    .iter()
                    .filter(|p| p.phase == Phase::Open)
                    .filter_map(|p| p.prepared.as_ref().map(|p| &p.config)),
            )
            .filter(|c| c.local_port != 0)
            .map(|c| (c.tcp, c.local_port))
            .collect()
    }
    pub fn outgoing(&self) -> &VecDeque<RawRequest> {
        &self.outgoing
    }
    pub fn pop_request(&mut self) -> Option<RawRequest> {
        self.outgoing.pop_front()
    }
    pub fn active_count(&self) -> usize {
        usize::from(self.pending.is_some())
    }
    pub fn reset_instruction_state(&mut self) {
        self.pending = None;
        self.connection = None;
        self.events.clear();
        self.outgoing.clear();
        self.latched.clear();
        self.blocked = None;
        self.last_keep = false;
        self.now_us = 0;
        self.epoch = self.epoch.wrapping_add(1);
    }
    pub fn queue_event(
        &mut self,
        at_us: u64,
        id: u64,
        epoch: u64,
        event: RawEvent,
    ) -> Result<(), String> {
        if at_us < self.now_us || matches!(&event,RawEvent::Packet{bytes,..} if bytes.len()>65535) {
            return Err("ETHRS olay zamanı/boyutu geçersiz".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("ETHRS olay sayacı taştı")?;
        self.events
            .insert((at_us, self.sequence), (id, epoch, event));
        Ok(())
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.events
            .first_key_value()
            .map(|(k, _)| k.0)
            .into_iter()
            .chain(self.pending.as_ref().and_then(|p| p.deadline))
            .chain(self.idle_deadline())
            .min()
    }
    fn idle_deadline(&self) -> Option<u64> {
        if self.last_keep || self.pending.is_some() {
            return None;
        }
        self.connection
            .as_ref()
            .filter(|c| c.config.tcp)
            .and_then(|c| c.completed_at)
            .map(|at| at.saturating_add(self.profile.keep_alive_us.max(1)))
    }
    pub fn begin_scan(_m: &mut Memory) {}
    pub fn end_scan(_m: &mut Memory) {}
    pub fn abort(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.ethernet_raw);
        if let Some(p) = &s.pending {
            m.set_internal(p.done, false);
        }
        s.pending = None;
        s.connection = None;
        s.events.clear();
        s.outgoing.clear();
        s.blocked = None;
        s.latched.clear();
        s.epoch = s.epoch.wrapping_add(1);
        m.set_system(1197, false);
        m.ethernet_raw = s;
    }
    pub fn service_controls(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.ethernet_raw);
        s.controls(m);
        m.ethernet_raw = s;
    }
    fn request(&mut self, m: &mut Memory, pc: u16, kind: RawRequestKind) -> Option<u64> {
        let Some(id) = self.next_id.checked_add(1) else {
            m.execution_limitations
                .insert("ETHRS olay kimliği tükendi".into());
            return None;
        };
        self.next_id = id;
        self.outgoing.push_back(RawRequest {
            id,
            epoch: self.epoch,
            pc,
            at_us: self.now_us,
            kind,
        });
        Some(id)
    }
    fn close(&mut self, m: &mut Memory, pc: u16, done: Device, restart: Option<Prepared>) {
        if let Some(id) = self.request(m, pc, RawRequestKind::Close) {
            self.pending = Some(Pending {
                id,
                pc,
                phase: Phase::Close,
                prepared: restart,
                done,
                deadline: None,
            });
            m.set_system(1197, false);
        }
    }
    fn fault(&mut self, m: &mut Memory, pc: u16, done: Device, code: Option<u16>, timeout: bool) {
        let tcp = self.connection.as_ref().map(|c| c.config.tcp).or_else(|| {
            self.pending
                .as_ref()
                .and_then(|p| p.prepared.as_ref())
                .map(|p| p.config.tcp)
        });
        self.pending = None;
        m.set_internal(done, false);
        m.set_system(1198, true);
        if let Some(code) = code {
            m.set_word(d(1176), code as i16);
        } else {
            self.notices.insert(
                "ETHRS UDP timeout: PDF UDP tablosunda sayısal kod yok; D1176 tutuldu".into(),
            );
        }
        self.blocked = Some((pc, timeout, false));
        if !m.system_bit(1196) || !self.link_up {
            if self.connection.is_some() {
                self.close(m, pc, done, None);
            } else {
                m.set_system(1197, false);
            }
        } else {
            m.set_system(1197, self.connection.is_some());
        }
        if tcp.is_none() {
            m.set_system(1197, false);
        }
    }
    fn controls(&mut self, m: &mut Memory) {
        let keep = m.system_bit(1196);
        let falling = self.last_keep && !keep;
        self.last_keep = keep;
        if falling && (self.connection.is_some() || self.pending.is_some()) {
            let (pc, done, tcp) = if let Some(c) = &self.connection {
                (c.pc, c.done, c.config.tcp)
            } else {
                let p = self.pending.as_ref().unwrap();
                (
                    p.pc,
                    p.done,
                    p.prepared.as_ref().is_none_or(|p| p.config.tcp),
                )
            };
            self.pending = None;
            if !tcp && self.profile.udp_falling_done {
                m.set_internal(done, true);
                self.notices
                    .insert("ETHRS UDP M1196 düşüşünde DoneON: PDFnotu aday politikası".into());
            } else {
                m.set_internal(done, false);
            }
            self.close(m, pc, done, None);
        }
        if !self.link_up {
            if let Some(p) = self.pending.clone().filter(|p| p.phase != Phase::Close) {
                self.fault(m, p.pc, p.done, Some(0x600d), false);
            } else if self.pending.is_none()
                && let Some(c) = self.connection.clone()
            {
                self.fault(m, c.pc, c.done, Some(0x600d), false);
            }
        }
    }
    fn visit(&mut self, m: &mut Memory, a: &RawEthernetAction, power: bool, pc: u16) {
        if !power {
            self.latched.remove(&pc);
            if let Some((owner, _, seen)) = &mut self.blocked
                && *owner == pc
            {
                *seen = true;
            }
            if let Some(p) = self.pending.clone()
                && p.pc == pc
                && p.phase != Phase::Close
            {
                self.pending = None;
                m.set_internal(a.done, false);
                if m.system_bit(1196) && self.connection.is_some() {
                    m.set_system(1197, true);
                } else {
                    self.close(m, pc, a.done, None);
                }
            }
            return;
        }
        if self.pending.is_some() {
            return;
        }
        if m.system_bit(1198) {
            return;
        }
        if let Some((owner, needs_false, seen)) = self.blocked {
            if needs_false && (!seen || owner != pc) {
                return;
            }
            self.blocked = None;
        }
        if self.profile.activation == RawActivation::RisingOnly && self.latched.contains(&pc) {
            return;
        }
        if !self.link_up {
            self.fault(m, pc, a.done, Some(0x600d), false);
            return;
        }
        let prepared = match a.prepare(m, self.profile) {
            Ok(p) => p,
            Err(PrepareError::Unsupported(e)) => {
                m.execution_limitations.insert(e);
                return;
            }
            Err(PrepareError::Code(code)) => {
                self.fault(m, pc, a.done, Some(code), false);
                return;
            }
        };
        m.set_internal(a.done, false);
        self.latched.insert(pc);
        if let Some(c) = &self.connection {
            if c.config == prepared.config {
                self.start_phase(m, pc, prepared);
                return;
            }
            // Category/config changes close before rebinding. No implicit socket.
            self.close(m, pc, a.done, Some(prepared));
            return;
        }
        self.open(m, pc, prepared);
    }
    fn open(&mut self, m: &mut Memory, pc: u16, p: Prepared) {
        if p.config.local_port != 0
            && (1..=4)
                .filter_map(|id| m.ethernet_config.socket(id))
                .any(|s| {
                    s.status != 0
                        && s.config.tcp == p.config.tcp
                        && s.config.local_port == p.config.local_port
                })
        {
            m.set_internal(p.done, false);
            m.execution_limitations.insert(
                "ETHRS yerel portu etkin ETHMSG soketi kullanıyor; bağlantı açılmadı".into(),
            );
            return;
        }
        if p.config.local_port == 0 {
            self.notices
                .insert("ETHRS local port0: sanal ephemeral seçim; OSportu açılmaz".into());
        }
        if let Some(id) = self.request(
            m,
            pc,
            RawRequestKind::Open {
                config: p.config.clone(),
            },
        ) {
            let done = p.done;
            self.pending = Some(Pending {
                id,
                pc,
                phase: Phase::Open,
                prepared: Some(p),
                done,
                deadline: None,
            });
            m.set_system(1197, true);
        }
    }
    fn start_phase(&mut self, m: &mut Memory, pc: u16, p: Prepared) {
        if let Some(c) = &mut self.connection {
            c.pc = pc;
            c.done = p.done;
            c.completed_at = None;
        }
        if p.mode == 2 || p.mode == 3 || p.mode == 1 && p.tx.is_empty() {
            self.receive(m, pc, p);
        } else {
            self.send(m, pc, p);
        }
    }
    fn send(&mut self, m: &mut Memory, pc: u16, p: Prepared) {
        let target = self.connection.as_ref().and_then(|c| c.peer).or_else(|| {
            (p.config.remote.ip != [0; 4] && p.config.remote.port != 0).then_some(p.config.remote)
        });
        let Some(peer) = target else {
            self.fault(m, pc, p.done, Some(0x6215), false);
            return;
        };
        if let Some(id) = self.request(
            m,
            pc,
            RawRequestKind::Send {
                peer,
                bytes: p.tx.clone(),
            },
        ) {
            let done = p.done;
            self.pending = Some(Pending {
                id,
                pc,
                phase: Phase::Send,
                prepared: Some(p),
                done,
                deadline: None,
            });
            m.set_system(1197, true);
        }
    }
    fn receive(&mut self, m: &mut Memory, pc: u16, p: Prepared) {
        if let Some(id) = self.request(m, pc, RawRequestKind::Receive) {
            let deadline = p.timeout_us.map(|dt| self.now_us.saturating_add(dt));
            let done = p.done;
            self.pending = Some(Pending {
                id,
                pc,
                phase: Phase::Receive,
                prepared: Some(p),
                done,
                deadline,
            });
            m.set_system(1197, true);
        }
    }
    fn complete(&mut self, m: &mut Memory, pc: u16, p: &Prepared) {
        self.pending = None;
        m.set_internal(p.done, true);
        m.set_system(1197, true);
        if let Some(c) = &mut self.connection {
            c.pc = pc;
            c.done = p.done;
            c.completed_at = Some(self.now_us);
        }
    }
    fn accept_peer(config: &SocketConfig, peer: Endpoint) -> bool {
        if !config.server {
            peer == config.remote
        } else {
            config.remote.ip == [0; 4] || config.remote.port == 0 || peer == config.remote
        }
    }
    fn valid_fault(&self, code: u16, tcp: bool) -> bool {
        if [0x2003, 0x600c, 0x600d, 0x6213, 0x6215, 0x6217].contains(&code) {
            return true;
        }
        if tcp {
            (0x6200..=0x6208).contains(&code) || [0x6212, 0x6214].contains(&code)
        } else {
            [
                0x6209, 0x620a, 0x620c, 0x620d, 0x620e, 0x620f, 0x6210, 0x6211,
            ]
            .contains(&code)
        }
    }
    fn event(&mut self, m: &mut Memory, id: u64, epoch: u64, event: RawEvent) {
        if epoch != self.epoch || self.pending.as_ref().is_none_or(|p| p.id != id) {
            return;
        }
        let pending = self.pending.clone().unwrap();
        if let RawEvent::Fault(code) = event {
            let tcp = pending
                .prepared
                .as_ref()
                .map(|p| p.config.tcp)
                .or_else(|| self.connection.as_ref().map(|c| c.config.tcp))
                .unwrap_or(true);
            if self.valid_fault(code, tcp) {
                self.fault(m, pending.pc, pending.done, Some(code), code == 0x6212);
            } else {
                self.notices
                    .insert("ETHRS belgelenmemiş dış hata kodu göz ardı edildi".into());
            }
            return;
        }
        match (pending.phase, event) {
            (Phase::Close, RawEvent::Closed) => {
                self.pending = None;
                self.connection = None;
                m.set_system(1197, false);
                if let Some(p) = pending.prepared {
                    self.open(m, pending.pc, p);
                }
            }
            (Phase::Open, RawEvent::Opened { peer }) => {
                let p = pending.prepared.unwrap();
                if p.config.tcp && p.config.server && peer.is_none() {
                    self.notices.insert("ETHRS TCP server Opened gerçek peer gerektirir; yalnız bind ACK bağlantı kurmaz".into());
                    return;
                }
                if peer.is_some_and(|peer| !Self::accept_peer(&p.config, peer)) {
                    self.notices.insert(
                        "ETHRS açılış peer'i konfigürasyonla eşleşmedi; ACK tutuldu".into(),
                    );
                    return;
                }
                let effective = peer.or_else(|| (!p.config.server).then_some(p.config.remote));
                self.connection = Some(Connection {
                    config: p.config.clone(),
                    peer: effective,
                    pc: pending.pc,
                    done: p.done,
                    completed_at: None,
                });
                self.pending = None;
                self.start_phase(m, pending.pc, p);
            }
            (Phase::Send, RawEvent::Sent) => {
                let p = pending.prepared.unwrap();
                self.pending = None;
                if p.mode == 1 {
                    self.receive(m, pending.pc, p);
                } else {
                    self.complete(m, pending.pc, &p);
                }
            }
            (Phase::Receive, RawEvent::Packet { peer, bytes }) => {
                let p = pending.prepared.unwrap();
                // Wildcard configuration selects a peer while accepting a
                // TCP connection; it does not permit another sender to inject
                // messages into that established connection. UDP remains
                // connectionless and can select a different wildcard sender.
                let accepted = if p.config.tcp {
                    self.connection
                        .as_ref()
                        .is_some_and(|c| c.peer == Some(peer))
                } else {
                    Self::accept_peer(&p.config, peer)
                };
                if !accepted {
                    self.notices
                        .insert("ETHRS beklenmeyen sender paketi atlandı".into());
                    return;
                }
                if bytes.len() > 200 {
                    self.fault(m, pending.pc, p.done, Some(0x6213), false);
                    return;
                }
                if !span(m, p.destination, bytes.len() as u16 + 1) {
                    self.fault(
                        m,
                        pending.pc,
                        p.done,
                        Some(if p.config.tcp { 0x6208 } else { 0x6211 }),
                        false,
                    );
                    return;
                }
                // Validate the entire packet before the first RAM write.
                m.set_word(p.destination, bytes.len() as i16);
                for (i, byte) in bytes.iter().enumerate() {
                    let target = d(p.destination.index + i as u16 + 1);
                    let old = m.word(target) as u16;
                    m.set_word(target, ((old & 0xff00) | u16::from(*byte)) as i16);
                }
                if p.sender_ip {
                    m.set_word(d(1227), u16::from_be_bytes([peer.ip[2], peer.ip[3]]) as i16);
                    m.set_word(d(1228), u16::from_be_bytes([peer.ip[0], peer.ip[1]]) as i16);
                }
                if let Some(c) = &mut self.connection {
                    c.peer = Some(peer);
                }
                self.pending = None;
                if p.mode == 2 {
                    self.send(m, pending.pc, p);
                } else {
                    self.complete(m, pending.pc, &p);
                }
            }
            _ => {
                self.notices
                    .insert("ETHRS yanlış faz olayı göz ardı edildi".into());
            }
        }
    }
    pub fn advance(m: &mut Memory, at_us: u64) {
        let mut s = std::mem::take(&mut m.ethernet_raw);
        if at_us < s.now_us {
            m.execution_limitations
                .insert("ETHRS saati geriye gidemez".into());
            m.ethernet_raw = s;
            return;
        }
        // Apply controls at the current clock boundary before replaying any
        // queued future event. In particular, a link loss or M1196 falling
        // edge invalidates the old phase ID before a Packet/Sent can commit.
        // Updating last_keep here also prevents an old idle deadline from
        // closing a connection whose keep-alive control has just been enabled.
        s.controls(m);
        while let Some(at) = s.next_deadline_us().filter(|at| *at <= at_us) {
            s.now_us = at;
            s.controls(m);
            // Complete events at the exact receive deadline before timeout.
            let keys: Vec<_> = s.events.range(..=(at, u64::MAX)).map(|(k, _)| *k).collect();
            for key in keys {
                let (id, epoch, event) = s.events.remove(&key).unwrap();
                s.event(m, id, epoch, event);
            }
            if let Some(p) = s.pending.clone()
                && p.deadline.is_some_and(|deadline| deadline <= at)
            {
                let tcp = p.prepared.as_ref().is_none_or(|p| p.config.tcp);
                s.fault(m, p.pc, p.done, if tcp { Some(0x6212) } else { None }, true);
            }
            if s.idle_deadline().is_some_and(|deadline| deadline <= at)
                && let Some(c) = s.connection.clone()
            {
                s.close(m, c.pc, c.done, None);
            }
        }
        s.now_us = at_us;
        s.controls(m);
        m.ethernet_raw = s;
    }
}

#[cfg(test)]
#[path = "ethernet_raw_tests.rs"]
mod tests;
