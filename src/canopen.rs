//! Explicit CANopen control-plane simulation. Requests require external replies.
//! Typed parameter intents are not a claimed byte-exact SDO/PDO implementation.
use crate::calendar_operand::CalendarValue;
use crate::plc::{Area, Device, Memory};
use crate::serial_io::SerialOperand;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn m(index: u16) -> Device {
    Device {
        area: Area::M,
        index,
    }
}
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanCpu {
    Es2C,
    Se2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanServoModel {
    A2M,
    A3M,
    B3M,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanProfile {
    pub cpu: CanCpu,
    pub firmware_major: u16,
    pub firmware_minor: u8,
    pub servo: CanServoModel,
}
impl CanProfile {
    fn modern(self) -> bool {
        self.cpu == CanCpu::Se2 || (self.firmware_major, self.firmware_minor) >= (3, 60)
    }
    fn validate(self) -> Result<(), String> {
        if self.firmware_minor > 99 {
            return Err("CAN firmware alt sürümü 0..99 olmalı".into());
        }
        let version = (self.firmware_major, self.firmware_minor);
        if self.cpu == CanCpu::Se2 && version < (1, 0)
            || self.cpu == CanCpu::Es2C && version < (3, 48)
        {
            return Err("INITC/ASDON profili ES2-C>=3.48 veya SE2>=1.00 gerektirir".into());
        }
        if self.cpu == CanCpu::Es2C && self.servo != CanServoModel::A2M && version < (3, 60) {
            return Err("ASDA-A3-M/B3-M ES2-C>=3.60 gerektirir".into());
        }
        if self.cpu == CanCpu::Es2C && version > (3, 48) && version < (3, 60) {
            return Err("ES2-C3.49..3.59 heartbeat geçiş planı PDF'de tam açıklanmıyor".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanOperation {
    ParameterWrite { parameter: String, value: i32 },
    ObjectWrite { index: u16, value: u32 },
    PdoMap { number: u8, entries: Vec<String> },
    ServoEnable(bool),
    ResetDelta,
    Nmt(u8),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanRequest {
    pub id: u64,
    pub epoch: u64,
    pub node: u8,
    pub pc: u16,
    pub at_us: u64,
    pub operation: CanOperation,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanReply {
    Ack,
    Fault { code: Option<u16> },
}
#[derive(Debug, Clone, Default)]
pub struct CanNode {
    pub online: bool,
    pub initialized: bool,
    pub previously_worked: bool,
    pub servo_enabled: bool,
    pub parameters: BTreeMap<String, i32>,
    pub objects: BTreeMap<u16, u32>,
    pub pdo_maps: BTreeMap<u8, Vec<String>>,
    pub nmt_state: Option<CanNmtState>,
    pub fault: Option<u16>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanNmtState {
    Operational,
    Stopped,
    PreOperational,
    ResetApplicationPending,
    ResetCommunicationPending,
}
#[derive(Debug, Clone, PartialEq)]
pub enum CanopenAction {
    Init {
        count: u16,
    },
    ServoEnable {
        node: u16,
        enabled: bool,
    },
    /// RSTD header BE02 is recovered from the installed Delta editor compiler table.
    Reset {
        node: CalendarValue,
        parameter: Device,
        ok: Device,
        error: Device,
    },
}
impl CanopenAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let constant = |raw: &str| -> Result<i16, String> {
            if !raw.starts_with(['K', 'H']) {
                return Err("INITC/ASDON yalnız K/H sabit kabul eder".into());
            }
            match crate::plc::Operand::parse(raw)? {
                crate::plc::Operand::Constant(v) => Ok(v),
                _ => Err("K/H sabit gerekli".into()),
            }
        };
        match (name, args.len()) {
            ("INITC", 1) => Ok(Self::Init {
                count: constant(&args[0])? as u16,
            }),
            ("ASDON", 2) => Ok(Self::ServoEnable {
                node: constant(&args[0])? as u16,
                enabled: constant(&args[1])? != 0,
            }),
            ("RSTD", 4) => Self::reset_unencoded(
                CalendarValue::parse(&args[0], false)?,
                args[1].parse()?,
                args[2].parse()?,
                args[3].parse()?,
            ),
            _ => Err(
                "INITC bir K/H, ASDON iki K/H; RSTD Node Para Ok Err gerektirir. P/D biçimi yok"
                    .into(),
            ),
        }
    }
    pub fn reset_unencoded(
        node: CalendarValue,
        parameter: Device,
        ok: Device,
        error: Device,
    ) -> Result<Self, String> {
        if parameter.area != Area::D || ok.area != Area::M || error.area != Area::M {
            return Err("RSTD Para D, Ok/Err M olmalı".into());
        }
        if !matches!(
            &node,
            CalendarValue::Additional(crate::additional_processing::AdditionalValue::Word(
                crate::plc::Operand::Constant(_)
                    | crate::plc::Operand::Register(Device { area: Area::D, .. })
            ))
        ) {
            return Err("RSTD Node K/H/D olmalı".into());
        }
        Ok(Self::Reset {
            node,
            parameter,
            ok,
            error,
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Init { .. } => "INITC",
            Self::ServoEnable { .. } => "ASDON",
            Self::Reset { .. } => "RSTD",
        }
    }
    pub fn header(&self) -> Option<u16> {
        match self {
            Self::Init { .. } => Some(0xaf02),
            Self::ServoEnable { .. } => Some(0xb002),
            Self::Reset { .. } => Some(0xbe02),
        }
    }
    pub fn words(&self) -> u16 {
        match self {
            Self::Init { .. } => 3,
            Self::ServoEnable { .. } => 5,
            Self::Reset { .. } => 9,
        }
    }
    pub fn wide(&self) -> bool {
        false
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Init { .. } => m(1615),
            Self::ServoEnable { node, .. } => m(1639 + (*node).min(8)),
            Self::Reset { ok, .. } => *ok,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Init { count } => format!("INITC K{count}"),
            Self::ServoEnable { node, enabled } => format!("ASDON K{node} K{}", u8::from(*enabled)),
            Self::Reset {
                node,
                parameter,
                ok,
                error,
            } => format!("RSTD {node} {parameter} {ok} {error}"),
        }
    }
    pub fn operands(&self) -> Vec<SerialOperand> {
        let value = |v: i32| {
            SerialOperand::Value(CalendarValue::parse(&format!("K{v}"), false).expect("constant"))
        };
        match self {
            Self::Init { count } => vec![value(i32::from(*count as i16))],
            Self::ServoEnable { node, enabled } => {
                vec![value(i32::from(*node as i16)), value(i32::from(*enabled))]
            }
            Self::Reset {
                node,
                parameter,
                ok,
                error,
            } => vec![
                SerialOperand::Value(node.clone()),
                SerialOperand::Data(*parameter),
                SerialOperand::Data(*ok),
                SerialOperand::Data(*error),
            ],
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.extend([1614, 1615, 1616, 1617].map(m));
        out.extend([6000, 6001, 6002].map(d));
        out.extend((1640..=1647).map(m));
        if let Self::Reset {
            node,
            parameter,
            ok,
            error,
        } = self
        {
            node.devices(out);
            out.extend([*parameter, *ok, *error]);
        }
    }
    pub fn observed_devices(&self, _memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        format!("{}={}", self.dest(), memory.bit(self.dest()))
    }
    pub fn execute(&self, memory: &mut Memory, power: bool, pc: u16) {
        let now = memory.timing_io.now_us;
        if now != memory.canopen.now_us {
            CanopenNetwork::advance(memory, now);
        }
        let mut network = std::mem::take(&mut memory.canopen);
        network.visit(memory, self, power, pc);
        memory.canopen = network;
    }
}
#[derive(Debug, Clone)]
enum JobKind {
    Init(Vec<u8>),
    Enable,
    Reset {
        ok: Device,
        error: Device,
        suppressed: bool,
        restore: Vec<u8>,
    },
}
#[derive(Debug, Clone)]
struct Job {
    pc: u16,
    kind: JobKind,
    remaining: VecDeque<(u8, CanOperation)>,
    pending: Option<CanRequest>,
}
#[derive(Debug, Clone, Default)]
pub struct CanopenNetwork {
    pub now_us: u64,
    pub notices: BTreeSet<String>,
    profile: Option<CanProfile>,
    nodes: BTreeMap<u8, CanNode>,
    active: Option<Job>,
    exchange_owner: Option<(u16, u16)>,
    completed: BTreeSet<u16>,
    epoch: u64,
    sequence: u64,
    replies: BTreeMap<(u64, u64), (u64, u64, CanReply)>,
    outgoing: VecDeque<CanRequest>,
}
impl CanopenNetwork {
    /// One conservative CAN control-plane resource shared with exchange commands.
    pub fn reserve_exchange(&mut self, port: u16, pc: u16) -> bool {
        if self.active.is_some() {
            return false;
        }
        match self.exchange_owner {
            Some(owner) => owner == (port, pc),
            None => {
                self.exchange_owner = Some((port, pc));
                true
            }
        }
    }
    pub fn release_exchange(&mut self, pc: u16) {
        if self.exchange_owner.is_some_and(|(_, owner)| owner == pc) {
            self.exchange_owner = None;
        }
    }
    pub fn exchange_owner(&self) -> Option<(u16, u16)> {
        self.exchange_owner
    }
    pub fn profile(&self) -> Option<CanProfile> {
        self.profile
    }
    pub fn set_profile(&mut self, profile: CanProfile) -> Result<(), String> {
        profile.validate()?;
        if self.active.is_some() || self.exchange_owner.is_some() || !self.nodes.is_empty() {
            return Err("CAN profil değişimi etkin işlem/node ortamı varken yapılamaz".into());
        }
        self.profile = Some(profile);
        Ok(())
    }
    pub fn configure_node(&mut self, id: u8, online: bool) -> Result<(), String> {
        if self.profile.is_none()
            || !(1..=127).contains(&id)
            || self.active.is_some()
            || self.exchange_owner.is_some()
        {
            return Err("CAN node açık profil,1..127 ve boş owner gerektirir".into());
        }
        if self.nodes.contains_key(&id) {
            return Err("CAN node zaten yapılandırılmış".into());
        }
        self.nodes.insert(
            id,
            CanNode {
                online,
                ..CanNode::default()
            },
        );
        Ok(())
    }
    pub fn node(&self, id: u8) -> Option<&CanNode> {
        self.nodes.get(&id)
    }
    pub fn nodes(&self) -> impl Iterator<Item = (&u8, &CanNode)> {
        self.nodes.iter()
    }
    pub fn outgoing_requests(&self) -> &VecDeque<CanRequest> {
        &self.outgoing
    }
    pub fn pop_outgoing(&mut self) -> Option<CanRequest> {
        self.outgoing.pop_front()
    }
    pub fn pending(&self) -> Option<&CanRequest> {
        self.active.as_ref()?.pending.as_ref()
    }
    pub fn queue_reply(
        &mut self,
        at_us: u64,
        request_id: u64,
        epoch: u64,
        reply: CanReply,
    ) -> Result<(), String> {
        if at_us < self.now_us || self.replies.len() >= 4096 {
            return Err("CAN yanıtı gelecek zaman ve4096olay sınırı gerektirir".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("CAN sıra numarası taştı")?;
        self.replies
            .insert((at_us, self.sequence), (request_id, epoch, reply));
        Ok(())
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.replies.first_key_value().map(|(k, _)| k.0)
    }
    pub fn begin_scan(_memory: &mut Memory) {}
    pub fn end_scan(_memory: &mut Memory) {}
    /// Drop instruction epoch, preserve explicitly configured node environment.
    pub fn reset_instruction_state(&mut self) {
        self.active = None;
        self.exchange_owner = None;
        self.completed.clear();
        self.replies.clear();
        self.outgoing.clear();
        self.now_us = 0;
        self.epoch = self.epoch.wrapping_add(1);
    }
    pub fn restart_cpu_clock(&mut self, elapsed_cpu_us: u64) -> Result<(), String> {
        if elapsed_cpu_us < self.now_us {
            return Err("CAN CPU reset zamanı işlenmiş sanal saatten önce olamaz".into());
        }
        self.reset_instruction_state();
        Ok(())
    }
    pub fn abort(memory: &mut Memory) {
        let mut n = std::mem::take(&mut memory.canopen);
        let now = n.now_us;
        n.reset_instruction_state();
        n.now_us = now;
        memory.set_system(1615, false);
        for node in n.nodes.values_mut() {
            node.initialized = false;
            node.servo_enabled = false;
        }
        for id in 1..=8 {
            memory.set_system(1639 + id, false);
        }
        memory.canopen = n;
    }
    pub fn advance(memory: &mut Memory, at_us: u64) {
        let mut n = std::mem::take(&mut memory.canopen);
        if at_us < n.now_us {
            memory
                .execution_limitations
                .insert("CAN saati geriye gidiyor".into());
            memory.canopen = n;
            return;
        }
        while let Some((&key, _)) = n.replies.first_key_value() {
            if key.0 > at_us {
                break;
            }
            let (id, epoch, reply) = n.replies.remove(&key).unwrap();
            n.now_us = key.0;
            n.receive(memory, id, epoch, reply);
        }
        n.now_us = at_us;
        memory.canopen = n;
    }
    /// Full PDF parameter/PDO intent sequence. Actual SDO byte addresses remain open.
    pub fn initialization_plan(profile: CanProfile) -> Result<Vec<CanOperation>, String> {
        profile.validate()?;
        let mut plan = Vec::new();
        let mut parameter = |name: &str, value: i32| {
            plan.push(CanOperation::ParameterWrite {
                parameter: name.into(),
                value,
            })
        };
        if profile.modern() {
            parameter("P2-30", 5);
        }
        for (name, value) in [
            ("P6-02", 0),
            ("P6-06", 0),
            ("P3-06", 0x100),
            ("P4-07", 0),
            ("P2-36", 0x101),
            ("P0-17", 1),
            ("P0-18", 64),
        ] {
            parameter(name, value);
        }
        for i in 20..=35 {
            parameter(&format!("P5-{i:02}"), 1);
        }
        for i in 60..=75 {
            parameter(&format!("P5-{i:02}"), 1);
        }
        if profile.modern() {
            parameter("P3-10", 0x10);
        }
        plan.push(CanOperation::PdoMap {
            number: 1,
            entries: ["P5-07", "P0-01", "P0-46", "P4-07"]
                .map(String::from)
                .to_vec(),
        });
        plan.push(CanOperation::PdoMap {
            number: 2,
            entries: ["P0-09", "P0-10"].map(String::from).to_vec(),
        });
        if profile.modern() {
            for index in [0x1017, 0x100c, 0x100d] {
                plan.push(CanOperation::ObjectWrite { index, value: 0 });
            }
            plan.push(CanOperation::ObjectWrite {
                index: 0x1016,
                value: 200,
            });
        } else {
            plan.push(CanOperation::ObjectWrite {
                index: 0x1017,
                value: 200,
            });
        }
        Ok(plan)
    }
    fn visit(&mut self, memory: &mut Memory, action: &CanopenAction, power: bool, pc: u16) {
        if !power {
            self.completed.remove(&pc);
            if let Some(job) = &mut self.active
                && job.pc == pc
                && let JobKind::Reset { suppressed, .. } = &mut job.kind
            {
                *suppressed = true;
            }
            return;
        }
        if self.active.is_some() || self.exchange_owner.is_some() || self.completed.contains(&pc) {
            return;
        }
        let Some(profile) = self.profile else {
            memory
                .execution_limitations
                .insert("CANopen açık ES2-C/SE2 profil ve node ortamı gerektirir".into());
            return;
        };
        if memory.data_register_limit.unwrap_or(9999) < 6002 {
            memory
                .execution_limitations
                .insert("CAN hata kaydı D6000..6002 tam model aralığında olmalı".into());
            return;
        }
        let mut work = VecDeque::new();
        let result = (|| -> Result<JobKind, String> {
            match action {
                CanopenAction::Init { count } => {
                    if !memory.system_bit(1614) {
                        return Err("INITC Delta özel modu M1614ON gerektirir".into());
                    }
                    if *count == 0 || (*count as i16) < 0 {
                        return Err(
                            "INITC sıfır/negatif istasyon sayısı PDF'de tanımlanmıyor".into()
                        );
                    }
                    let ids = (1..=(*count).min(8) as u8).collect::<Vec<_>>();
                    for id in &ids {
                        if !self.nodes.get(id).is_some_and(|n| n.online) {
                            return Err(format!("INITC node{id} açık çevrimiçi peer gerektirir"));
                        }
                        for op in Self::initialization_plan(profile)? {
                            work.push_back((*id, op));
                        }
                    }
                    Ok(JobKind::Init(ids))
                }
                CanopenAction::ServoEnable { node, enabled } => {
                    if !(1..=8).contains(node) {
                        return Ok(JobKind::Enable);
                    }
                    if !memory.system_bit(1614)
                        || !memory.system_bit(1615)
                        || !self
                            .nodes
                            .get(&(*node as u8))
                            .is_some_and(|n| n.online && n.initialized)
                    {
                        return Err("ASDON gerçek INITC tamamlanması ve çevrimiçi initialized node gerektirir".into());
                    }
                    work.push_back((*node as u8, CanOperation::ServoEnable(*enabled)));
                    Ok(JobKind::Enable)
                }
                CanopenAction::Reset {
                    node,
                    parameter,
                    ok,
                    error,
                } => {
                    if profile.cpu == CanCpu::Es2C
                        && (profile.firmware_major, profile.firmware_minor) < (3, 60)
                    {
                        return Err("RSTD ES2-C>=3.60 gerektirir".into());
                    }
                    for target in [*ok, *error] {
                        if target.area != Area::M
                            || target.index > memory.relay_limit.unwrap_or(4095)
                            || !target.writable_bit()
                            || target.to_string().parse::<Device>().is_err()
                        {
                            return Err("RSTD Ok/Err tam model M adresi olmalı".into());
                        }
                    }
                    let mut source = BTreeSet::new();
                    node.devices(&mut source);
                    if source.iter().any(|d| {
                        d.area != Area::D || d.index > memory.data_register_limit.unwrap_or(9999)
                    }) {
                        return Err("RSTD Node D aralığı model sınırını aşıyor".into());
                    }
                    let value = node.read(memory);
                    let delta = memory.system_bit(1614);
                    let maximum = if delta { 8 } else { 16 };
                    if !(0..=maximum).contains(&value) {
                        memory.set_internal(*error, true);
                        return Err(
                            "RSTD Node geçersiz; sayısal D6001 hata kodu doğrulanmadı".into()
                        );
                    }
                    let ids = if value == 0 {
                        self.nodes
                            .keys()
                            .copied()
                            .filter(|n| i32::from(*n) <= maximum)
                            .collect::<Vec<_>>()
                    } else {
                        vec![value as u8]
                    };
                    if ids.is_empty() {
                        return Err("RSTD broadcast için açık node ortamı yok".into());
                    }
                    let code = if delta {
                        0
                    } else {
                        if parameter.area != Area::D
                            || parameter.index > memory.data_register_limit.unwrap_or(9999)
                        {
                            return Err("RSTD Para D adresi model sınırını aşıyor".into());
                        }
                        let v = memory.word(*parameter) as u16;
                        if ![1, 2, 0x80, 0x81, 0x82].contains(&v) {
                            memory.set_internal(*error, true);
                            return Err(
                                "RSTD NMT kodu geçersiz; sayısal D6001 hata kodu doğrulanmadı"
                                    .into(),
                            );
                        }
                        v as u8
                    };
                    for id in &ids {
                        let n = self.nodes.get(id).ok_or("RSTD açık node peer gerektirir")?;
                        if !n.online || delta && !n.previously_worked {
                            return Err(
                                "Delta RSTD daha önce normal çalışmış çevrimiçi node gerektirir"
                                    .into(),
                            );
                        }
                        work.push_back((
                            *id,
                            if delta {
                                CanOperation::ResetDelta
                            } else {
                                CanOperation::Nmt(code)
                            },
                        ));
                        if delta {
                            for op in Self::initialization_plan(profile)? {
                                work.push_back((*id, op));
                            }
                        }
                    }
                    Ok(JobKind::Reset {
                        ok: *ok,
                        error: *error,
                        suppressed: false,
                        restore: if delta { ids } else { Vec::new() },
                    })
                }
            }
        })();
        let kind = match result {
            Ok(v) => v,
            Err(e) => {
                memory.execution_limitations.insert(e);
                return;
            }
        };
        if work.is_empty() {
            return;
        }
        if self.sequence.checked_add(1).is_none() {
            memory
                .execution_limitations
                .insert("CAN istek sıra numarası taştı".into());
            return;
        }
        if matches!(kind, JobKind::Init(_)) {
            memory.set_system(1615, false);
        }
        self.notices.insert("CANopen typed yerel işlem planı: gerçek peer ACK gerekir; byte-exact SDO/PDO, heartbeat ve fiziksel servo eşitliği açık facet".into());
        self.active = Some(Job {
            pc,
            kind,
            remaining: work,
            pending: None,
        });
        self.send_next(memory);
    }
    fn send_next(&mut self, memory: &mut Memory) {
        let Some(mut job) = self.active.take() else {
            return;
        };
        if let Some((node, operation)) = job.remaining.pop_front() {
            let Some(id) = self.sequence.checked_add(1) else {
                memory
                    .execution_limitations
                    .insert("CAN istek sayacı taştı".into());
                return;
            };
            self.sequence = id;
            let request = CanRequest {
                id,
                epoch: self.epoch,
                node,
                pc: job.pc,
                at_us: self.now_us,
                operation,
            };
            if self.outgoing.len() >= 4096 {
                self.outgoing.pop_front();
                self.notices
                    .insert("CAN geçmiş TX görünümü4096olayla sınırlı".into());
            }
            self.outgoing.push_back(request.clone());
            job.pending = Some(request);
            self.active = Some(job);
        } else {
            match job.kind {
                JobKind::Init(ids) => {
                    for id in ids {
                        if let Some(n) = self.nodes.get_mut(&id) {
                            n.initialized = true;
                            n.previously_worked = true;
                        }
                    }
                    memory.set_system(1615, true);
                }
                JobKind::Enable => {}
                JobKind::Reset {
                    ok,
                    suppressed,
                    restore,
                    ..
                } => {
                    for id in restore {
                        if let Some(n) = self.nodes.get_mut(&id) {
                            n.initialized = true;
                        }
                    }
                    if !suppressed {
                        memory.set_internal(ok, true);
                    }
                }
            }
            self.completed.insert(job.pc);
        }
    }
    fn receive(&mut self, memory: &mut Memory, id: u64, epoch: u64, reply: CanReply) {
        let Some(mut job) = self.active.take() else {
            self.notices.insert("Owner dışı CAN yanıtı atıldı".into());
            return;
        };
        let Some(request) = job.pending.take() else {
            self.active = Some(job);
            return;
        };
        if request.id != id || request.epoch != epoch {
            job.pending = Some(request);
            self.active = Some(job);
            self.notices
                .insert("Eski/başka CAN işlem yanıtı atıldı".into());
            return;
        }
        match reply {
            CanReply::Fault { code } => {
                memory.set_system(1616, true);
                memory.set_word(d(6000), i16::from(request.node));
                if let Some(code) = code {
                    memory.set_word(d(6001), code as i16);
                } else {
                    memory
                        .execution_limitations
                        .insert("CAN peer hata kodu sağlanmadı; D6001 için kod uydurulmadı".into());
                }
                memory.set_word(d(6002), job.pc as i16);
                if let JobKind::Reset { error, .. } = job.kind {
                    memory.set_internal(error, true);
                }
                if let Some(node) = self.nodes.get_mut(&request.node) {
                    node.fault = code;
                }
                self.completed.insert(job.pc);
            }
            CanReply::Ack => {
                let Some(node) = self.nodes.get_mut(&request.node) else {
                    return;
                };
                if !node.online {
                    memory
                        .execution_limitations
                        .insert("Çevrimdışı CAN node yanıtı uygulanmadı".into());
                    return;
                }
                match request.operation {
                    CanOperation::ParameterWrite { parameter, value } => {
                        if parameter == "P4-07" && value == 0 {
                            node.servo_enabled = false;
                            memory.set_system(1639 + u16::from(request.node), false);
                        }
                        node.parameters.insert(parameter, value);
                    }
                    CanOperation::ObjectWrite { index, value } => {
                        node.objects.insert(index, value);
                    }
                    CanOperation::PdoMap { number, entries } => {
                        node.pdo_maps.insert(number, entries);
                    }
                    CanOperation::ServoEnable(enabled) => {
                        node.servo_enabled = enabled;
                        memory.set_system(1639 + u16::from(request.node), enabled);
                    }
                    CanOperation::ResetDelta => {
                        node.fault = None;
                        node.initialized = false;
                        node.servo_enabled = false;
                        memory.set_system(1639 + u16::from(request.node), false);
                    }
                    CanOperation::Nmt(code) => {
                        node.nmt_state = Some(match code {
                            1 => CanNmtState::Operational,
                            2 => CanNmtState::Stopped,
                            0x80 => CanNmtState::PreOperational,
                            0x81 => CanNmtState::ResetApplicationPending,
                            0x82 => CanNmtState::ResetCommunicationPending,
                            _ => unreachable!("validated NMT service code"),
                        });
                        if matches!(code, 2 | 0x81 | 0x82) {
                            node.servo_enabled = false;
                            if request.node <= 8 {
                                memory.set_system(1639 + u16::from(request.node), false);
                            }
                        }
                    }
                }
                self.active = Some(job);
                self.send_next(memory);
            }
        }
    }
}
#[cfg(test)]
#[path = "canopen_tests.rs"]
mod tests;
