//! ETHRW virtual Modbus/TCP transport. No socket or automatic remote peer.
//! Delta PDF490..492; MBAP follows Modbus Messaging Implementation Guide1.0b.
use crate::calendar_operand::CalendarValue;
use crate::file_memory::FirmwareVersion;
use crate::plc::{Area, Device, Memory};
use crate::serial_io::SerialOperand;
use crate::serial_protocol::{Frame, Reply, validate_response};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn span(m: &Memory, base: Device, count: u16) -> Result<(), String> {
    if base.area != Area::D
        || count == 0
        || u32::from(base.index) + u32::from(count) - 1
            > u32::from(m.data_register_limit.unwrap_or(9999))
    {
        Err("ETHRW D tamponu model sınırını aşıyor".into())
    } else {
        Ok(())
    }
}
fn scalar(m: &Memory, value: &CalendarValue) -> Result<i16, String> {
    if let Some(base) = value.base() {
        span(m, base, 1)?;
    }
    Ok(value.read(m) as i16)
}
fn flag(m: &mut Memory, register: u16, port: u16, on: bool) {
    let mask = 1u16 << (port - 100);
    let old = m.word(d(register)) as u16;
    m.set_word(
        d(register),
        if on { old | mask } else { old & !mask } as i16,
    );
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EthernetModel {
    #[default]
    Disabled,
    Es2E,
    Se,
    Se2,
    Sa2,
    Sx2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BitPacking {
    #[default]
    Unspecified,
    LowBitFirst,
    SwapBytesCandidate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompletionRegister {
    #[default]
    Explanation1395,
    Example1394,
}
impl CompletionRegister {
    fn index(self) -> u16 {
        if self == Self::Explanation1395 {
            1395
        } else {
            1394
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct EthernetProfile {
    pub model: EthernetModel,
    pub firmware: FirmwareVersion,
    pub bit_packing: BitPacking,
    pub completion_register: CompletionRegister,
}
impl Default for EthernetProfile {
    fn default() -> Self {
        Self {
            model: EthernetModel::Disabled,
            firmware: FirmwareVersion::new(1, 0),
            bit_packing: BitPacking::Unspecified,
            completion_register: CompletionRegister::Explanation1395,
        }
    }
}
impl EthernetProfile {
    fn validate(self, port: u16, function: u8) -> Result<(), String> {
        let v = self.firmware;
        if v.minor > 99 || self.model == EthernetModel::Disabled {
            return Err("ETHRW açık CPU/Ethernet profili gerektirir".into());
        }
        if !(100..=108).contains(&port)
            || port == 108 && matches!(self.model, EthernetModel::Sa2 | EthernetModel::Sx2)
        {
            return Err("ETHRW seçili modelde bu Ethernet portu yok".into());
        }
        let bits = matches!(function, 1 | 2 | 5 | 15);
        let minimum = match self.model {
            EthernetModel::Sa2 | EthernetModel::Sx2 if bits => {
                return Err("SA2/SX2 ETHRW bit işlevleri belgelenmedi".into());
            }
            EthernetModel::Sa2 | EthernetModel::Sx2 => FirmwareVersion::new(2, 62),
            EthernetModel::Se if bits => FirmwareVersion::new(1, 86),
            _ => FirmwareVersion::new(1, 0),
        };
        if v < minimum {
            return Err("ETHRW işlevi seçili firmware sürümünde yok".into());
        }
        Ok(())
    }
    fn custom_tcp_port(self) -> bool {
        match self.model {
            EthernetModel::Es2E => self.firmware >= FirmwareVersion::new(1, 46),
            EthernetModel::Se => self.firmware >= FirmwareVersion::new(2, 4),
            EthernetModel::Se2 => self.firmware >= FirmwareVersion::new(1, 0),
            _ => false,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct EthernetAction {
    pub connection: Device,
    pub address: CalendarValue,
    pub data: Device,
    pub count: CalendarValue,
}
impl EthernetAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if name != "ETHRW" || args.len() != 4 {
            return Err("ETHRW D K/H/D D K/H/D gerekli; P/D biçimi yok".into());
        }
        let device = |n: usize| -> Result<Device, String> {
            let v: Device = args[n].parse()?;
            if v.area != Area::D {
                return Err("ETHRW doğrudan D tamponu gerektirir".into());
            }
            Ok(v)
        };
        let value = |n: usize| -> Result<CalendarValue, String> {
            let v = CalendarValue::parse(&args[n], false)?;
            if !args[n].starts_with(['K', 'H', 'D']) || v.base().is_some_and(|d| d.area != Area::D)
            {
                return Err("ETHRW sayısal operandı K/H/D olmalı".into());
            }
            Ok(v)
        };
        Ok(Self {
            connection: device(0)?,
            address: value(1)?,
            data: device(2)?,
            count: value(3)?,
        })
    }
    pub fn name(&self) -> &'static str {
        "ETHRW"
    }
    pub fn header(&self) -> u16 {
        0xf200
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
    pub fn operands(&self) -> Vec<SerialOperand> {
        vec![
            SerialOperand::Data(self.connection),
            SerialOperand::Value(self.address.clone()),
            SerialOperand::Data(self.data),
            SerialOperand::Value(self.count.clone()),
        ]
    }
    pub fn label(&self) -> String {
        format!(
            "ETHRW {} {} {} {}",
            self.connection, self.address, self.data, self.count
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.extend((0..5).filter_map(|i| self.connection.index.checked_add(i).map(d)));
        out.insert(self.data);
        self.address.devices(out);
        self.count.devices(out);
        out.extend([1111, 1349, 1394, 1395, 1396].map(d));
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(p) = self.prepare(m, m.ethernet.profile) {
            out.extend((0..p.words).map(|i| d(self.data.index + i)));
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        format!("ETHRW: {} etkin port", m.ethernet.active.len())
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let mut s = std::mem::take(&mut m.ethernet);
        // State's clock is maintained by the shared scheduler, even while
        // TimingState itself is taken out of Memory during event dispatch.
        s.visit(m, self, power, pc);
        m.ethernet = s;
    }
    fn prepare(&self, m: &Memory, profile: EthernetProfile) -> Result<Prepared, String> {
        span(m, self.connection, 5)?;
        let c = |n| m.word(d(self.connection.index + n)) as u16;
        let low = c(0).to_be_bytes();
        let high = c(1).to_be_bytes();
        let remote = [high[0], high[1], low[0], low[1]];
        let port = c(2);
        let station = u8::try_from(c(3)).map_err(|_| "ETHRW Unit Identifier 0..255 olmalı")?;
        let function = u8::try_from(c(4)).map_err(|_| "ETHRW desteklenmeyen işlev kodu")?;
        if !matches!(function, 1 | 2 | 3 | 4 | 5 | 6 | 15 | 16) {
            return Err("ETHRW desteklenmeyen işlev kodu".into());
        }
        profile.validate(port, function)?;
        let count = scalar(m, &self.count)?.clamp(
            1,
            if matches!(function, 1 | 2 | 5 | 15) {
                256
            } else {
                96
            },
        ) as u16;
        let words = match function {
            1 | 2 | 15 => count.div_ceil(16),
            5 | 6 => 1,
            _ => count,
        };
        span(m, self.data, words)?;
        if matches!(function, 1 | 2 | 15) && profile.bit_packing == BitPacking::Unspecified {
            return Err(
                "ETHRW bit yerleşimi PDF örneğinde çelişkili; açık BitPacking seçilmeli".into(),
            );
        }
        let address = scalar(m, &self.address)? as u16;
        let addressed = if matches!(function, 5 | 6) { 1 } else { count };
        if u32::from(address) + u32::from(addressed) > 65536 {
            return Err("ETHRW uzak adres aralığı taşıyor".into());
        }
        let mut data = address.to_be_bytes().to_vec();
        match function {
            1..=4 => data.extend_from_slice(&count.to_be_bytes()),
            5 => data.extend_from_slice(
                &(if m.word(self.data) == 0 { 0u16 } else { 0xff00 }).to_be_bytes(),
            ),
            6 => data.extend_from_slice(&(m.word(self.data) as u16).to_be_bytes()),
            15 => {
                data.extend_from_slice(&count.to_be_bytes());
                data.push(count.div_ceil(8) as u8);
                let mut packed = vec![0u8; usize::from(count.div_ceil(8))];
                for i in 0..count {
                    let mut word = m.word(d(self.data.index + i / 16)) as u16;
                    if profile.bit_packing == BitPacking::SwapBytesCandidate {
                        word = word.swap_bytes();
                    }
                    if word & (1 << (i % 16)) != 0 {
                        packed[usize::from(i / 8)] |= 1 << (i % 8);
                    }
                }
                data.extend(packed);
            }
            16 => {
                data.extend_from_slice(&count.to_be_bytes());
                data.push((count * 2) as u8);
                for i in 0..count {
                    data.extend_from_slice(&(m.word(d(self.data.index + i)) as u16).to_be_bytes());
                }
            }
            _ => unreachable!(),
        }
        let tcp_port = if profile.custom_tcp_port() {
            match m.word(d(1111)) as u16 {
                0 => 502,
                v => v,
            }
        } else {
            502
        };
        // D1349 is reused by other models' motion registers. Only a stored
        // word overrides the ETHRW-specific documented reset default.
        let timeout = m.stored_word(d(1349)).filter(|v| *v > 0).unwrap_or(3000) as u64 * 1000;
        Ok(Prepared {
            port,
            remote,
            tcp_port,
            frame: Frame {
                station,
                function,
                data,
            },
            words,
            timeout,
        })
    }
}
struct Prepared {
    port: u16,
    remote: [u8; 4],
    tcp_port: u16,
    frame: Frame,
    words: u16,
    timeout: u64,
}
#[derive(Debug, Clone)]
pub struct TcpRequest {
    pub port: u16,
    pub id: u64,
    pub pc: u16,
    pub remote: [u8; 4],
    pub tcp_port: u16,
    pub bytes: Vec<u8>,
    pub at_us: u64,
}
#[derive(Debug, Clone)]
struct Pending {
    pc: u16,
    id: u64,
    request: Frame,
    dest: Device,
    words: u16,
    packing: BitPacking,
    completion: u16,
    deadline: u64,
    rx: Vec<u8>,
}
#[derive(Debug, Clone, Default)]
pub struct EthernetState {
    pub profile: EthernetProfile,
    pub now_us: u64,
    pub notices: BTreeSet<String>,
    ports: BTreeSet<u16>,
    active: BTreeMap<u16, Pending>,
    outgoing: VecDeque<TcpRequest>,
    events: BTreeMap<(u64, u64), (u16, u64, Vec<u8>)>,
    sequence: u64,
    next_id: u64,
}
impl EthernetState {
    pub fn configure_port(&mut self, port: u16) -> Result<(), String> {
        self.profile.validate(port, 3)?;
        if !self.ports.insert(port) {
            return Err("Ethernet portu zaten yapılandırılmış".into());
        }
        Ok(())
    }
    pub fn outgoing(&self) -> &VecDeque<TcpRequest> {
        &self.outgoing
    }
    pub fn pop_request(&mut self) -> Option<TcpRequest> {
        self.outgoing.pop_front()
    }
    pub fn queue_received(
        &mut self,
        port: u16,
        id: u64,
        at_us: u64,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        if at_us < self.now_us
            || bytes.is_empty()
            || bytes.len() > 4096
            || !self.ports.contains(&port)
        {
            return Err("ETHRW geçersiz alım zamanı/port/veri".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Ethernet olay sayacı taştı")?;
        self.events
            .insert((at_us, self.sequence), (port, id, bytes));
        Ok(())
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.active
            .values()
            .map(|p| p.deadline)
            .chain(self.events.first_key_value().map(|(k, _)| k.0))
            .min()
    }
    pub fn reset_instruction_state(&mut self) {
        self.active.clear();
        self.events.clear();
        self.outgoing.clear();
        self.now_us = 0; /* IDs are not reused across CPU restart. */
    }
    pub fn abort(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.ethernet);
        for (port, p) in &s.active {
            flag(m, p.completion, *port, false);
            flag(m, 1396, *port, false);
        }
        s.active.clear();
        s.events.clear();
        m.ethernet = s;
    }
    fn visit(&mut self, m: &mut Memory, a: &EthernetAction, power: bool, pc: u16) {
        if !power {
            let ports: Vec<_> = self
                .active
                .iter()
                .filter_map(|(port, p)| (p.pc == pc).then_some(*port))
                .collect();
            for port in ports {
                let p = self.active.remove(&port).unwrap();
                flag(m, p.completion, port, false);
                flag(m, 1396, port, false);
            }
            return;
        }
        // A live descriptor change cannot move an in-flight instruction onto
        // another port or create two owners for the same source instruction.
        if self.active.values().any(|p| p.pc == pc) {
            return;
        }
        let p = match a.prepare(m, self.profile) {
            Ok(p) => p,
            Err(e) => {
                m.execution_limitations.insert(e);
                return;
            }
        };
        if self.active.contains_key(&p.port) {
            return;
        }
        if !self.ports.contains(&p.port) {
            m.execution_limitations
                .insert("ETHRW açık sanal Ethernet portu gerektirir".into());
            return;
        }
        let Some(id) = self.next_id.checked_add(1) else {
            m.execution_limitations
                .insert("ETHRW işlem sayacı taştı".into());
            return;
        };
        let Some(deadline) = self.now_us.checked_add(p.timeout) else {
            m.execution_limitations
                .insert("ETHRW zaman aşımı saati taştı".into());
            return;
        };
        self.next_id = id;
        let mut bytes = (id as u16).to_be_bytes().to_vec();
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(&(p.frame.data.len() as u16 + 2).to_be_bytes());
        bytes.extend([p.frame.station, p.frame.function]);
        bytes.extend_from_slice(&p.frame.data);
        if self.outgoing.len() == 256 {
            self.outgoing.pop_front();
        }
        self.outgoing.push_back(TcpRequest {
            port: p.port,
            id,
            pc,
            remote: p.remote,
            tcp_port: p.tcp_port,
            bytes,
            at_us: self.now_us,
        });
        let completion = self.profile.completion_register.index();
        flag(m, completion, p.port, false);
        flag(m, 1396, p.port, false);
        self.active.insert(
            p.port,
            Pending {
                pc,
                id,
                request: p.frame,
                dest: a.data,
                words: p.words,
                packing: self.profile.bit_packing,
                completion,
                deadline,
                rx: Vec::new(),
            },
        );
        self.notices.insert("ETHRW sanal TCP: D1395 açıklama/D1394 örnek çelişkisi ve bit yerleşimi açık profildir; fiziksel ağ bağlantısı yok".into());
    }
    pub fn advance(m: &mut Memory, at_us: u64) {
        let mut s = std::mem::take(&mut m.ethernet);
        if at_us < s.now_us {
            m.execution_limitations
                .insert("Ethernet sanal saat geriye gidemez".into());
            m.ethernet = s;
            return;
        }
        while let Some((&key, _)) = s.events.first_key_value() {
            if key.0 > at_us {
                break;
            }
            let (port, id, bytes) = s.events.remove(&key).unwrap();
            s.expire(m, key.0, false);
            if let Some(mut p) = s.active.remove(&port) {
                if p.id != id {
                    s.active.insert(port, p);
                    continue;
                }
                p.rx.extend(bytes);
                match s.receive(m, &mut p) {
                    Ok(false) => {
                        s.active.insert(port, p);
                    }
                    Ok(true) => {
                        flag(m, p.completion, port, true);
                    }
                    Err(e) => {
                        flag(m, 1396, port, true);
                        s.notices.insert(e);
                    }
                }
            }
        }
        s.expire(m, at_us, true);
        s.now_us = at_us;
        m.ethernet = s;
    }
    fn expire(&mut self, m: &mut Memory, at: u64, inclusive: bool) {
        let ports: Vec<_> = self
            .active
            .iter()
            .filter_map(|(port, p)| {
                ((p.deadline < at) || (inclusive && p.deadline == at)).then_some(*port)
            })
            .collect();
        for port in ports {
            self.active.remove(&port);
            flag(m, 1396, port, true);
            self.notices
                .insert("ETHRW zaman aşımı; hedef veri korundu".into());
        }
    }
    fn receive(&mut self, m: &mut Memory, p: &mut Pending) -> Result<bool, String> {
        loop {
            if p.rx.len() < 6 {
                return Ok(false);
            }
            let protocol = u16::from_be_bytes([p.rx[2], p.rx[3]]);
            let len = usize::from(u16::from_be_bytes([p.rx[4], p.rx[5]]));
            if protocol != 0 || !(2..=254).contains(&len) {
                return Err("ETHRW geçersiz MBAP protocol/length".into());
            }
            if p.rx.len() < len + 6 {
                return Ok(false);
            }
            let bytes: Vec<_> = p.rx.drain(..len + 6).collect();
            if u16::from_be_bytes([bytes[0], bytes[1]]) != p.id as u16 {
                self.notices
                    .insert("ETHRW eşleşmeyen transaction ID yanıtı atlandı".into());
                continue;
            }
            let mut response = Frame {
                station: bytes[6],
                function: bytes[7],
                data: bytes[8..].to_vec(),
            };
            if response.station != p.request.station {
                return Err("ETHRW Unit Identifier uyuşmuyor".into());
            }
            let mut request = p.request.clone();
            // TCP unit0 is not a serial broadcast. Preserve wire identity above
            // then reuse the existing transport-independent PDU validator.
            if request.station == 0 {
                request.station = 1;
                response.station = 1;
            }
            let reply = validate_response(&request, &response)?;
            span(m, p.dest, p.words)?;
            match reply {
                Reply::Registers(words) => {
                    for (i, v) in words.iter().enumerate() {
                        m.set_word(d(p.dest.index + i as u16), *v as i16);
                    }
                }
                Reply::Bits(bits) => {
                    let mut words = vec![0u16; usize::from(p.words)];
                    for (i, v) in bits.iter().enumerate() {
                        if *v {
                            words[i / 16] |= 1 << (i % 16);
                        }
                    }
                    for (i, mut word) in words.into_iter().enumerate() {
                        if p.packing == BitPacking::SwapBytesCandidate {
                            word = word.swap_bytes();
                        }
                        m.set_word(d(p.dest.index + i as u16), word as i16);
                    }
                }
                Reply::WriteAck => {}
                Reply::Exception(code) => return Err(format!("ETHRW Modbus exception {code:02X}")),
            }
            return Ok(true);
        }
    }
}
