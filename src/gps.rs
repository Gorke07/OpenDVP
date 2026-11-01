//! Explicit virtual COM1/NMEA receiver. PLC handshake/packing choices that the
//! manual does not settle are left explicit.
use crate::plc::{Area, Device, Memory, Operand, word_range};
use std::collections::{BTreeSet, VecDeque};

const REQUEST: u16 = 1312;
const READY: u16 = 1313;
const COMPLETED: u16 = 1314;
const RECEIVE_ERROR: u16 = 1315;
const MAX_FRAME_BYTES: usize = 82;
pub const MAX_RX_CHUNKS: usize = 4096;
pub const MAX_RX_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct GpsAction {
    pub source: Operand,
    pub dest: Device,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpsSentence {
    Gga,
    Rmc,
}
impl GpsSentence {
    pub fn identifier(self) -> &'static str {
        match self {
            Self::Gga => "GPGGA",
            Self::Rmc => "GPRMC",
        }
    }
    pub fn words(self) -> u16 {
        match self {
            Self::Gga => 16,
            Self::Rmc => 17,
        }
    }
    fn from_selector(value: i16) -> Result<Self, GpsFault> {
        match value {
            0 => Ok(Self::Gga),
            1 => Ok(Self::Rmc),
            _ => Err(GpsFault::InvalidSelector(value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialParity {
    None,
    Odd,
    Even,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Com1Profile {
    pub port: u8,
    pub baud: u32,
    pub data_bits: u8,
    pub parity: SerialParity,
    pub stop_bits: u8,
    pub sentence_frequency_hz: u8,
}
impl Default for Com1Profile {
    fn default() -> Self {
        Self {
            port: 1,
            baud: 9600,
            data_bits: 8,
            parity: SerialParity::None,
            stop_bits: 1,
            sentence_frequency_hz: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpsFault {
    BackendUnavailable,
    InvalidProfile,
    InvalidProtocol(u16),
    InvalidTimeout,
    InvalidSelector(i16),
    PortBusy(usize),
    ClockRegression,
    QueueCapacity,
    PastRxTime,
    DestinationRange,
    DestinationControlOverlap,
    Timeout,
    Checksum,
    Framing,
    FrameTooLong,
    Sentence(String),
}
impl std::fmt::Display for GpsFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackendUnavailable => write!(
                f,
                "GPS requires an explicitly configured virtual COM1 receiver"
            ),
            Self::InvalidProfile => write!(
                f,
                "GPS COM1 requires 9600/8N1, NMEA at 1 Hz and a configured calendar century"
            ),
            Self::InvalidProtocol(value) => write!(
                f,
                "GPS COM1 D1036={value:04X}; the documented profile is H81"
            ),
            Self::InvalidTimeout => write!(
                f,
                "GPS D1249 timeout must be configured; zero behavior is not established"
            ),
            Self::InvalidSelector(value) => {
                write!(f, "GPS selector {value} is not the documented K0/K1")
            }
            Self::PortBusy(owner) => {
                write!(f, "GPS COM1 is owned by instruction occurrence {owner}")
            }
            Self::ClockRegression => write!(f, "GPS virtual receive clock moved backwards"),
            Self::QueueCapacity => write!(
                f,
                "GPS virtual RX queue exceeds its {MAX_RX_CHUNKS} chunk / {MAX_RX_BYTES} byte capacity"
            ),
            Self::PastRxTime => write!(
                f,
                "GPS virtual RX event is older than the observed receive clock"
            ),
            Self::DestinationRange => write!(
                f,
                "GPS destination does not fit the configured D register model"
            ),
            Self::DestinationControlOverlap => write!(
                f,
                "GPS destination overlaps its COM1 control registers; overlap semantics are unresolved"
            ),
            Self::Timeout => write!(
                f,
                "GPS selected sentence was not received before the timeout"
            ),
            Self::Checksum => write!(f, "GPS NMEA checksum is missing, malformed or incorrect"),
            Self::Framing => write!(
                f,
                "GPS NMEA framing requires ASCII $...*HH followed by CRLF"
            ),
            Self::FrameTooLong => write!(f, "GPS NMEA sentence exceeds 82 bytes"),
            Self::Sentence(message) => write!(f, "GPS NMEA field: {message}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RxChunk {
    pub at_us: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct VirtualCom1Receiver {
    pub profile: Com1Profile,
    /// Explicit local date policy, not a host-clock or rolling-century guess.
    pub date_century: u16,
    rx: VecDeque<RxChunk>,
    rx_bytes: usize,
    not_before_us: Option<u64>,
}
impl Default for VirtualCom1Receiver {
    fn default() -> Self {
        Self {
            profile: Com1Profile::default(),
            date_century: 2000,
            rx: VecDeque::new(),
            rx_bytes: 0,
            not_before_us: None,
        }
    }
}
impl VirtualCom1Receiver {
    pub fn inject_rx(&mut self, at_us: u64, bytes: impl AsRef<[u8]>) -> Result<(), GpsFault> {
        if self.not_before_us.is_some_and(|floor| at_us < floor) {
            return Err(GpsFault::PastRxTime);
        }
        let bytes = bytes.as_ref();
        if self.rx.len() >= MAX_RX_CHUNKS
            || self
                .rx_bytes
                .checked_add(bytes.len())
                .is_none_or(|total| total > MAX_RX_BYTES)
        {
            return Err(GpsFault::QueueCapacity);
        }
        let item = RxChunk {
            at_us,
            bytes: bytes.to_vec(),
        };
        let at = self
            .rx
            .iter()
            .position(|old| old.at_us > at_us)
            .unwrap_or(self.rx.len());
        self.rx.insert(at, item);
        self.rx_bytes += bytes.len();
        Ok(())
    }
    pub fn queued_bytes(&self) -> usize {
        self.rx_bytes
    }
    pub fn queued_chunks(&self) -> usize {
        self.rx.len()
    }
    fn advance_floor(&mut self, now_us: u64) {
        self.not_before_us = Some(self.not_before_us.map_or(now_us, |old| old.max(now_us)));
    }
    fn take_due(&mut self, now_us: u64) -> Vec<RxChunk> {
        self.advance_floor(now_us);
        let mut due = Vec::new();
        while self.rx.front().is_some_and(|chunk| chunk.at_us <= now_us) {
            let chunk = self.rx.pop_front().unwrap();
            self.rx_bytes -= chunk.bytes.len();
            due.push(chunk);
        }
        due
    }
    fn discard_before(&mut self, now_us: u64) {
        self.advance_floor(now_us);
        while self.rx.front().is_some_and(|chunk| chunk.at_us < now_us) {
            self.rx_bytes -= self.rx.pop_front().unwrap().bytes.len();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpsRequest {
    pub owner: usize,
    pub sentence: GpsSentence,
    pub dest: Device,
    pub started_us: u64,
    pub deadline_us: u64,
    pub timeout_ms: u16,
    pub date_century: u16,
}
#[derive(Debug, Clone)]
struct Pending {
    request: GpsRequest,
    frame: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GpsCoordinate {
    pub raw_ascii: String,
    pub hemisphere: u8,
    pub packed_degrees_minutes: f32,
    pub decimal_degrees: f32,
}
impl GpsCoordinate {
    fn direction_word(&self) -> u16 {
        u16::from(matches!(self.hemisphere, b'S' | b'W'))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GpsPacket {
    pub sentence: GpsSentence,
    pub raw_sentence: String,
    pub raw_time: String,
    pub utc: Option<[u16; 3]>,
    pub latitude: Option<GpsCoordinate>,
    pub longitude: Option<GpsCoordinate>,
    pub validity: u16,
    pub raw_status: u8,
    pub altitude_m: Option<f32>,
    pub date: Option<[u16; 3]>,
}
impl GpsPacket {
    /// Missing fields of a checksum-valid invalid fix retain old destination
    /// values; validity=0 is still committed, so stale coordinates are not
    /// represented as a new valid fix.
    fn writes(&self) -> Vec<(u16, u16)> {
        let mut out = vec![(9, self.validity)];
        if let Some(time) = self.utc {
            out.extend(time.into_iter().enumerate().map(|(i, v)| (i as u16, v)));
        }
        if let Some(value) = &self.latitude {
            float_words(&mut out, 3, value.packed_degrees_minutes);
            out.push((5, value.direction_word()));
            float_words(
                &mut out,
                if self.sentence == GpsSentence::Gga {
                    12
                } else {
                    13
                },
                value.decimal_degrees,
            );
        }
        if let Some(value) = &self.longitude {
            float_words(&mut out, 6, value.packed_degrees_minutes);
            out.push((8, value.direction_word()));
            float_words(
                &mut out,
                if self.sentence == GpsSentence::Gga {
                    14
                } else {
                    15
                },
                value.decimal_degrees,
            );
        }
        if let Some(value) = self.altitude_m {
            float_words(&mut out, 10, value);
        }
        if let Some(date) = self.date {
            out.extend(
                date.into_iter()
                    .enumerate()
                    .map(|(i, v)| (10 + i as u16, v)),
            );
        }
        out
    }
}

#[derive(Debug, Clone, Default)]
pub struct GpsState {
    pub receiver: Option<VirtualCom1Receiver>,
    pub last_request: Option<GpsRequest>,
    pub last_packet: Option<GpsPacket>,
    pub last_error: Option<GpsFault>,
    pub requests_started: u64,
    pub completed_requests: u64,
    pub cancelled_requests: u64,
    pub ignored_sentences: u64,
    pub resynchronized_frames: u64,
    pending: Option<Pending>,
    last_now_us: Option<u64>,
}
impl GpsState {
    pub fn configured_virtual() -> Self {
        Self {
            receiver: Some(VirtualCom1Receiver::default()),
            ..Self::default()
        }
    }
    pub fn inject_rx(&mut self, at_us: u64, bytes: impl AsRef<[u8]>) -> Result<(), GpsFault> {
        let result = if self.last_now_us.is_some_and(|now| at_us < now) {
            Err(GpsFault::PastRxTime)
        } else {
            match self.receiver.as_mut() {
                Some(receiver) => receiver.inject_rx(at_us, bytes),
                None => Err(GpsFault::BackendUnavailable),
            }
        };
        if let Err(error) = &result {
            self.last_error = Some(error.clone());
        }
        result
    }
    pub fn is_receiving(&self) -> bool {
        self.pending.is_some()
    }
    pub fn owner(&self) -> Option<usize> {
        self.pending.as_ref().map(|pending| pending.request.owner)
    }
    pub fn partial_frame_len(&self) -> usize {
        self.pending
            .as_ref()
            .map_or(0, |pending| pending.frame.len())
    }

    fn reject(&mut self, memory: &mut Memory, owner: usize, fault: GpsFault) {
        if self.owner() == Some(owner) {
            self.pending = None;
            memory.set_system(READY, false);
        }
        memory.execution_limitations.insert(fault.to_string());
        self.last_error = Some(fault);
    }
    fn receive_error(&mut self, memory: &mut Memory, fault: GpsFault) {
        memory.set_system(REQUEST, false);
        memory.set_system(READY, false);
        memory.set_system(COMPLETED, false);
        memory.set_system(RECEIVE_ERROR, true);
        match fault {
            GpsFault::Timeout => memory.set_word(d(1250), 1),
            GpsFault::Checksum => memory.set_word(d(1250), 2),
            _ => {}
        }
        self.last_error = Some(fault);
        self.pending = None;
    }
    fn cancel(&mut self, memory: &mut Memory) {
        self.pending = None;
        self.cancelled_requests = self.cancelled_requests.saturating_add(1);
        memory.set_system(REQUEST, false);
        memory.set_system(READY, false);
    }

    fn step(
        &mut self,
        action: &GpsAction,
        memory: &mut Memory,
        power: bool,
        owner: usize,
        now_us: u64,
    ) {
        if self.last_now_us.is_some_and(|previous| now_us < previous) {
            self.reject(memory, owner, GpsFault::ClockRegression);
            return;
        }
        self.last_now_us = Some(now_us);
        if let Some(receiver) = &mut self.receiver {
            receiver.advance_floor(now_us);
        }
        if !power || !memory.system_bit(REQUEST) {
            if self.owner() == Some(owner) {
                self.cancel(memory);
                if let Some(receiver) = &mut self.receiver {
                    receiver.take_due(now_us);
                }
            } else if self.pending.is_none()
                && let Some(receiver) = &mut self.receiver
            {
                receiver.discard_before(now_us);
            }
            return;
        }
        if let Some(other) = self.owner()
            && other != owner
        {
            self.reject(memory, owner, GpsFault::PortBusy(other));
            return;
        }
        let Some(receiver) = &self.receiver else {
            self.reject(memory, owner, GpsFault::BackendUnavailable);
            return;
        };
        if receiver.profile != Com1Profile::default()
            || receiver.date_century < 2000
            || receiver.date_century % 100 != 0
            || receiver.date_century.checked_add(99).is_none()
        {
            self.reject(memory, owner, GpsFault::InvalidProfile);
            return;
        }
        let century = receiver.date_century;
        if memory.word(d(1036)) as u16 != 0x81 {
            self.reject(
                memory,
                owner,
                GpsFault::InvalidProtocol(memory.word(d(1036)) as u16),
            );
            return;
        }
        if self.pending.is_none() {
            let sentence = match GpsSentence::from_selector(action.source.read(memory)) {
                Ok(value) => value,
                Err(error) => {
                    self.reject(memory, owner, error);
                    return;
                }
            };
            if let Err(error) = destination(
                action.dest,
                sentence,
                memory.data_register_limit.unwrap_or(9999),
            ) {
                self.reject(memory, owner, error);
                return;
            }
            let timeout_ms = memory.word(d(1249)) as u16;
            if timeout_ms == 0 {
                self.reject(memory, owner, GpsFault::InvalidTimeout);
                return;
            }
            let request = GpsRequest {
                owner,
                sentence,
                dest: action.dest,
                started_us: now_us,
                deadline_us: now_us.saturating_add(u64::from(timeout_ms) * 1000),
                timeout_ms,
                date_century: century,
            };
            self.last_request = Some(request.clone());
            self.requests_started = self.requests_started.saturating_add(1);
            self.pending = Some(Pending {
                request,
                frame: Vec::new(),
            });
            self.last_error = None;
            memory.set_system(READY, true);
            memory.set_system(COMPLETED, false);
            memory.set_system(RECEIVE_ERROR, false);
        }
        let mut pending = self.pending.take().unwrap();
        let chunks = self.receiver.as_mut().unwrap().take_due(now_us);
        for chunk in chunks {
            if chunk.at_us < pending.request.started_us {
                continue;
            }
            if chunk.at_us > pending.request.deadline_us {
                self.receive_error(memory, GpsFault::Timeout);
                return;
            }
            for value in chunk.bytes {
                if value == b'$' {
                    if !pending.frame.is_empty() {
                        self.resynchronized_frames = self.resynchronized_frames.saturating_add(1);
                    }
                    pending.frame.clear();
                    pending.frame.push(value);
                    continue;
                }
                if pending.frame.is_empty() {
                    continue;
                }
                pending.frame.push(value);
                if pending.frame.len() > MAX_FRAME_BYTES {
                    self.receive_error(memory, GpsFault::FrameTooLong);
                    return;
                }
                if value != b'\n' {
                    continue;
                }
                let frame = std::mem::take(&mut pending.frame);
                if identifier(&frame) != pending.request.sentence.identifier().as_bytes() {
                    self.ignored_sentences = self.ignored_sentences.saturating_add(1);
                    continue;
                }
                let packet = match parse_nmea_sentence(
                    &frame,
                    pending.request.sentence,
                    pending.request.date_century,
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.receive_error(memory, error);
                        return;
                    }
                };
                if let Err(error) = destination(
                    pending.request.dest,
                    pending.request.sentence,
                    memory.data_register_limit.unwrap_or(9999),
                ) {
                    self.reject(memory, owner, error);
                    memory.set_system(READY, false);
                    return;
                }
                // Parse and validate all fields before the first destination write.
                for (offset, value) in packet.writes() {
                    memory.set_word(d(pending.request.dest.index + offset), value as i16);
                }
                self.last_packet = Some(packet);
                self.last_error = None;
                self.completed_requests = self.completed_requests.saturating_add(1);
                memory.set_system(REQUEST, false);
                memory.set_system(READY, false);
                memory.set_system(COMPLETED, true);
                memory.set_system(RECEIVE_ERROR, false);
                return;
            }
        }
        if now_us >= pending.request.deadline_us {
            self.receive_error(memory, GpsFault::Timeout);
        } else {
            self.pending = Some(pending);
        }
    }
}

impl GpsAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if name != "GPS" || args.len() != 2 {
            return Err(
                "GPS S D: iki operand gerekli; pulse/çift kelime biçimi belgelenmiyor".into(),
            );
        }
        let source = Operand::parse(&args[0])?;
        if matches!(&source,Operand::Register(device)if device.area!=Area::D) {
            return Err("GPS S operandı K/H sabiti veya D register olmalı".into());
        }
        let dest: Device = args[1].parse()?;
        if dest.area != Area::D {
            return Err("GPS hedefi D register olmalı".into());
        }
        word_range(dest, 1)?;
        if let Operand::Constant(value) = &source {
            let sentence = GpsSentence::from_selector(*value).map_err(|error| error.to_string())?;
            word_range(dest, sentence.words())?;
        }
        Ok(Self { source, dest })
    }
    pub fn name(&self) -> &'static str {
        "GPS"
    }
    /// Candidate associated with the installed five-word stub. It is for local
    /// layout only; the native encoder must reject GPS rather than execute it.
    pub fn header(&self) -> u16 {
        0x8001
    }
    pub fn dest(&self) -> Device {
        self.dest
    }
    pub fn label(&self) -> String {
        format!("GPS {} {}", self.source, self.dest)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.insert(self.dest);
        if let Operand::Register(source) = self.source {
            out.insert(source);
        }
        out.extend([1138, REQUEST, READY, COMPLETED, RECEIVE_ERROR].map(Memory::system_device));
        out.extend([1036, 1249, 1250].map(d));
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        let limit = memory.data_register_limit.unwrap_or(9999);
        if let Ok(sentence) = GpsSentence::from_selector(self.source.read(memory)) {
            output_span(self.dest, sentence.words(), limit, out);
        }
        if let Some(pending) = &memory.gps.pending {
            output_span(
                pending.request.dest,
                pending.request.sentence.words(),
                limit,
                out,
            );
        }
    }
    pub fn execute(&self, memory: &mut Memory, power: bool, instance: usize, now_us: u64) {
        let mut state = std::mem::take(&mut memory.gps);
        state.step(self, memory, power, instance, now_us);
        memory.gps = state;
    }
}

pub fn nmea_checksum(body: &[u8]) -> u8 {
    body.iter().fold(0, |checksum, value| checksum ^ value)
}

pub fn parse_nmea_sentence(
    frame: &[u8],
    sentence: GpsSentence,
    century: u16,
) -> Result<GpsPacket, GpsFault> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(GpsFault::FrameTooLong);
    }
    if !frame.starts_with(b"$") || !frame.ends_with(b"\r\n") || !frame.is_ascii() {
        return Err(GpsFault::Framing);
    }
    let content = &frame[1..frame.len() - 2];
    let star = content
        .iter()
        .position(|value| *value == b'*')
        .ok_or(GpsFault::Checksum)?;
    if star + 3 != content.len() {
        return Err(GpsFault::Checksum);
    }
    let expected = std::str::from_utf8(&content[star + 1..])
        .ok()
        .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        .ok_or(GpsFault::Checksum)?;
    if nmea_checksum(&content[..star]) != expected {
        return Err(GpsFault::Checksum);
    }
    let body = std::str::from_utf8(&content[..star]).map_err(|_| GpsFault::Framing)?;
    let fields: Vec<_> = body.split(',').collect();
    if fields.first().copied() != Some(sentence.identifier()) {
        return Err(field("unexpected sentence identifier"));
    }
    if century < 2000 || !century.is_multiple_of(100) {
        return Err(field("unconfigured date century"));
    }
    let (validity, raw_status, lat_index, lon_index) = match sentence {
        GpsSentence::Gga => {
            if fields.len() < 15 {
                return Err(field("incomplete GGA fields"));
            }
            let quality = fields[6].as_bytes();
            let Some(&quality) = quality.first().filter(|_| quality.len() == 1) else {
                return Err(field("GGA quality must be a single digit"));
            };
            if !(b'0'..=b'2').contains(&quality) {
                return Err(field(
                    "GGA quality outside the documented PLC 0/1/2 mapping",
                ));
            }
            (u16::from(quality - b'0'), quality, 2, 4)
        }
        GpsSentence::Rmc => {
            if fields.len() < 12 {
                return Err(field("incomplete RMC fields"));
            }
            let status = match fields[2] {
                "A" => b'A',
                "V" => b'V',
                _ => return Err(field("RMC status must be A or V")),
            };
            let mode = fields.get(12).copied().unwrap_or("");
            let validity = if status == b'V' || mode == "N" {
                0
            } else {
                match mode {
                    "" | "A" => 1,
                    "D" => 2,
                    _ => return Err(field("RMC mode has no established PLC 0/1/2 mapping")),
                }
            };
            (validity, status, 3, 5)
        }
    };
    let required = validity != 0;
    let utc = if fields[1].is_empty() && !required {
        None
    } else {
        Some(parse_time(fields[1])?)
    };
    let latitude = parse_coordinate(fields[lat_index], fields[lat_index + 1], true, required)?;
    let longitude = parse_coordinate(fields[lon_index], fields[lon_index + 1], false, required)?;
    let (altitude_m, date) = match sentence {
        GpsSentence::Gga => {
            let altitude = if fields[9].is_empty() && !required {
                None
            } else {
                let value = parse_decimal(fields[9])?;
                if !(0.0..=9999.9).contains(&value) || fields[10] != "M" {
                    return Err(field("altitude must be 0..9999.9 metres"));
                }
                Some(value as f32)
            };
            (altitude, None)
        }
        GpsSentence::Rmc => {
            let date = if fields[9].is_empty() && !required {
                None
            } else {
                Some(parse_date(fields[9], century)?)
            };
            (None, date)
        }
    };
    Ok(GpsPacket {
        sentence,
        raw_sentence: String::from_utf8(frame.to_vec()).map_err(|_| GpsFault::Framing)?,
        raw_time: fields[1].to_owned(),
        utc,
        latitude,
        longitude,
        validity,
        raw_status,
        altitude_m,
        date,
    })
}

fn parse_time(value: &str) -> Result<[u16; 3], GpsFault> {
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, None), |(whole, fraction)| (whole, Some(fraction)));
    if whole.len() != 6
        || !whole.bytes().all(|value| value.is_ascii_digit())
        || fraction.is_some_and(|part| {
            part.is_empty() || !part.bytes().all(|value| value.is_ascii_digit())
        })
    {
        return Err(field("UTC must be hhmmss with optional fractional seconds"));
    }
    let hour = whole[..2].parse::<u16>().unwrap();
    let minute = whole[2..4].parse::<u16>().unwrap();
    let second = whole[4..].parse::<u16>().unwrap();
    if hour > 23 || minute > 59 || second > 59 {
        return Err(field(
            "UTC is outside the documented hour/minute/second ranges",
        ));
    }
    Ok([hour, minute, second])
}

fn parse_coordinate(
    value: &str,
    direction: &str,
    latitude: bool,
    required: bool,
) -> Result<Option<GpsCoordinate>, GpsFault> {
    if value.is_empty() && direction.is_empty() && !required {
        return Ok(None);
    }
    let whole = value.split('.').next().unwrap_or("");
    if whole.len() != if latitude { 4 } else { 5 } {
        return Err(field("coordinate must use NMEA degrees/minutes digits"));
    }
    let raw = parse_decimal(value)?;
    let degrees = whole[..whole.len() - 2]
        .parse::<u16>()
        .map_err(|_| field("coordinate degrees are invalid"))?;
    let minutes = raw - f64::from(degrees) * 100.0;
    let max = if latitude { 90 } else { 180 };
    if degrees > max || !(0.0..60.0).contains(&minutes) || (degrees == max && minutes != 0.0) {
        return Err(field("coordinate degrees/minutes are outside range"));
    }
    let hemisphere = match (latitude, direction) {
        (true, "N") => b'N',
        (true, "S") => b'S',
        (false, "E") => b'E',
        (false, "W") => b'W',
        _ => return Err(field("coordinate hemisphere is invalid")),
    };
    let packed = (f64::from(degrees) + minutes / 100.0) as f32;
    let decimal = f64::from(degrees) + minutes / 60.0;
    let signed = if matches!(hemisphere, b'S' | b'W') {
        -decimal
    } else {
        decimal
    };
    Ok(Some(GpsCoordinate {
        raw_ascii: value.to_owned(),
        hemisphere,
        packed_degrees_minutes: packed,
        decimal_degrees: signed as f32,
    }))
}

fn parse_decimal(value: &str) -> Result<f64, GpsFault> {
    let mut dot = false;
    if value.is_empty()
        || !value.bytes().all(|value| {
            if value == b'.' && !dot {
                dot = true;
                true
            } else {
                value.is_ascii_digit()
            }
        })
        || value == "."
    {
        return Err(field("numeric ASCII field is malformed"));
    }
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| field("numeric ASCII field is not finite"))
}

fn parse_date(value: &str, century: u16) -> Result<[u16; 3], GpsFault> {
    if value.len() != 6 || !value.bytes().all(|value| value.is_ascii_digit()) {
        return Err(field("RMC date must be ddmmyy"));
    }
    let day = value[..2].parse::<u16>().unwrap();
    let month = value[2..4].parse::<u16>().unwrap();
    let year = century
        .checked_add(value[4..].parse::<u16>().unwrap())
        .ok_or_else(|| field("date century overflows a word"))?;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    if day == 0 || day > days {
        return Err(field("RMC calendar date is invalid"));
    }
    Ok([day, month, year])
}
fn identifier(frame: &[u8]) -> &[u8] {
    if !frame.starts_with(b"$") {
        return &[];
    }
    let stop = frame[1..]
        .iter()
        .position(|value| matches!(value, b',' | b'*' | b'\r' | b'\n'))
        .map_or(frame.len(), |index| index + 1);
    &frame[1..stop]
}
fn destination(dest: Device, sentence: GpsSentence, limit: u16) -> Result<(), GpsFault> {
    if dest.area != Area::D
        || dest
            .index
            .checked_add(sentence.words() - 1)
            .is_none_or(|end| end > limit)
    {
        return Err(GpsFault::DestinationRange);
    }
    let end = dest.index + sentence.words() - 1;
    if [1036, 1249, 1250]
        .iter()
        .any(|index| (dest.index..=end).contains(index))
    {
        return Err(GpsFault::DestinationControlOverlap);
    }
    Ok(())
}
fn output_span(dest: Device, count: u16, limit: u16, out: &mut BTreeSet<Device>) {
    out.extend(
        (dest.index..dest.index.saturating_add(count))
            .filter(|index| *index <= limit)
            .map(d),
    );
}
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn field(message: &str) -> GpsFault {
    GpsFault::Sentence(message.to_owned())
}
fn float_words(out: &mut Vec<(u16, u16)>, offset: u16, value: f32) {
    let bits = value.to_bits();
    out.extend([(offset, bits as u16), (offset + 1, (bits >> 16) as u16)]);
}

#[cfg(test)]
mod tests {
    use super::*;
    // Manufacturer examples with literal, independently printed checksums.
    const GGA: &str =
        "$GPGGA,092725.00,4717.11399,N,00833.91590,E,1,08,1.01,499.6,M,48.0,M,,*5B\r\n";
    const RMC: &str = "$GPRMC,083559.00,A,4717.11437,N,00833.91522,E,0.004,77.52,091202,,,A*57\r\n";
    fn action(selector: i16, dest: u16) -> GpsAction {
        GpsAction {
            source: Operand::Constant(selector),
            dest: d(dest),
        }
    }
    fn configured() -> Memory {
        let mut m = Memory::default();
        m.gps = GpsState::configured_virtual();
        m.set_word(d(1036), 0x81);
        m.set_word(d(1249), 2000);
        m.set_system(REQUEST, true);
        m
    }
    fn seed(m: &mut Memory, base: u16) {
        for offset in 0..17 {
            m.set_word(d(base + offset), (0x5a00 + offset) as i16);
        }
    }
    fn words(m: &Memory, base: u16) -> Vec<i16> {
        (0..17).map(|offset| m.word(d(base + offset))).collect()
    }
    fn number(m: &Memory, index: u16) -> f32 {
        f32::from_bits(m.double_word(d(index)) as u32)
    }
    fn frame(body: &str) -> Vec<u8> {
        format!("${body}*{:02X}\r\n", nmea_checksum(body.as_bytes())).into_bytes()
    }
    fn body(sentence: &str) -> &str {
        sentence
            .strip_prefix('$')
            .unwrap()
            .split('*')
            .next()
            .unwrap()
    }

    #[test]
    fn manufacturer_gga_maps_documented_words_and_preserves_unassigned_word() {
        let mut m = configured();
        seed(&mut m, 100);
        m.gps.inject_rx(0, GGA).unwrap();
        action(0, 100).execute(&mut m, true, 0, 0);
        assert_eq!(
            [m.word(d(100)), m.word(d(101)), m.word(d(102))],
            [9, 27, 25]
        );
        assert_eq!(number(&m, 103).to_bits(), 0x423c_af3f);
        assert_eq!(m.word(d(105)), 0);
        assert_eq!(number(&m, 106).to_bits(), 0x4105_6d32);
        assert_eq!(m.word(d(108)), 0);
        assert_eq!(m.word(d(109)), 1);
        assert_eq!(number(&m, 110), 499.6);
        assert_eq!(number(&m, 112).to_bits(), 0x423d_2414);
        assert_eq!(number(&m, 114).to_bits(), 0x4109_0b53);
        assert_eq!(m.word(d(116)), 0x5a10);
        assert!(m.system_bit(COMPLETED));
        assert!(!m.system_bit(REQUEST));
        assert!(!m.system_bit(READY));
        assert!(!m.system_bit(RECEIVE_ERROR));
        let packet = m.gps.last_packet.as_ref().unwrap();
        assert_eq!(packet.raw_status, b'1');
        assert_eq!(packet.raw_time, "092725.00");
        assert_eq!(packet.latitude.as_ref().unwrap().raw_ascii, "4717.11399");
    }
    #[test]
    fn manufacturer_rmc_maps_calendar_and_signed_decimal_coordinate_pairs() {
        let mut m = configured();
        seed(&mut m, 100);
        m.gps.inject_rx(0, RMC).unwrap();
        action(1, 100).execute(&mut m, true, 0, 0);
        assert_eq!(
            [m.word(d(100)), m.word(d(101)), m.word(d(102))],
            [8, 35, 59]
        );
        assert_eq!(
            [m.word(d(110)), m.word(d(111)), m.word(d(112))],
            [9, 12, 2002]
        );
        assert_eq!(number(&m, 103).to_bits(), 0x423c_af40);
        assert_eq!(number(&m, 106).to_bits(), 0x4105_6d2b);
        assert_eq!(number(&m, 113).to_bits(), 0x423d_2416);
        assert_eq!(number(&m, 115).to_bits(), 0x4109_0b47);
        assert_eq!(m.gps.last_packet.as_ref().unwrap().raw_status, b'A');
        let southern = body(RMC)
            .replace(",N,", ",S,")
            .replace(",E,", ",W,")
            .replace(",,,A", ",,,D");
        let packet = parse_nmea_sentence(&frame(&southern), GpsSentence::Rmc, 2000).unwrap();
        assert_eq!(packet.validity, 2);
        assert!(packet.latitude.unwrap().decimal_degrees < 0.0);
        assert!(packet.longitude.unwrap().decimal_degrees < 0.0);
    }
    #[test]
    fn partial_and_out_of_order_injected_chunks_wait_for_complete_checksum() {
        let mut m = configured();
        seed(&mut m, 100);
        let before = words(&m, 100);
        let a = action(0, 100);
        a.execute(&mut m, true, 0, 0);
        let half = GGA.len() / 2;
        m.gps.inject_rx(2000, &GGA.as_bytes()[half..]).unwrap();
        m.gps.inject_rx(1000, &GGA.as_bytes()[..half]).unwrap();
        a.execute(&mut m, true, 0, 1500);
        assert_eq!(words(&m, 100), before);
        assert!(m.system_bit(READY));
        assert_eq!(m.gps.partial_frame_len(), half);
        a.execute(&mut m, true, 0, 2000);
        assert!(m.system_bit(COMPLETED));
        assert_eq!(m.word(d(109)), 1);
        assert_eq!(m.gps.completed_requests, 1);
    }
    #[test]
    fn timeout_uses_virtual_time_and_another_request_is_required_after_error() {
        let mut m = configured();
        seed(&mut m, 100);
        let before = words(&m, 100);
        let a = action(0, 100);
        a.execute(&mut m, true, 0, 0);
        m.set_word(d(1249), 1);
        a.execute(&mut m, true, 0, 1_999_999);
        assert!(m.system_bit(READY));
        assert!(!m.system_bit(RECEIVE_ERROR));
        a.execute(&mut m, true, 0, 2_000_000);
        assert_eq!(m.word(d(1250)), 1);
        assert!(m.system_bit(RECEIVE_ERROR));
        assert_eq!(words(&m, 100), before);
        m.set_system(REQUEST, true);
        m.gps.inject_rx(2_000_001, GGA).unwrap();
        a.execute(&mut m, true, 0, 2_000_001);
        assert!(m.system_bit(COMPLETED));
        assert!(!m.system_bit(RECEIVE_ERROR));
        assert_eq!(m.word(d(1250)), 1);
        assert_eq!(m.gps.requests_started, 2);
    }
    #[test]
    fn checksum_and_semantic_errors_preserve_output_without_invented_codes() {
        let mut m = configured();
        seed(&mut m, 100);
        let before = words(&m, 100);
        let mut corrupt = GGA.as_bytes().to_vec();
        let star = corrupt.iter().position(|v| *v == b'*').unwrap();
        corrupt[star + 1] = b'0';
        corrupt[star + 2] = b'0';
        m.gps.inject_rx(0, corrupt).unwrap();
        action(0, 100).execute(&mut m, true, 0, 0);
        assert_eq!(m.word(d(1250)), 2);
        assert_eq!(words(&m, 100), before);
        assert!(m.system_bit(RECEIVE_ERROR));
        assert_eq!(m.gps.last_error, Some(GpsFault::Checksum));
        m.set_word(d(1250), 77);
        m.set_system(REQUEST, true);
        let invalid = body(GGA).replace("4717.11399", "4760.00000");
        m.gps.inject_rx(1000, frame(&invalid)).unwrap();
        action(0, 100).execute(&mut m, true, 0, 1000);
        assert_eq!(m.word(d(1250)), 77);
        assert_eq!(words(&m, 100), before);
        assert!(matches!(m.gps.last_error, Some(GpsFault::Sentence(_))));
    }
    #[test]
    fn invalid_fix_commits_validity_without_fabricating_empty_coordinates() {
        let mut m = configured();
        seed(&mut m, 100);
        let old_lat = number(&m, 103).to_bits();
        let old_alt = number(&m, 110).to_bits();
        m.gps
            .inject_rx(0, frame("GPGGA,092725.00,,,,,0,00,99.99,,,,,,"))
            .unwrap();
        action(0, 100).execute(&mut m, true, 0, 0);
        assert!(m.system_bit(COMPLETED));
        assert_eq!(m.word(d(109)), 0);
        assert_eq!(number(&m, 103).to_bits(), old_lat);
        assert_eq!(number(&m, 110).to_bits(), old_alt);
        assert!(m.gps.last_packet.as_ref().unwrap().latitude.is_none());
        let packet = parse_nmea_sentence(
            &frame("GPRMC,083559.00,V,,,,,,,091202,,,N"),
            GpsSentence::Rmc,
            2000,
        )
        .unwrap();
        assert_eq!(packet.validity, 0);
        assert_eq!(packet.date, Some([9, 12, 2002]));
    }
    #[test]
    fn request_off_and_false_power_cancel_without_clearing_previous_data() {
        let mut m = configured();
        seed(&mut m, 100);
        let before = words(&m, 100);
        let a = action(0, 100);
        a.execute(&mut m, true, 0, 0);
        m.gps.inject_rx(1000, &GGA.as_bytes()[..20]).unwrap();
        a.execute(&mut m, true, 0, 1000);
        assert!(m.gps.is_receiving());
        m.set_system(REQUEST, false);
        a.execute(&mut m, true, 0, 2000);
        assert!(!m.gps.is_receiving());
        assert_eq!(m.gps.cancelled_requests, 1);
        m.gps.inject_rx(3000, &GGA.as_bytes()[20..]).unwrap();
        m.set_system(REQUEST, true);
        a.execute(&mut m, true, 0, 4000);
        assert!(m.gps.is_receiving());
        a.execute(&mut m, false, 0, 5000);
        a.execute(&mut m, false, 0, 4_000_000);
        assert_eq!(words(&m, 100), before);
        assert!(!m.system_bit(RECEIVE_ERROR));
        assert!(!m.system_bit(READY));
        m.set_system(REQUEST, true);
        m.gps.inject_rx(4_001_000, GGA).unwrap();
        a.execute(&mut m, true, 0, 4_001_000);
        assert!(m.system_bit(COMPLETED));
    }
    #[test]
    fn complete_frame_at_deadline_wins_but_late_final_bytes_time_out() {
        let mut m = configured();
        let a = action(0, 100);
        a.execute(&mut m, true, 0, 0);
        m.gps.inject_rx(2_000_000, GGA).unwrap();
        a.execute(&mut m, true, 0, 2_000_000);
        assert!(m.system_bit(COMPLETED));
        let mut m = configured();
        seed(&mut m, 100);
        let before = words(&m, 100);
        a.execute(&mut m, true, 0, 0);
        m.gps
            .inject_rx(1_999_999, &GGA.as_bytes()[..GGA.len() - 1])
            .unwrap();
        m.gps.inject_rx(2_000_001, b"\n").unwrap();
        a.execute(&mut m, true, 0, 2_000_001);
        assert_eq!(m.gps.last_error, Some(GpsFault::Timeout));
        assert_eq!(words(&m, 100), before);
    }
    #[test]
    fn framing_resynchronizes_and_filters_other_talkers_and_sentences() {
        let mut m = configured();
        let a = action(0, 100);
        a.execute(&mut m, true, 0, 0);
        let mut bytes = b"garbage$GPGGA,partial".to_vec();
        bytes.extend(frame(&body(GGA).replacen("GPGGA", "GNGGA", 1)));
        bytes.extend_from_slice(RMC.as_bytes());
        bytes.extend_from_slice(GGA.as_bytes());
        m.gps.inject_rx(1000, bytes).unwrap();
        a.execute(&mut m, true, 0, 1000);
        assert!(m.system_bit(COMPLETED));
        assert_eq!(m.gps.ignored_sentences, 2);
        assert_eq!(m.gps.resynchronized_frames, 1);
        assert_eq!(
            m.gps.last_packet.as_ref().unwrap().sentence,
            GpsSentence::Gga
        );
    }
    #[test]
    fn calendar_hemisphere_and_raw_ascii_policies_are_explicit() {
        let southern = "GPRMC,235959.75,A,0000.00000,S,00000.00000,W,0.0,0.0,290224,,,A";
        let packet = parse_nmea_sentence(&frame(southern), GpsSentence::Rmc, 2000).unwrap();
        assert_eq!(packet.utc, Some([23, 59, 59]));
        assert_eq!(packet.date, Some([29, 2, 2024]));
        assert_eq!(
            packet.latitude.unwrap().decimal_degrees.to_bits(),
            0x80000000
        );
        assert_eq!(
            packet.longitude.unwrap().decimal_degrees.to_bits(),
            0x80000000
        );
        assert!(
            parse_nmea_sentence(
                &frame(&southern.replace("290224", "290223")),
                GpsSentence::Rmc,
                2000
            )
            .is_err()
        );
        assert!(
            parse_nmea_sentence(
                &frame(&southern.replace("290224", "290200")),
                GpsSentence::Rmc,
                2100
            )
            .is_err()
        );
        let future = parse_nmea_sentence(
            &frame(&body(RMC).replace("091202", "230394")),
            GpsSentence::Rmc,
            2000,
        )
        .unwrap();
        assert_eq!(future.date, Some([23, 3, 2094]));
        assert!(
            parse_nmea_sentence(
                &frame(&body(RMC).replace("083559.00", "235960.00")),
                GpsSentence::Rmc,
                2000
            )
            .is_err()
        );
        assert!(
            parse_nmea_sentence(
                &frame(&body(GGA).replace(",E,1,08", ",E,4,08")),
                GpsSentence::Gga,
                2000
            )
            .is_err()
        );
    }
    #[test]
    fn destination_boundaries_and_selector_overlap_commit_atomically() {
        let mut m = configured();
        m.data_register_limit = Some(4999);
        m.gps.inject_rx(0, GGA).unwrap();
        let a = action(0, 4984);
        a.execute(&mut m, true, 0, 0);
        assert!(m.system_bit(COMPLETED));
        let mut seen = BTreeSet::new();
        a.observed_devices(&m, &mut seen);
        assert!(!seen.iter().any(|v| v.area == Area::D && v.index > 4999));
        let mut m = configured();
        m.data_register_limit = Some(4999);
        m.set_word(d(4984), 1234);
        action(1, 4984).execute(&mut m, true, 0, 0);
        assert_eq!(m.gps.last_error, Some(GpsFault::DestinationRange));
        assert_eq!(m.word(d(4984)), 1234);
        let mut m = configured();
        m.set_word(d(0), 0);
        m.gps.inject_rx(0, GGA).unwrap();
        GpsAction {
            source: Operand::Register(d(0)),
            dest: d(0),
        }
        .execute(&mut m, true, 0, 0);
        assert_eq!(m.word(d(0)), 9);
        assert!(m.system_bit(COMPLETED));
        let mut m = configured();
        m.set_word(d(1250), 77);
        action(0, 1240).execute(&mut m, true, 0, 0);
        assert_eq!(m.gps.last_error, Some(GpsFault::DestinationControlOverlap));
        assert_eq!(m.word(d(1250)), 77);
    }
    #[test]
    fn one_com1_owner_and_snapshot_selector_prevent_cross_instruction_mixups() {
        let mut m = configured();
        let first = action(0, 100);
        let other = action(1, 200);
        first.execute(&mut m, true, 7, 0);
        other.execute(&mut m, true, 8, 0);
        assert_eq!(m.gps.owner(), Some(7));
        assert_eq!(m.gps.last_error, Some(GpsFault::PortBusy(7)));
        other.execute(&mut m, false, 8, 0);
        assert_eq!(m.gps.owner(), Some(7));
        m.gps.inject_rx(1000, GGA).unwrap();
        first.execute(&mut m, true, 7, 1000);
        assert!(m.system_bit(COMPLETED));
        assert_eq!(m.word(d(209)), 0);
        let mut m = configured();
        m.set_word(d(0), 1);
        let dynamic = GpsAction {
            source: Operand::Register(d(0)),
            dest: d(100),
        };
        dynamic.execute(&mut m, true, 0, 0);
        m.set_word(d(0), 0);
        let mut seen = BTreeSet::new();
        dynamic.observed_devices(&m, &mut seen);
        assert!(seen.contains(&d(116)));
        m.gps.inject_rx(1000, RMC).unwrap();
        dynamic.execute(&mut m, true, 0, 1000);
        assert_eq!(m.word(d(112)), 2002);
    }
    #[test]
    fn unconfigured_or_wrong_serial_profile_never_fabricates_a_fix() {
        let mut m = Memory::default();
        m.set_system(REQUEST, true);
        m.set_word(d(100), 1234);
        action(0, 100).execute(&mut m, true, 0, 0);
        assert_eq!(m.word(d(100)), 1234);
        assert_eq!(m.gps.last_error, Some(GpsFault::BackendUnavailable));
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(COMPLETED));
        assert_eq!(m.gps.inject_rx(0, GGA), Err(GpsFault::BackendUnavailable));
        let mut m = configured();
        m.gps.receiver.as_mut().unwrap().profile.baud = 4800;
        action(0, 100).execute(&mut m, true, 0, 0);
        assert_eq!(m.gps.last_error, Some(GpsFault::InvalidProfile));
        assert_eq!(m.gps.requests_started, 0);
        let mut m = configured();
        m.set_word(d(1036), 0x80);
        action(0, 100).execute(&mut m, true, 0, 0);
        assert_eq!(m.gps.last_error, Some(GpsFault::InvalidProtocol(0x80)));
        let mut m = configured();
        m.set_word(d(1249), 0);
        action(0, 100).execute(&mut m, true, 0, 0);
        assert_eq!(m.gps.last_error, Some(GpsFault::InvalidTimeout));
    }
    #[test]
    fn framing_and_clock_regression_are_visible_without_new_plc_error_codes() {
        let mut m = configured();
        seed(&mut m, 100);
        let before = words(&m, 100);
        m.set_word(d(1250), 77);
        let mut incomplete = GGA.as_bytes()[..GGA.len() - 2].to_vec();
        incomplete.push(b'\n');
        m.gps.inject_rx(0, incomplete).unwrap();
        action(0, 100).execute(&mut m, true, 0, 0);
        assert_eq!(m.gps.last_error, Some(GpsFault::Framing));
        assert_eq!(m.word(d(1250)), 77);
        assert_eq!(words(&m, 100), before);
        let mut m = configured();
        action(0, 100).execute(&mut m, true, 0, 2000);
        action(0, 100).execute(&mut m, true, 0, 1999);
        assert_eq!(m.gps.last_error, Some(GpsFault::ClockRegression));
        assert!(!m.execution_limitations.is_empty());
    }
    #[test]
    fn grammar_accepts_only_documented_operands_and_word_spans() {
        let parse = |name: &str, args: &[&str]| {
            GpsAction::parse(
                name,
                &args.iter().map(|v| (*v).to_owned()).collect::<Vec<_>>(),
            )
        };
        assert!(parse("GPS", &["K0", "D9984"]).is_ok());
        assert!(parse("GPS", &["K1", "D9984"]).is_err());
        assert!(parse("GPS", &["T0", "D0"]).is_err());
        assert!(parse("GPS", &["K2", "D0"]).is_err());
        assert!(parse("GPSP", &["K0", "D0"]).is_err());
    }

    #[test]
    fn rx_api_capacity_and_past_timestamp_fail_atomically_without_plc_errors() {
        let mut m = configured();
        m.set_word(d(1250), 77);
        let a = action(0, 100);
        a.execute(&mut m, true, 0, 0);
        m.gps.inject_rx(1000, &GGA.as_bytes()[..20]).unwrap();
        a.execute(&mut m, true, 0, 1000);
        let pending = m.gps.partial_frame_len();
        let owner = m.gps.owner();
        assert_eq!(
            m.gps.inject_rx(1000, vec![b'X'; MAX_RX_BYTES + 1]),
            Err(GpsFault::QueueCapacity)
        );
        assert_eq!(m.gps.inject_rx(999, b"old"), Err(GpsFault::PastRxTime));
        assert_eq!(m.gps.partial_frame_len(), pending);
        assert_eq!(m.gps.owner(), owner);
        assert_eq!(m.gps.receiver.as_ref().unwrap().queued_bytes(), 0);
        assert!(m.system_bit(READY));
        assert!(!m.system_bit(RECEIVE_ERROR));
        assert_eq!(m.word(d(1250)), 77);
        m.gps.inject_rx(2000, &GGA.as_bytes()[20..]).unwrap();
        a.execute(&mut m, true, 0, 2000);
        assert!(m.system_bit(COMPLETED));
        let mut receiver = VirtualCom1Receiver::default();
        receiver.inject_rx(0, vec![b'X'; MAX_RX_BYTES]).unwrap();
        assert_eq!(receiver.inject_rx(0, b"X"), Err(GpsFault::QueueCapacity));
        assert_eq!(receiver.queued_chunks(), 1);
        assert_eq!(receiver.queued_bytes(), MAX_RX_BYTES);
        let mut receiver = VirtualCom1Receiver::default();
        for index in 0..MAX_RX_CHUNKS {
            receiver.inject_rx(index as u64, b"X").unwrap();
        }
        assert_eq!(
            receiver.inject_rx(MAX_RX_CHUNKS as u64, b"X"),
            Err(GpsFault::QueueCapacity)
        );
        assert_eq!(receiver.queued_chunks(), MAX_RX_CHUNKS);
    }

    #[test]
    fn equal_timestamp_rx_order_is_stable_and_drain_releases_both_capacities() {
        let mut receiver = VirtualCom1Receiver::default();
        receiver.inject_rx(1, b"A").unwrap();
        receiver.inject_rx(1, b"B").unwrap();
        receiver.inject_rx(0, b"Z").unwrap();
        let bytes: Vec<_> = receiver
            .take_due(1)
            .into_iter()
            .flat_map(|chunk| chunk.bytes)
            .collect();
        assert_eq!(bytes, b"ZAB");
        assert_eq!(receiver.queued_bytes(), 0);
        assert_eq!(receiver.queued_chunks(), 0);
        assert_eq!(receiver.inject_rx(0, b"past"), Err(GpsFault::PastRxTime));
        receiver.inject_rx(1, b"same clock").unwrap();
        assert_eq!(receiver.queued_chunks(), 1);
    }

    #[test]
    fn sentence_length_error_is_distinct_from_backend_capacity_and_preserves_data() {
        let mut m = configured();
        seed(&mut m, 100);
        let before = words(&m, 100);
        m.set_word(d(1250), 77);
        let mut bytes = b"$GPGGA,".to_vec();
        bytes.extend([b'X'; MAX_FRAME_BYTES]);
        m.gps.inject_rx(0, bytes).unwrap();
        action(0, 100).execute(&mut m, true, 0, 0);
        assert_eq!(m.gps.last_error, Some(GpsFault::FrameTooLong));
        assert!(m.system_bit(RECEIVE_ERROR));
        assert_eq!(m.word(d(1250)), 77);
        assert_eq!(words(&m, 100), before);
    }
}
