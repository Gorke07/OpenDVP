//! DMV vision-controller command sequencing (PDF528..532).
//! Semantic virtual peers are explicit. A configured candidate register map can
//! encode operations, but neither mode claims an original DMV wire oracle.
use crate::plc::{Area, Device, Memory};
use crate::serial_io::{SerialCpu, SerialPort};
use crate::serial_protocol::{self, Frame, WireMode};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn d(area: Area, index: u16) -> Device {
    Device { area, index }
}
fn limit(m: &mut Memory, s: impl Into<String>) {
    m.execution_limitations.insert(s.into());
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DmvOperand {
    Word(Device),
    Bit(Device),
}
#[derive(Debug, Clone, PartialEq)]
pub struct DmvAction {
    pub connection: Device,
    pub function: Device,
    pub data: Device,
    pub status: Device,
}
impl DmvAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if name != "DMVRW" || args.len() != 4 {
            return Err("DMVRW dört operand gerektirir; P/32bit biçimi yok".into());
        }
        let parse = |i: usize| -> Result<Device, String> {
            let v: Device = args[i].parse()?;
            if v.area != Area::D {
                return Err("DMVRW ilk üç operandı doğrudan D olmalı".into());
            }
            Ok(v)
        };
        let status: Device = args[3].parse()?;
        if !matches!(status.area, Area::Y | Area::M | Area::S) || !status.writable_bit() {
            return Err("DMVRW durum tabanı yazılabilir Y/M/S olmalı".into());
        }
        Ok(Self {
            connection: parse(0)?,
            function: parse(1)?,
            data: parse(2)?,
            status,
        })
    }
    pub fn name(&self) -> &'static str {
        "DMVRW"
    }
    pub fn header(&self) -> u16 {
        0x6e02
    }
    pub fn words(&self) -> u16 {
        9
    }
    pub fn wide(&self) -> bool {
        false
    }
    pub fn dest(&self) -> Device {
        self.data
    }
    pub fn operands(&self) -> Vec<DmvOperand> {
        vec![
            DmvOperand::Word(self.connection),
            DmvOperand::Word(self.function),
            DmvOperand::Word(self.data),
            DmvOperand::Bit(self.status),
        ]
    }
    pub fn label(&self) -> String {
        format!(
            "DMVRW {} {} {} {}",
            self.connection, self.function, self.data, self.status
        )
    }
    pub fn display_value(&self, m: &Memory) -> String {
        format!(
            "Busy={} Done={} Error={}",
            m.bit(self.status),
            m.bit(d(self.status.area, self.status.index.saturating_add(1))),
            m.bit(d(self.status.area, self.status.index.saturating_add(2)))
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for (base, n) in [
            (self.connection, 4),
            (self.function, 4),
            (self.status, 3),
            (self.data, 1),
        ] {
            for k in 0..n {
                if let Some(index) = base.index.checked_add(k) {
                    out.insert(d(base.area, index));
                }
            }
        }
        for n in [1120, 1129, 1249] {
            out.insert(d(Area::D, n));
        }
        out.insert(d(Area::M, 1120));
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(plan) = self.prepare(m) {
            for i in 0..plan.data_words {
                out.insert(d(Area::D, self.data.index + i));
            }
        }
    }
    pub fn prepare(&self, m: &Memory) -> Result<DmvPlan, String> {
        if !word_span(m, self.connection, 4)
            || !word_span(m, self.function, 4)
            || !flag_span(m, self.status)
        {
            return Err("DMVRW tam kontrol/durum aralığı model sınırını aşıyor".into());
        }
        let connection: [u16; 4] =
            std::array::from_fn(|i| m.word(d(Area::D, self.connection.index + i as u16)) as u16);
        let function: [u16; 4] =
            std::array::from_fn(|i| m.word(d(Area::D, self.function.index + i as u16)) as u16);
        let port=match connection[0]{1=>SerialPort::Com1,2=>SerialPort::Com2,_=>return Err("DMVRW seçilen SS2'de mevcut COM1/COM2 gerektirir; COM3..5 fiziksel kabiliyet değildir".into())};
        let station = u8::try_from(connection[1])
            .ok()
            .filter(|v| (1..=254).contains(v))
            .ok_or("DMVRW istasyon1..254 olmalı")?;
        let selector = function[0];
        if selector > 7 {
            return Err("DMVRW fonksiyon0..7 olmalı".into());
        }
        let reads = selector == 0 && function[2] == 0 || [1, 3, 5, 7].contains(&selector);
        let count_used = reads || selector == 0;
        let n = function[3];
        if count_used && !(1..=16).contains(&n) {
            return Err("DMVRW veri uzunluğu1..16WORD olmalı; sıfır PDF'de tanımlı değil".into());
        }
        let source_words = match selector {
            0 if !reads => n,
            2 | 3 => 1,
            4..=7 => 2,
            _ => 0,
        };
        let data_words = source_words.max(if reads { n } else { 0 }).max(1);
        if !word_span(m, self.data, u32::from(data_words)) {
            return Err("DMVRW bütün kaynak/hedef aralığı model sınırını aşıyor".into());
        }
        let snapshot: Vec<u16> = (0..source_words)
            .map(|i| m.word(d(Area::D, self.data.index + i)) as u16)
            .collect();
        if selector == 0 && u32::from(function[1]) + u32::from(n) > 65536 {
            return Err("DMVRW uzak adres/uzunluk aralığı taşıyor".into());
        }
        let mut operations = Vec::new();
        if selector == 0 {
            if reads {
                operations.push(DmvOperation::ReadRegisters {
                    address: function[1],
                    count: n,
                });
            } else {
                operations.push(DmvOperation::WriteRegisters {
                    address: function[1],
                    values: snapshot,
                });
            }
        } else {
            if [2, 3].contains(&selector) {
                let program = snapshot[0];
                if program > 31 {
                    return Err("DMVRW DMV program numarası0..31 olmalı".into());
                }
                operations.push(DmvOperation::SelectProgram(program));
            }
            if selector >= 4 {
                operations.push(DmvOperation::WriteMemory {
                    bank: if selector <= 5 { 1 } else { 2 },
                    words: [snapshot[0], snapshot[1]],
                });
            }
            operations.push(DmvOperation::Trigger1);
            if reads {
                operations.push(DmvOperation::ReadOutput { count: n });
            }
        }
        Ok(DmvPlan {
            port,
            station,
            selector,
            connection_snapshot: connection,
            function_snapshot: function,
            operations,
            data_words,
            read_destination: reads.then_some(self.data),
            status: self.status,
            function_base: self.function,
        })
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let mut s = std::mem::take(&mut m.dmv_comm);
        s.now_us = m.serial_io.now_us;
        s.visit(m, self, power, pc);
        m.dmv_comm = s;
    }
}
fn word_span(m: &Memory, base: Device, n: u32) -> bool {
    base.area == Area::D
        && n > 0
        && u32::from(base.index) + n - 1 <= u32::from(m.data_register_limit.unwrap_or(9999))
}
fn flag_span(m: &Memory, base: Device) -> bool {
    (0..3).all(|i| {
        base.index.checked_add(i).is_some_and(|index| {
            let v = d(base.area, index);
            let max = match v.area {
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
            index <= max && v.writable_bit() && v.to_string().parse::<Device>().ok() == Some(v)
        })
    })
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DmvOperation {
    ReadRegisters { address: u16, count: u16 },
    WriteRegisters { address: u16, values: Vec<u16> },
    SelectProgram(u16),
    WriteMemory { bank: u8, words: [u16; 2] },
    Trigger1,
    ReadOutput { count: u16 },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmvPlan {
    pub port: SerialPort,
    pub station: u8,
    pub selector: u16,
    pub connection_snapshot: [u16; 4],
    pub function_snapshot: [u16; 4],
    pub operations: Vec<DmvOperation>,
    pub data_words: u16,
    pub read_destination: Option<Device>,
    pub status: Device,
    pub function_base: Device,
}
/// Entirely explicit candidate map. No default addresses or trigger values are
/// fabricated from the programming manual's semantic combination selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmvWireMap {
    pub program: u16,
    pub memory1: u16,
    pub memory2: u16,
    pub trigger: u16,
    pub trigger_value: u16,
    pub output: u16,
    pub write_function: u8,
}
impl DmvWireMap {
    pub fn frame(&self, station: u8, op: &DmvOperation) -> Result<Frame, String> {
        let (address, values, count) = match op {
            DmvOperation::ReadRegisters { address, count } => (*address, None, *count),
            DmvOperation::ReadOutput { count } => (self.output, None, *count),
            DmvOperation::WriteRegisters { address, values } => (*address, Some(values.clone()), 0),
            DmvOperation::SelectProgram(n) => (self.program, Some(vec![*n]), 0),
            DmvOperation::WriteMemory { bank, words } => (
                if *bank == 1 {
                    self.memory1
                } else {
                    self.memory2
                },
                Some(words.to_vec()),
                0,
            ),
            DmvOperation::Trigger1 => (self.trigger, Some(vec![self.trigger_value]), 0),
        };
        let mut data = address.to_be_bytes().to_vec();
        let function = if let Some(values) = values {
            match self.write_function {
                6 if values.len() == 1 => {
                    data.extend_from_slice(&values[0].to_be_bytes());
                    6
                }
                16 => {
                    data.extend_from_slice(&(values.len() as u16).to_be_bytes());
                    data.push((values.len() * 2) as u8);
                    for v in values {
                        data.extend_from_slice(&v.to_be_bytes());
                    }
                    16
                }
                _ => return Err("DMV aday wiremap yazımıFC16 veya tekWORD içinFC6 olmalı".into()),
            }
        } else {
            data.extend_from_slice(&count.to_be_bytes());
            3
        };
        Ok(Frame {
            station,
            function,
            data,
        })
    }
    pub fn encode(
        &self,
        mode: WireMode,
        station: u8,
        op: &DmvOperation,
    ) -> Result<Vec<u8>, String> {
        serial_protocol::encode(mode, &self.frame(station, op)?)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DmvReply {
    Ready,
    Words(Vec<u16>),
    Busy,
    Exception(u8),
    Silent,
}
#[derive(Debug, Clone)]
pub struct VirtualDmvPeer {
    pub registers: BTreeMap<u16, u16>,
    pub output: Vec<u16>,
    pub selected_program: u16,
    pub memory1: [u16; 2],
    pub memory2: [u16; 2],
    pub trigger_count: u64,
    pub response_delay_us: u64,
    pub replies: VecDeque<DmvReply>,
}
impl Default for VirtualDmvPeer {
    fn default() -> Self {
        Self {
            registers: BTreeMap::new(),
            output: vec![0; 16],
            selected_program: 0,
            memory1: [0; 2],
            memory2: [0; 2],
            trigger_count: 0,
            response_delay_us: 1000,
            replies: VecDeque::new(),
        }
    }
}
impl VirtualDmvPeer {
    fn apply(&mut self, op: &DmvOperation) -> Result<Option<Vec<u16>>, String> {
        match op {
            DmvOperation::ReadRegisters { address, count } => {
                if u32::from(*address) + u32::from(*count) > 65536 {
                    return Err("DMV uzak okuma adresi taşması".into());
                }
                Ok(Some(
                    (0..*count)
                        .map(|i| self.registers.get(&(address + i)).copied().unwrap_or(0))
                        .collect(),
                ))
            }
            DmvOperation::WriteRegisters { address, values } => {
                if usize::from(*address) + values.len() > 65536 {
                    return Err("DMV uzak yazma adresi taşması".into());
                }
                for (i, v) in values.iter().enumerate() {
                    self.registers.insert(address + i as u16, *v);
                }
                Ok(None)
            }
            DmvOperation::SelectProgram(n) => {
                self.selected_program = *n;
                Ok(None)
            }
            DmvOperation::WriteMemory { bank, words } => {
                if *bank == 1 {
                    self.memory1 = *words
                } else {
                    self.memory2 = *words
                }
                Ok(None)
            }
            DmvOperation::Trigger1 => {
                self.trigger_count = self.trigger_count.wrapping_add(1);
                Ok(None)
            }
            DmvOperation::ReadOutput { count } => {
                if self.output.len() < usize::from(*count) {
                    return Err("Sanal DMV çıktı bloğu istenen boyutta yapılandırılmadı".into());
                }
                Ok(Some(self.output[..usize::from(*count)].to_vec()))
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DmvFalsePolicy {
    #[default]
    CancelActive,
    ContinueActive,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DmvRetryScope {
    #[default]
    WholeCombination,
    CurrentOperation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DmvTimeoutScope {
    #[default]
    PerAttempt,
    WholeCombination,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DmvFlagReset {
    #[default]
    AllThreeCandidate,
    BusyOnlyLiteral,
}
#[derive(Debug, Clone)]
pub struct DmvRequest {
    pub pc: u16,
    pub port: SerialPort,
    pub station: u8,
    pub operation: DmvOperation,
    pub at_us: u64,
    pub attempt: u64,
}
#[derive(Debug, Clone)]
struct Job {
    plan: DmvPlan,
    index: usize,
    response_at: Option<u64>,
    timeout_at: u64,
    whole_deadline: u64,
    reply: DmvReply,
    attempt: u64,
}
#[derive(Debug, Clone, Default)]
pub struct DmvState {
    pub now_us: u64,
    pub notices: BTreeSet<String>,
    pub false_policy: DmvFalsePolicy,
    pub exception_retry: DmvRetryScope,
    pub timeout_scope: DmvTimeoutScope,
    pub flag_reset: DmvFlagReset,
    /// Peer latency and read snapshots are semantic model inputs, not byte timing.
    pub retry_delay_us: u64,
    pub wire_map: Option<DmvWireMap>,
    peers: BTreeMap<(SerialPort, u8), VirtualDmvPeer>,
    jobs: BTreeMap<u16, Job>,
    latched: BTreeSet<u16>,
    requests: VecDeque<DmvRequest>,
}
impl DmvState {
    pub fn configure_peer(
        &mut self,
        port: SerialPort,
        station: u8,
        peer: VirtualDmvPeer,
    ) -> Result<(), String> {
        if !(1..=254).contains(&station) {
            return Err("DMV istasyonu1..254 olmalı".into());
        }
        if self
            .jobs
            .values()
            .any(|j| j.plan.port == port && j.plan.station == station)
        {
            return Err("Etkin DMV eşini değiştirmeden önce işlemi durdurun".into());
        }
        self.peers.insert((port, station), peer);
        Ok(())
    }
    pub fn peer(&self, port: SerialPort, station: u8) -> Option<&VirtualDmvPeer> {
        self.peers.get(&(port, station))
    }
    pub fn peer_mut(&mut self, port: SerialPort, station: u8) -> Option<&mut VirtualDmvPeer> {
        self.peers.get_mut(&(port, station))
    }
    pub fn outgoing_requests(&self) -> &VecDeque<DmvRequest> {
        &self.requests
    }
    pub fn pop_request(&mut self) -> Option<DmvRequest> {
        self.requests.pop_front()
    }
    pub fn active_count(&self) -> usize {
        self.jobs.len()
    }
    pub fn reset_instruction_state(&mut self) {
        self.jobs.clear();
        self.latched.clear();
        self.requests.clear();
        self.now_us = 0;
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.jobs
            .values()
            .flat_map(|j| [j.response_at, Some(j.timeout_at)])
            .flatten()
            .filter(|at| *at > self.now_us)
            .min()
    }
    pub fn begin_scan(_m: &mut Memory) {}
    pub fn end_scan(_m: &mut Memory) {}
    pub fn abort(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.dmv_comm);
        for (&pc, j) in &s.jobs {
            m.serial_io.release_external(pc);
            set_flags(m, j.plan.status, false, false, true);
        }
        s.jobs.clear();
        s.latched.clear();
        s.requests.clear();
        m.dmv_comm = s;
    }
    pub fn advance(m: &mut Memory, at_us: u64) {
        let mut s = std::mem::take(&mut m.dmv_comm);
        if at_us < s.now_us {
            limit(m, "DMV zamanı geriye gidiyor");
            m.dmv_comm = s;
            return;
        }
        let mut work = 0;
        while let Some(at) = s.next_deadline_us().filter(|v| *v <= at_us) {
            s.now_us = at;
            let ids: Vec<_> = s.jobs.keys().copied().collect();
            for pc in ids {
                let mut j = s.jobs.remove(&pc).unwrap();
                if s.service(m, pc, &mut j) {
                    s.jobs.insert(pc, j);
                }
            }
            work += 1;
            if work >= 8192 {
                limit(
                    m,
                    "DMV retry olay bütçesi doldu; daha küçük zaman adımı gerekli",
                );
                m.dmv_comm = s;
                return;
            }
        }
        s.now_us = at_us;
        m.dmv_comm = s;
    }
    fn timeout(m: &Memory, port: SerialPort) -> u64 {
        let raw = m.word(d(
            Area::D,
            if port == SerialPort::Com1 { 1249 } else { 1129 },
        ));
        if raw <= 0 {
            100000
        } else {
            raw.max(50) as u64 * 1000
        }
    }
    fn visit(&mut self, m: &mut Memory, a: &DmvAction, power: bool, pc: u16) {
        if !power {
            self.latched.remove(&pc);
            if self.false_policy == DmvFalsePolicy::CancelActive
                && let Some(j) = self.jobs.remove(&pc)
            {
                m.serial_io.release_external(pc);
                set_flags(m, j.plan.status, false, false, false);
            }
            return;
        }
        if self.latched.contains(&pc) || self.jobs.contains_key(&pc) {
            return;
        }
        let profile = m.serial_io.profile();
        if profile.cpu != SerialCpu::Ss2
            || (profile.firmware_major, profile.firmware_minor) < (3, 2)
        {
            limit(
                m,
                "DMVRW yalnız SS2 firmware3.2 ve sonrasında belgelenmiştir",
            );
            return;
        }
        let plan = match a.prepare(m) {
            Ok(p) => p,
            Err(e) => {
                limit(m, e);
                return;
            }
        };
        // A configured port and an explicit peer are mandatory; absent peers
        // are distinct from an explicitly configured Silent response fixture.
        if !self.peers.contains_key(&(plan.port, plan.station)) {
            limit(
                m,
                "DMVRW açık sanal DMV eşi gerektirir; fiziksel wiremap/cevap varsayılmaz",
            );
            return;
        }
        if let Some(owner) = m.serial_io.external_owner(plan.port)
            && owner != pc
        {
            return;
        }
        if m.serial_io.transaction(plan.port).is_some() {
            return;
        }
        if let Err(e) = m.serial_io.reserve_external(plan.port, pc) {
            limit(m, e);
            return;
        }
        self.latched.insert(pc);
        match self.flag_reset {
            DmvFlagReset::AllThreeCandidate => set_flags(m, plan.status, false, false, false),
            DmvFlagReset::BusyOnlyLiteral => m.set_internal(plan.status, false),
        }
        self.notices.insert(
            "DMVRW: açık semantic DMV peer, fiziksel byte/checksum veya native eşitliği değildir"
                .into(),
        );
        if plan.selector != 0 && self.wire_map.is_none() {
            self.notices.insert(
                "DMVRW birleşikkomut S2+1/+2 uzakadres writeback'i DMO wiremap olmadan korunur"
                    .into(),
            );
        }
        let timeout = Self::timeout(m, plan.port);
        let mut j = Job {
            plan,
            index: 0,
            response_at: None,
            timeout_at: self.now_us.saturating_add(timeout),
            whole_deadline: self.now_us.saturating_add(timeout),
            reply: DmvReply::Silent,
            attempt: 0,
        };
        self.send(m, pc, &mut j, 0);
        self.jobs.insert(pc, j);
    }
    fn send(&mut self, m: &mut Memory, pc: u16, j: &mut Job, delay: u64) {
        j.attempt = j.attempt.saturating_add(1);
        let at = self.now_us.saturating_add(delay);
        let peer = self
            .peers
            .get_mut(&(j.plan.port, j.plan.station))
            .expect("peer retained while active");
        j.reply = peer.replies.pop_front().unwrap_or(DmvReply::Ready);
        j.response_at = if j.reply == DmvReply::Silent {
            None
        } else {
            Some(at.saturating_add(peer.response_delay_us.max(1)))
        };
        j.timeout_at = if self.timeout_scope == DmvTimeoutScope::WholeCombination {
            j.whole_deadline
        } else {
            at.saturating_add(Self::timeout(m, j.plan.port))
        };
        let operation = j.plan.operations[j.index].clone();
        if j.plan.selector != 0
            && let Some(map) = &self.wire_map
            && let Ok(frame) = map.frame(j.plan.station, &operation)
        {
            let addr = u16::from_be_bytes([frame.data[0], frame.data[1]]);
            m.set_word(d(Area::D, j.plan.function_base.index + 1), addr as i16);
            m.set_word(
                d(Area::D, j.plan.function_base.index + 2),
                i16::from(!matches!(
                    operation,
                    DmvOperation::ReadRegisters { .. } | DmvOperation::ReadOutput { .. }
                )),
            );
        }
        m.set_internal(j.plan.status, true);
        self.requests.push_back(DmvRequest {
            pc,
            port: j.plan.port,
            station: j.plan.station,
            operation,
            at_us: at,
            attempt: j.attempt,
        });
    }
    fn fail(&mut self, m: &mut Memory, pc: u16, j: &Job, e: Option<String>) {
        set_flags(m, j.plan.status, false, false, true);
        m.serial_io.release_external(pc);
        if let Some(e) = e {
            limit(m, e);
        }
    }
    fn service(&mut self, m: &mut Memory, pc: u16, j: &mut Job) -> bool {
        // Exact-time tie gives an arrived response priority; policy is explicit.
        if j.response_at
            .is_some_and(|at| at <= self.now_us && at <= j.timeout_at)
        {
            j.response_at = None;
            match &j.reply {
                DmvReply::Busy => {
                    if self.timeout_scope == DmvTimeoutScope::WholeCombination
                        && self.now_us >= j.whole_deadline
                    {
                        self.fail(m, pc, j, None);
                        return false;
                    }
                    self.send(m, pc, j, self.retry_delay_us.max(1));
                    return true;
                }
                DmvReply::Exception(_) => {
                    if self.timeout_scope == DmvTimeoutScope::WholeCombination
                        && self.now_us >= j.whole_deadline
                    {
                        self.fail(m, pc, j, None);
                        return false;
                    }
                    if self.exception_retry == DmvRetryScope::WholeCombination {
                        j.index = 0;
                    }
                    self.send(m, pc, j, self.retry_delay_us.max(1));
                    return true;
                }
                DmvReply::Silent => {}
                reply => {
                    let operation = &j.plan.operations[j.index];
                    let result = match reply {
                        DmvReply::Words(words) => {
                            if !matches!(
                                operation,
                                DmvOperation::ReadRegisters { .. }
                                    | DmvOperation::ReadOutput { .. }
                            ) {
                                Err("DMV yazma onayı yerine veri döndü".into())
                            } else {
                                Ok(Some(words.clone()))
                            }
                        }
                        _ => self
                            .peers
                            .get_mut(&(j.plan.port, j.plan.station))
                            .expect("active peer")
                            .apply(operation),
                    };
                    match result {
                        Err(e) => {
                            self.fail(m, pc, j, Some(e));
                            return false;
                        }
                        Ok(words) => {
                            if let Some(words) = words {
                                let expected = match operation {
                                    DmvOperation::ReadRegisters { count, .. }
                                    | DmvOperation::ReadOutput { count } => usize::from(*count),
                                    _ => 0,
                                };
                                let Some(dest) = j.plan.read_destination else {
                                    self.fail(m, pc, j, Some("DMV beklenmeyen readback".into()));
                                    return false;
                                };
                                if words.len() != expected || !word_span(m, dest, expected as u32) {
                                    self.fail(m,pc,j,Some("DMV readback uzunluğu/hedef aralığı geçersiz; veri tutuldu".into()));
                                    return false;
                                }
                                for (i, w) in words.into_iter().enumerate() {
                                    m.set_word(d(Area::D, dest.index + i as u16), w as i16);
                                }
                            }
                        }
                    }
                    j.index += 1;
                    if j.index == j.plan.operations.len() {
                        set_flags(m, j.plan.status, false, true, false);
                        m.serial_io.release_external(pc);
                        return false;
                    }
                    self.send(m, pc, j, 0);
                    return true;
                }
            }
        }
        if self.now_us >= j.timeout_at {
            self.fail(m, pc, j, None);
            return false;
        }
        true
    }
}
fn set_flags(m: &mut Memory, base: Device, busy: bool, done: bool, error: bool) {
    for (i, value) in [busy, done, error].into_iter().enumerate() {
        m.set_internal(d(base.area, base.index + i as u16), value);
    }
}
#[cfg(test)]
#[path = "dmv_comm_tests.rs"]
mod tests;
