//! SE2 user-defined sockets. Explicit packet events; no OS socket or fake ACK.
use crate::calendar_operand::CalendarValue;
use crate::ethernet_raw::Endpoint;
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn span(m: &Memory, base: Device, words: u16) -> bool {
    base.area == Area::D
        && u32::from(base.index) + u32::from(words.saturating_sub(1))
            <= u32::from(m.data_register_limit.unwrap_or(9999))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EthernetConfigKind {
    Configure,
    Message,
}
#[derive(Debug, Clone, PartialEq)]
pub enum EthernetConfigOperand {
    Value(CalendarValue),
    Word(Device),
    Bit(Device),
}
#[derive(Debug, Clone, PartialEq)]
pub struct EthernetConfigAction {
    pub kind: EthernetConfigKind,
    pub args: Vec<EthernetConfigOperand>,
}
impl EthernetConfigAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let (kind, spec) = match name {
            "ETHCONF" => (EthernetConfigKind::Configure, "vwvvbbbvvbbw"),
            "ETHMSG" => (EthernetConfigKind::Message, "vbwvbb wbbwwbw"),
            _ => return Err("ETHCONF/ETHMSG temel yazımı gerekli; P/D biçimi yok".into()),
        };
        let specs: Vec<_> = spec.chars().filter(|c| !c.is_whitespace()).collect();
        if args.len() != specs.len() {
            return Err(format!("{name} {} operand gerektirir", specs.len()));
        }
        let mut parsed = Vec::new();
        for (index, (raw, kind)) in args.iter().zip(specs).enumerate() {
            parsed.push(match kind {
                'v' => {
                    let value = CalendarValue::parse(raw, false)?;
                    if !raw.starts_with(['K', 'H', 'D'])
                        || value.base().is_some_and(|x| x.area != Area::D)
                    {
                        return Err("Ethernet sayısal matrisi yalnız K/H/D".into());
                    }
                    EthernetConfigOperand::Value(value)
                }
                'w' => {
                    let v: Device = raw.parse()?;
                    if v.area != Area::D {
                        return Err("Ethernet word operandı doğrudan D olmalı".into());
                    }
                    EthernetConfigOperand::Word(v)
                }
                'b' => {
                    let v: Device = raw.parse()?;
                    let output = if name == "ETHCONF" {
                        matches!(index, 9 | 10)
                    } else {
                        matches!(index, 7 | 8 | 11)
                    };
                    if v.area != Area::M || output && !v.writable_bit() {
                        return Err("Ethernet BOOL kaynağı M, çıktı yazılabilir M olmalı".into());
                    }
                    EthernetConfigOperand::Bit(v)
                }
                _ => unreachable!(),
            });
        }
        Ok(Self { kind, args: parsed })
    }
    pub fn name(&self) -> &'static str {
        match self.kind {
            EthernetConfigKind::Configure => "ETHCONF",
            EthernetConfigKind::Message => "ETHMSG",
        }
    }
    pub fn header(&self) -> Option<u16> {
        Some(match self.kind {
            EthernetConfigKind::Configure => 0xe600,
            EthernetConfigKind::Message => 0x1d02,
        })
    }
    pub fn words(&self) -> u16 {
        if self.kind == EthernetConfigKind::Configure {
            25
        } else {
            27
        }
    }
    pub fn operands(&self) -> Vec<EthernetConfigOperand> {
        self.args.clone()
    }
    pub fn dest(&self) -> Device {
        self.device(if self.kind == EthernetConfigKind::Configure {
            11
        } else {
            9
        })
    }
    pub fn label(&self) -> String {
        format!(
            "{} {}",
            self.name(),
            self.args
                .iter()
                .map(|v| match v {
                    EthernetConfigOperand::Value(v) => v.to_string(),
                    EthernetConfigOperand::Word(v) | EthernetConfigOperand::Bit(v) => v.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
    pub fn display_value(&self, m: &Memory) -> String {
        format!("{}={}", self.dest(), m.word(self.dest()))
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for v in &self.args {
            match v {
                EthernetConfigOperand::Value(v) => v.devices(out),
                EthernetConfigOperand::Word(v) | EthernetConfigOperand::Bit(v) => {
                    out.insert(*v);
                }
            }
        }
        if self.kind == EthernetConfigKind::Configure && self.device(1).index < 9999 {
            out.insert(d(self.device(1).index + 1));
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if self.kind == EthernetConfigKind::Message {
            let tx = self.number(m, 3).clamp(1, 4096).div_ceil(2);
            let rx = m
                .ethernet_config
                .socket(self.number(m, 0) as u8)
                .map_or(0, |s| s.config.rx_max_len.div_ceil(2));
            for (base, count) in [(self.device(2), tx), (self.device(9), rx)] {
                for i in 0..count {
                    if u32::from(base.index) + u32::from(i)
                        <= u32::from(m.data_register_limit.unwrap_or(9999))
                    {
                        out.insert(d(base.index + i));
                    }
                }
            }
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        m.ethernet_config.externally_reserved_ports = m.ethernet_raw.reserved_ports();
        let mut s = std::mem::take(&mut m.ethernet_config);
        s.visit(m, self, power, pc);
        m.ethernet_config = s;
    }
    fn device(&self, i: usize) -> Device {
        match self.args[i] {
            EthernetConfigOperand::Word(v) | EthernetConfigOperand::Bit(v) => v,
            _ => unreachable!(),
        }
    }
    fn number(&self, m: &Memory, i: usize) -> u16 {
        match &self.args[i] {
            EthernetConfigOperand::Value(v) => v.read(m) as u16,
            _ => unreachable!(),
        }
    }
    fn bit(&self, m: &Memory, i: usize) -> bool {
        m.bit(self.device(i))
    }
    fn validate(&self, m: &Memory) -> Result<(), String> {
        let mut out = BTreeSet::new();
        self.devices(&mut out);
        for v in out {
            if v.area == Area::D && !span(m, v, 1)
                || v.area == Area::M && v.index > m.relay_limit.unwrap_or(4095)
            {
                return Err("Ethernet operand aralığı model belleğini aşıyor".into());
            }
        }
        if self.kind == EthernetConfigKind::Configure && !span(m, self.device(1), 2) {
            return Err("ETHCONF IP tam D çifti gerektirir".into());
        }
        Ok(())
    }
    fn error(&self, m: &mut Memory, code: u16) {
        let i = self.args.len() - 2;
        m.set_internal(self.device(i), code != 0);
        m.set_word(self.device(i + 1), code as i16);
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EthernetConfigProfile {
    #[default]
    Disabled,
    Se2,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UdpClientPortPolicy {
    #[default]
    TableZeroOnly,
    ExampleFixedPort,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthernetSocketConfig {
    pub tcp: bool,
    pub server: bool,
    pub remote: Endpoint,
    pub local_port: u16,
    pub rx_max_len: u16,
    pub keep_alive: bool,
    pub keep_alive_seconds: u16,
}
#[derive(Debug, Clone)]
pub enum ConfigRequestKind {
    Open {
        config: EthernetSocketConfig,
    },
    Send {
        peer: Endpoint,
        bytes: Vec<u8>,
        segment_index: u16,
        segment_count: u16,
    },
    KeepAlive {
        peer: Endpoint,
    },
    Close,
}
#[derive(Debug, Clone)]
pub struct ConfigRequest {
    pub id: u64,
    pub epoch: u64,
    pub socket: u8,
    pub pc: u16,
    pub at_us: u64,
    pub kind: ConfigRequestKind,
}
#[derive(Debug, Clone)]
pub enum ConfigEvent {
    Opened { peer: Option<Endpoint> },
    Sent,
    Packet { peer: Endpoint, bytes: Vec<u8> },
    Closed,
    Fault(u16),
    KeepAliveAck,
}
#[derive(Debug, Clone)]
struct Binding {
    action: EthernetConfigAction,
    pc: u16,
    last_send: bool,
    last_clear: bool,
}
#[derive(Debug, Clone)]
struct Sending {
    request: u64,
    peer: Endpoint,
    segments: VecDeque<Vec<u8>>,
    total: u16,
    next_index: u16,
}
#[derive(Debug, Clone)]
pub struct EthernetSocketState {
    pub config: EthernetSocketConfig,
    pub status: u16,
    pub peer: Option<Endpoint>,
    pub rx_len: u16,
    binding: Option<Binding>,
    connect: bool,
    bound: bool,
    open_id: Option<u64>,
    sending: Option<Sending>,
    tx_done: bool,
    rx_done: bool,
    error: u16,
    has_sent: bool,
    last_rx: u64,
    next_keep_alive: Option<u64>,
    retry: Option<u64>,
    keepalive_ids: BTreeSet<u64>,
}
impl EthernetSocketState {
    fn new(config: EthernetSocketConfig) -> Self {
        Self {
            config,
            status: 0,
            peer: None,
            rx_len: 0,
            binding: None,
            connect: false,
            bound: false,
            open_id: None,
            sending: None,
            tx_done: false,
            rx_done: false,
            error: 0,
            has_sent: false,
            last_rx: 0,
            next_keep_alive: None,
            retry: None,
            keepalive_ids: BTreeSet::new(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct EthernetConfigState {
    pub profile: EthernetConfigProfile,
    pub link_up: bool,
    pub now_us: u64,
    pub notices: BTreeSet<String>,
    pub udp_client_port_policy: UdpClientPortPolicy,
    /// Root's common Ethernet resource broker may publish occupied (TCP,port) pairs.
    pub externally_reserved_ports: BTreeSet<(bool, u16)>,
    sockets: BTreeMap<u8, EthernetSocketState>,
    config_edges: BTreeMap<u16, bool>,
    events: BTreeMap<(u64, u64), (u64, u64, ConfigEvent)>,
    requests: VecDeque<ConfigRequest>,
    sequence: u64,
    next_id: u64,
    epoch: u64,
}
impl Default for EthernetConfigState {
    fn default() -> Self {
        Self {
            profile: EthernetConfigProfile::Disabled,
            link_up: true,
            now_us: 0,
            notices: BTreeSet::new(),
            udp_client_port_policy: Default::default(),
            externally_reserved_ports: BTreeSet::new(),
            sockets: BTreeMap::new(),
            config_edges: BTreeMap::new(),
            events: BTreeMap::new(),
            requests: VecDeque::new(),
            sequence: 0,
            next_id: 0,
            epoch: 0,
        }
    }
}
impl EthernetConfigState {
    pub fn socket(&self, id: u8) -> Option<&EthernetSocketState> {
        self.sockets.get(&id)
    }
    pub fn outgoing(&self) -> &VecDeque<ConfigRequest> {
        &self.requests
    }
    pub fn pop_request(&mut self) -> Option<ConfigRequest> {
        self.requests.pop_front()
    }
    pub fn queue_event(
        &mut self,
        at_us: u64,
        id: u64,
        epoch: u64,
        event: ConfigEvent,
    ) -> Result<(), String> {
        if at_us < self.now_us {
            return Err("Ethernet olayı geçmişe eklenemez".into());
        }
        self.sequence = self.sequence.wrapping_add(1);
        self.events
            .insert((at_us, self.sequence), (id, epoch, event));
        Ok(())
    }
    pub fn reset_instruction_state(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.events.clear();
        self.requests.clear();
        self.config_edges.clear();
        self.now_us = 0;
        for s in self.sockets.values_mut() {
            *s = EthernetSocketState::new(s.config.clone());
        }
    }
    pub fn abort(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.ethernet_config);
        for id in s.sockets.keys().copied().collect::<Vec<_>>() {
            let mut socket = s.sockets.remove(&id).unwrap();
            s.close(id, &mut socket, false);
            socket.connect = false;
            s.publish_socket(m, &socket);
            socket.binding = None;
            s.sockets.insert(id, socket);
        }
        s.events.clear();
        s.epoch = s.epoch.wrapping_add(1);
        m.ethernet_config = s;
    }
    pub fn begin_scan(_m: &mut Memory) {}
    pub fn end_scan(_m: &mut Memory) {}
    pub fn service_controls(m: &mut Memory) {
        m.ethernet_config.externally_reserved_ports = m.ethernet_raw.reserved_ports();
        let mut s = std::mem::take(&mut m.ethernet_config);
        s.controls(m);
        s.publish(m);
        m.ethernet_config = s;
    }
    fn request(&mut self, id: u8, pc: u16, kind: ConfigRequestKind) -> u64 {
        self.next_id = self.next_id.wrapping_add(1);
        let request = self.next_id;
        self.requests.push_back(ConfigRequest {
            id: request,
            epoch: self.epoch,
            socket: id,
            pc,
            at_us: self.now_us,
            kind,
        });
        request
    }
    fn close(&mut self, id: u8, s: &mut EthernetSocketState, retry: bool) {
        if s.open_id.is_some() {
            self.request(
                id,
                s.binding.as_ref().map_or(0, |b| b.pc),
                ConfigRequestKind::Close,
            );
        }
        s.open_id = None;
        s.bound = false;
        s.sending = None;
        s.status = 0;
        s.peer = None;
        s.next_keep_alive = None;
        s.keepalive_ids.clear();
        s.retry = if retry && s.connect && s.config.tcp && !s.config.server {
            Some(self.now_us.saturating_add(5_000_000))
        } else {
            None
        };
        if retry && s.connect && s.config.tcp && s.config.server {
            self.open(id, s);
        }
    }
    fn open(&mut self, id: u8, s: &mut EthernetSocketState) {
        let pc = s.binding.as_ref().map_or(0, |b| b.pc);
        // A failed attempt must retain a future deadline while Connect stays ON.
        // Successful opens below consume it; cable outages must not require a new edge.
        s.retry = (s.connect && s.config.tcp && !s.config.server)
            .then(|| self.now_us.saturating_add(5_000_000));
        if !self.link_up {
            s.error = 10;
            return;
        }
        let conflict = s.config.local_port != 0
            && (self
                .externally_reserved_ports
                .contains(&(s.config.tcp, s.config.local_port))
                || self.sockets.values().any(|other| {
                    other.open_id.is_some()
                        && other.config.tcp == s.config.tcp
                        && other.config.local_port == s.config.local_port
                }));
        if conflict {
            s.error = 16;
            return;
        }
        let request = self.request(
            id,
            pc,
            ConfigRequestKind::Open {
                config: s.config.clone(),
            },
        );
        s.open_id = Some(request);
        s.status = 1;
        s.retry = None;
        s.last_rx = self.now_us;
        // An automatic reopen must not erase an error before it is published.
        // The explicit Connect edge already clears outputs in visit().
    }
    fn visit(&mut self, m: &mut Memory, a: &EthernetConfigAction, power: bool, pc: u16) {
        self.controls(m);
        if a.kind == EthernetConfigKind::Configure {
            let previous = self.config_edges.insert(pc, power).unwrap_or(false);
            if !power || previous {
                return;
            }
            if let Err(e) = a.validate(m) {
                m.execution_limitations.insert(e);
                return;
            }
            if self.profile != EthernetConfigProfile::Se2 {
                m.execution_limitations
                    .insert("ETHCONF açık SE2 Ethernet profili gerektirir".into());
                return;
            }
            self.configure(m, a);
            self.publish(m);
            return;
        }
        if !power {
            let ids: Vec<_> = self
                .sockets
                .iter()
                .filter_map(|(&id, s)| s.binding.as_ref().is_some_and(|b| b.pc == pc).then_some(id))
                .collect();
            for id in ids {
                let mut s = self.sockets.remove(&id).unwrap();
                self.close(id, &mut s, false);
                s.connect = false;
                s.binding = None;
                self.sockets.insert(id, s);
            }
            return;
        }
        if let Err(e) = a.validate(m) {
            m.execution_limitations.insert(e);
            return;
        }
        if self.profile != EthernetConfigProfile::Se2 {
            m.execution_limitations
                .insert("ETHMSG açık SE2 Ethernet profili gerektirir".into());
            return;
        }
        let raw = a.number(m, 0);
        if !(1..=4).contains(&raw) {
            a.error(m, 1);
            return;
        }
        let id = raw as u8;
        let Some(mut s) = self.sockets.remove(&id) else {
            a.error(m, 8);
            return;
        };
        if s.binding.as_ref().is_some_and(|b| b.pc != pc) {
            a.error(m, 16);
            self.sockets.insert(id, s);
            return;
        }
        let connect = a.bit(m, 1);
        let send = a.bit(m, 4);
        let clear = a.bit(m, 5);
        let old_connect = s.connect;
        let (old_send, old_clear) = s
            .binding
            .as_ref()
            .map_or((false, false), |b| (b.last_send, b.last_clear));
        // Full receive capacity is checked before clearing any aliased output.
        if !span(m, a.device(9), s.config.rx_max_len.div_ceil(2).max(1)) {
            m.execution_limitations
                .insert("ETHMSG tam receive alanı model belleğini aşıyor".into());
            self.sockets.insert(id, s);
            return;
        }
        let tx = if send && !old_send {
            let len = a.number(m, 3).clamp(1, 4096);
            if !span(m, a.device(2), len.div_ceil(2)) {
                None
            } else {
                Some(
                    (0..len)
                        .map(|i| {
                            let word = m.word(d(a.device(2).index + i / 2)) as u16;
                            (word >> (8 * (i % 2))) as u8
                        })
                        .collect::<Vec<_>>(),
                )
            }
        } else {
            Some(Vec::new())
        };
        s.binding = Some(Binding {
            action: a.clone(),
            pc,
            last_send: send,
            last_clear: clear,
        });
        s.connect = connect;
        if connect && !old_connect {
            s.rx_len = 0;
            s.rx_done = false;
            s.tx_done = false;
            s.error = 0;
            s.has_sent = false;
            self.open(id, &mut s);
        }
        if !connect && old_connect {
            self.close(id, &mut s, false);
        }
        if clear && !old_clear {
            s.rx_len = 0;
            s.rx_done = false;
        }
        if !send {
            s.tx_done = false;
        }
        if connect && send && !old_send {
            match tx {
                None => s.error = 13,
                Some(bytes) => {
                    if (!s.config.tcp
                        && s.bound
                        && s.status == 1
                        && (s.config.server && s.peer.is_some() || !s.config.server))
                        || (s.config.tcp && s.status == 2)
                    {
                        if s.sending.is_some() {
                            s.error = 11;
                        } else {
                            let peer = s.peer.unwrap_or(s.config.remote);
                            let segments: VecDeque<_> =
                                bytes.chunks(1460).map(<[u8]>::to_vec).collect();
                            let total = segments.len() as u16;
                            s.sending = Some(Sending {
                                request: 0,
                                peer,
                                segments,
                                total,
                                next_index: 0,
                            });
                            s.tx_done = false;
                            s.error = 0;
                            self.send_next(id, &mut s);
                        }
                    } else {
                        s.error = 11;
                    }
                }
            }
        }
        self.publish_socket(m, &s);
        self.sockets.insert(id, s);
    }
    fn configure(&mut self, m: &mut Memory, a: &EthernetConfigAction) {
        let id = a.number(m, 0);
        let ip0 = m.word(a.device(1)) as u16;
        let ip1 = m.word(d(a.device(1).index + 1)) as u16;
        let ip = [(ip0 >> 8) as u8, ip0 as u8, (ip1 >> 8) as u8, ip1 as u8];
        let remote = a.number(m, 2);
        let local = a.number(m, 3);
        let tcp = a.bit(m, 4);
        let server = a.bit(m, 5);
        let rx = a.number(m, 7);
        let time = a.number(m, 8);
        let error = if !(1..=4).contains(&id) {
            1
        } else if server && ip != [0; 4] || !server && (ip == [0; 4] || ip == [255; 4]) {
            2
        } else if server && remote != 0 || !server && remote == 0 {
            3
        } else if server && local == 0
            || !server
                && local != 0
                && (tcp || self.udp_client_port_policy == UdpClientPortPolicy::TableZeroOnly)
            || reserved(tcp, local)
        {
            4
        } else if rx > 4096 {
            6
        } else if tcp && !(5..=60000).contains(&time) {
            7
        } else {
            0
        };
        if error != 0 {
            m.set_internal(a.device(9), false);
            a.error(m, error);
            return;
        }
        let cfg = EthernetSocketConfig {
            tcp,
            server,
            remote: Endpoint { ip, port: remote },
            local_port: local,
            rx_max_len: rx,
            keep_alive: a.bit(m, 6),
            keep_alive_seconds: time,
        };
        let id = id as u8;
        if let Some(mut old) = self.sockets.remove(&id) {
            self.close(id, &mut old, false);
            if let Some(b) = old.binding {
                m.set_word(b.action.device(6), 0);
            }
        }
        self.sockets.insert(id, EthernetSocketState::new(cfg));
        m.set_internal(a.device(9), true);
        a.error(m, 0);
    }
    fn send_next(&mut self, id: u8, s: &mut EthernetSocketState) {
        if let Some(mut send) = s.sending.take() {
            if let Some(bytes) = send.segments.pop_front() {
                send.next_index += 1;
                send.request = self.request(
                    id,
                    s.binding.as_ref().unwrap().pc,
                    ConfigRequestKind::Send {
                        peer: send.peer,
                        bytes,
                        segment_index: send.next_index,
                        segment_count: send.total,
                    },
                );
                s.sending = Some(send);
            } else {
                s.tx_done = true;
                s.has_sent = true;
            }
        }
    }
    fn controls(&mut self, m: &Memory) {
        let ids: Vec<_> = self.sockets.keys().copied().collect();
        for id in ids {
            let mut s = self.sockets.remove(&id).unwrap();
            let connect = s.binding.as_ref().is_some_and(|b| b.action.bit(m, 1));
            if s.connect && !connect {
                self.close(id, &mut s, false);
                s.connect = false;
            } else if !self.link_up {
                s.error = 10;
                if s.open_id.is_some() {
                    // Retire this connection once. Subsequent scans must not move
                    // the client's existing five-second retry deadline forward.
                    self.close(id, &mut s, true);
                }
            } else if s.connect && s.config.tcp && s.config.server && s.open_id.is_none() {
                self.open(id, &mut s);
            }
            self.sockets.insert(id, s);
        }
    }
    fn publish_socket(&self, m: &mut Memory, s: &EthernetSocketState) {
        if let Some(b) = &s.binding {
            let a = &b.action;
            m.set_word(a.device(6), s.status as i16);
            m.set_internal(a.device(7), s.tx_done);
            m.set_internal(a.device(8), s.rx_done);
            m.set_word(a.device(10), s.rx_len as i16);
            a.error(m, s.error);
        }
    }
    fn publish(&self, m: &mut Memory) {
        for s in self.sockets.values() {
            self.publish_socket(m, s);
        }
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.events
            .first_key_value()
            .map(|(k, _)| k.0)
            .into_iter()
            .chain(self.sockets.values().flat_map(|s| {
                s.retry.into_iter().chain(s.next_keep_alive).chain(
                    (s.config.tcp && s.status == 2).then_some(
                        s.last_rx
                            .saturating_add(u64::from(s.config.keep_alive_seconds) * 1_000_000),
                    ),
                )
            }))
            .min()
    }
    pub fn advance(m: &mut Memory, at_us: u64) {
        m.ethernet_config.externally_reserved_ports = m.ethernet_raw.reserved_ports();
        let mut s = std::mem::take(&mut m.ethernet_config);
        if at_us < s.now_us {
            m.execution_limitations
                .insert("Ethernet saati geriye gidemez".into());
            m.ethernet_config = s;
            return;
        }
        s.controls(m);
        let mut iterations = 0;
        loop {
            let next = s
                .next_deadline_us()
                .filter(|t| *t <= at_us)
                .unwrap_or(at_us);
            s.now_us = next;
            let keys: Vec<_> = s
                .events
                .range(..=(next, u64::MAX))
                .map(|(k, _)| *k)
                .collect();
            for k in keys {
                s.controls(m);
                let (id, epoch, event) = s.events.remove(&k).unwrap();
                if epoch == s.epoch {
                    s.event(m, id, event);
                }
            }
            let ids: Vec<_> = s.sockets.keys().copied().collect();
            for id in ids {
                let mut socket = s.sockets.remove(&id).unwrap();
                if socket.config.tcp && socket.status == 2 {
                    let timeout = socket
                        .last_rx
                        .saturating_add(u64::from(socket.config.keep_alive_seconds) * 1_000_000);
                    if timeout <= next {
                        s.close(id, &mut socket, true);
                    } else if socket.next_keep_alive.is_some_and(|t| t <= next) {
                        if let Some(peer) = socket.peer {
                            let req = s.request(
                                id,
                                socket.binding.as_ref().map_or(0, |b| b.pc),
                                ConfigRequestKind::KeepAlive { peer },
                            );
                            socket.keepalive_ids.insert(req);
                        }
                        socket.next_keep_alive =
                            Some(next.saturating_add(
                                u64::from(socket.config.keep_alive_seconds) * 500_000,
                            ));
                    }
                }
                if socket.retry.is_some_and(|t| t <= next) && socket.connect {
                    s.open(id, &mut socket);
                }
                s.sockets.insert(id, socket);
            }
            s.publish(m);
            if next == at_us {
                break;
            }
            iterations += 1;
            if iterations > 65536 {
                m.execution_limitations
                    .insert("Ethernet olay bütçesi doldu".into());
                break;
            }
        }
        m.ethernet_config = s;
    }
    fn event(&mut self, m: &mut Memory, request: u64, event: ConfigEvent) {
        let Some(id) = self.sockets.iter().find_map(|(&id, s)| {
            (s.open_id == Some(request)
                || s.sending.as_ref().is_some_and(|tx| tx.request == request)
                || s.keepalive_ids.contains(&request))
            .then_some(id)
        }) else {
            return;
        };
        let mut s = self.sockets.remove(&id).unwrap();
        match event {
            ConfigEvent::Opened { peer } if s.open_id == Some(request) && s.status == 1 => {
                if s.config.tcp {
                    let peer = peer.or((!s.config.server).then_some(s.config.remote));
                    if let Some(peer) = peer
                        && peer.port != 0
                        && peer.ip != [0; 4]
                        && (s.config.server || peer == s.config.remote)
                    {
                        s.peer = Some(peer);
                        s.status = 2;
                        s.last_rx = self.now_us;
                        s.next_keep_alive = s.config.keep_alive.then_some(
                            self.now_us
                                .saturating_add(u64::from(s.config.keep_alive_seconds) * 500_000),
                        );
                    } else {
                        s.error = 12;
                        self.close(id, &mut s, true);
                    }
                } else {
                    s.status = 1;
                    s.bound = true;
                }
            }
            ConfigEvent::Sent if s.sending.as_ref().is_some_and(|tx| tx.request == request) => {
                if s.config.tcp {
                    s.last_rx = self.now_us;
                }
                self.send_next(id, &mut s);
            }
            ConfigEvent::Packet { peer, bytes } if s.open_id == Some(request) => {
                let allowed = if s.config.tcp {
                    s.status == 2 && s.peer == Some(peer)
                } else {
                    s.bound
                        && s.status == 1
                        && (s.config.server || s.has_sent && peer == s.config.remote)
                };
                if allowed {
                    s.last_rx = self.now_us;
                    if !s.config.tcp && s.config.server {
                        s.peer = Some(peer);
                    }
                    if s.config.rx_max_len != 0 {
                        let len = usize::from(s.rx_len) + bytes.len();
                        if len > usize::from(s.config.rx_max_len) {
                            s.error = 14;
                        } else if let Some(b) = &s.binding {
                            let base = b.action.device(9);
                            if span(m, base, (len as u16).div_ceil(2).max(1)) {
                                // Build all words before mutation: odd appends preserve the untouched byte.
                                let mut changed = BTreeMap::new();
                                for (i, byte) in bytes.iter().copied().enumerate() {
                                    let offset = usize::from(s.rx_len) + i;
                                    let device = d(base.index + (offset / 2) as u16);
                                    let word = changed
                                        .entry(device)
                                        .or_insert_with(|| m.word(device) as u16);
                                    let shift = (offset % 2) * 8;
                                    *word = (*word & !(255 << shift)) | (u16::from(byte) << shift);
                                }
                                for (device, word) in changed {
                                    m.set_word(device, word as i16);
                                }
                                s.rx_len = len as u16;
                                s.rx_done = true;
                            } else {
                                m.execution_limitations.insert(
                                    "ETHMSG receive alanı çalışma sırasında model dışına çıktı"
                                        .into(),
                                );
                            }
                        }
                    }
                }
            }
            ConfigEvent::Closed if s.open_id == Some(request) => {
                if s.sending.is_some() {
                    s.error = 15;
                }
                self.close(id, &mut s, true);
            }
            ConfigEvent::Fault(code) if [1, 8, 10, 11, 12, 13, 14, 15, 16].contains(&code) => {
                s.error = code;
                self.close(id, &mut s, true);
            }
            ConfigEvent::KeepAliveAck if s.keepalive_ids.remove(&request) => {
                s.last_rx = self.now_us;
            }
            _ => {}
        }
        self.sockets.insert(id, s);
    }
}
fn reserved(tcp: bool, port: u16) -> bool {
    if tcp {
        [502, 44818].contains(&port)
    } else {
        [67, 68, 2222, 20006, 20008].contains(&port)
    }
}
#[cfg(test)]
#[path = "ethernet_config_tests.rs"]
mod tests;
