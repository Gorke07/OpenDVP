//! Explicit Modbus connection to a Delta DVP PLC.
//! Monitoring (`connect`/`Client::read`) is read-only: only FC01 (Y coils) and FC02
//! (X discrete inputs) are sent. `download_program` is a separate, explicit operation on
//! its own connection that writes the program image (see `opendvp::plc_download`).
use futures::channel::oneshot;
use opendvp::plc::{Area, Device};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transport {
    Serial,
    UsbSerial,
    Tcp,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Parity {
    None,
    Even,
    Odd,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Ascii,
    Rtu,
    Tcp,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub transport: Transport,
    pub port_name: String,
    pub baud: u32,
    pub data_bits: u8,
    pub parity: Parity,
    pub stop_bits: u8,
    pub protocol: Protocol,
    pub host: String,
    pub tcp_port: u16,
    pub station: u8,
    pub timeout_ms: u64,
    pub poll_ms: u64,
    pub x_start: u16,
    pub x_count: u16,
    pub y_start: u16,
    pub y_count: u16,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            transport: Transport::Serial,
            port_name: String::new(),
            baud: 9600,
            data_bits: 7,
            parity: Parity::Even,
            stop_bits: 1,
            protocol: Protocol::Ascii,
            host: String::new(),
            tcp_port: 502,
            station: 1,
            timeout_ms: 1000,
            poll_ms: 500,
            x_start: 0,
            x_count: 16,
            y_start: 0,
            y_count: 16,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if !(250..=5000).contains(&self.timeout_ms) {
            return Err("Zaman aşımı 250–5000 ms olmalı.".into());
        }
        if !(100..=10000).contains(&self.poll_ms) {
            return Err("Okuma aralığı 100–10000 ms olmalı.".into());
        }
        for (start, count) in [(self.x_start, self.x_count), (self.y_start, self.y_count)] {
            if count == 0
                || count > 256
                || start >= 256
                || u32::from(start) + u32::from(count) > 256
            {
                return Err("X/Y aralığı 0–255 içinde ve 1–256 adres olmalı.".into());
            }
        }
        match self.transport {
            Transport::Tcp => {
                if self.protocol != Protocol::Tcp {
                    return Err("Ethernet için Modbus TCP seçin.".into());
                }
                if self.host.trim().parse::<IpAddr>().is_err() {
                    return Err("Geçerli bir IP adresi girin (host adı desteklenmez).".into());
                }
                if self.tcp_port == 0 || self.station == 0 {
                    return Err("TCP portu ve istasyon sıfır olamaz.".into());
                }
            }
            Transport::Serial | Transport::UsbSerial => {
                if self.port_name.trim().is_empty() {
                    return Err("Seri port seçin.".into());
                }
                if !matches!(self.protocol, Protocol::Ascii | Protocol::Rtu) {
                    return Err("Seri bağlantı için ASCII veya RTU seçin.".into());
                }
                if !(1..=247).contains(&self.station) {
                    return Err("Seri istasyon 1–247 olmalı; yayın adresi kullanılamaz.".into());
                }
                if !(7..=8).contains(&self.data_bits)
                    || !(1..=2).contains(&self.stop_bits)
                    || !(300..=1_000_000).contains(&self.baud)
                {
                    return Err("Seri hız, veri biti veya durdurma biti geçersiz.".into());
                }
                if self.protocol == Protocol::Rtu && self.data_bits != 8 {
                    return Err("Modbus RTU 8 veri biti gerektirir.".into());
                }
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortInfo {
    pub name: String,
    pub label: String,
    pub usb: bool,
}
#[derive(Debug, Clone)]
pub struct Sample {
    pub values: Vec<(Device, bool)>,
    pub sequence: u64,
    pub latency_ms: u64,
}

async fn background<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = oneshot::channel();
    std::thread::Builder::new()
        .name("plc-read-only".into())
        .spawn(move || {
            let _ = sender.send(job());
        })
        .map_err(|e| format!("İş parçacığı başlatılamadı: {e}"))?;
    receiver
        .await
        .map_err(|_| "Bağlantı iş parçacığı beklenmedik şekilde sonlandı.".to_owned())?
}
pub async fn available_ports() -> Result<Vec<PortInfo>, String> {
    background(|| {
        serialport::available_ports()
            .map_err(|e| format!("Seri portlar listelenemedi: {e}"))
            .map(|ports| {
                ports
                    .into_iter()
                    .map(|p| {
                        let (usb, detail) = match p.port_type {
                            serialport::SerialPortType::UsbPort(info) => (
                                true,
                                [info.manufacturer, info.product]
                                    .into_iter()
                                    .flatten()
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            ),
                            _ => (false, String::new()),
                        };
                        let label = if detail.is_empty() {
                            p.port_name.clone()
                        } else {
                            format!("{} · {}", p.port_name, detail)
                        };
                        PortInfo {
                            name: p.port_name,
                            label,
                            usb,
                        }
                    })
                    .collect()
            })
    })
    .await
}
#[derive(Clone)]
pub struct Client {
    session: Arc<Mutex<Option<Session>>>,
    closed: Arc<AtomicBool>,
}
impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("closed", &self.closed.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}
impl Client {
    /// Mark closed immediately; the worker releases the port after any current I/O times out.
    pub fn disconnect(&self) {
        self.closed.store(true, Ordering::Release);
        let session = self.session.clone();
        let _ = std::thread::Builder::new()
            .name("plc-close".into())
            .spawn(move || {
                if let Ok(mut guard) = session.lock() {
                    guard.take();
                }
            });
    }
    pub async fn read(&self) -> Result<Sample, String> {
        let client = self.clone();
        background(move || {
            if client.closed.load(Ordering::Acquire) {
                return Err("Bağlantı kapalı.".into());
            }
            let mut guard = client
                .session
                .lock()
                .map_err(|_| "Bağlantı kilidi bozuldu.".to_owned())?;
            if client.closed.load(Ordering::Acquire) {
                guard.take();
                return Err("Bağlantı kapalı.".into());
            }
            let result = guard
                .as_mut()
                .ok_or("Bağlantı kapalı.")?
                .sample(&client.closed);
            if result.is_err() || client.closed.load(Ordering::Acquire) {
                client.closed.store(true, Ordering::Release);
                guard.take();
                return match result {
                    Err(error) => Err(error),
                    Ok(_) => Err("Bağlantı kapatıldı.".into()),
                };
            }
            result
        })
        .await
    }
}
impl Client {
    /// Run one force command on the live monitoring session (Delta editor's FC64 `01 30`).
    async fn with_session<T: Send + 'static>(
        &self,
        action: impl FnOnce(&mut Session) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let client = self.clone();
        background(move || {
            if client.closed.load(Ordering::Acquire) {
                return Err("Bağlantı kapalı.".into());
            }
            let mut guard = client
                .session
                .lock()
                .map_err(|_| "Bağlantı kilidi bozuldu.".to_owned())?;
            action(guard.as_mut().ok_or("Bağlantı kapalı.")?)
        })
        .await
    }
    /// Force an X/Y device ON/OFF in the PLC, or release it. The PLC holds the state.
    pub async fn force(&self, device: Device, mode: opendvp::plc_force::Force) -> Result<(), String> {
        self.with_session(move |s| opendvp::plc_force::force(s, device, mode))
            .await
    }
    /// Read-only: current values of arbitrary bit/word devices on the monitoring session.
    pub async fn read_values(
        &self,
        wanted: Vec<(Device, opendvp::st::DataType)>,
    ) -> Result<Vec<opendvp::plc_monitor::Value>, String> {
        self.with_session(move |s| opendvp::plc_monitor::read_values(s, &wanted))
            .await
    }
    pub async fn release_all_forces(&self) -> Result<(), String> {
        self.with_session(|s| opendvp::plc_force::release_all(s)).await
    }
}
pub async fn connect(config: Config) -> Result<(Client, Sample), String> {
    connect_cancellable(config, Arc::new(AtomicBool::new(false))).await
}
pub async fn connect_cancellable(
    config: Config,
    cancelled: Arc<AtomicBool>,
) -> Result<(Client, Sample), String> {
    config.validate()?;
    background(move || {
        if cancelled.load(Ordering::Acquire) {
            return Err("Bağlantı iptal edildi.".into());
        }
        let mut session = Session::open(config)?;
        if cancelled.load(Ordering::Acquire) {
            return Err("Bağlantı iptal edildi.".into());
        }
        // A connection is only reported successful after both actual reads succeed.
        let sample = session.sample(&cancelled)?;
        if cancelled.load(Ordering::Acquire) {
            return Err("Bağlantı iptal edildi.".into());
        }
        let client = Client {
            session: Arc::new(Mutex::new(Some(session))),
            closed: cancelled,
        };
        Ok((client, sample))
    })
    .await
}

enum Io {
    Tcp(TcpStream),
    Serial(Box<dyn serialport::SerialPort>),
}
impl Io {
    fn timeout(&mut self, timeout: Duration) -> Result<(), String> {
        match self {
            Self::Tcp(s) => {
                s.set_read_timeout(Some(timeout))
                    .map_err(|e| e.to_string())?;
                s.set_write_timeout(Some(timeout))
                    .map_err(|e| e.to_string())
            }
            Self::Serial(s) => s.set_timeout(timeout).map_err(|e| e.to_string()),
        }
    }
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.write(bytes),
            Self::Serial(s) => s.write(bytes),
        }
    }
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.read(bytes),
            Self::Serial(s) => s.read(bytes),
        }
    }
    fn pending_serial(&self) -> Result<u32, String> {
        match self {
            Self::Serial(s) => s.bytes_to_read().map_err(|e| e.to_string()),
            Self::Tcp(_) => Ok(0),
        }
    }
}
struct Session {
    config: Config,
    io: Io,
    sequence: u64,
    transaction: u16,
    last_frame: Instant,
}
impl Session {
    fn open(config: Config) -> Result<Self, String> {
        let timeout = Duration::from_millis(config.timeout_ms);
        let io = if config.transport == Transport::Tcp {
            let ip: IpAddr = config
                .host
                .trim()
                .parse()
                .map_err(|_| "Geçersiz IP adresi")?;
            let stream = TcpStream::connect_timeout(&SocketAddr::new(ip, config.tcp_port), timeout)
                .map_err(|e| format!("TCP bağlantısı kurulamadı: {e}"))?;
            stream.set_nodelay(true).map_err(|e| e.to_string())?;
            Io::Tcp(stream)
        } else {
            let bits = if config.data_bits == 7 {
                serialport::DataBits::Seven
            } else {
                serialport::DataBits::Eight
            };
            let parity = match config.parity {
                Parity::None => serialport::Parity::None,
                Parity::Even => serialport::Parity::Even,
                Parity::Odd => serialport::Parity::Odd,
            };
            let stop = if config.stop_bits == 1 {
                serialport::StopBits::One
            } else {
                serialport::StopBits::Two
            };
            let port = serialport::new(&config.port_name, config.baud)
                .data_bits(bits)
                .parity(parity)
                .stop_bits(stop)
                .flow_control(serialport::FlowControl::None)
                .timeout(timeout)
                .open()
                .map_err(|e| format!("Seri port açılamadı: {e}"))?;
            Io::Serial(port)
        };
        Ok(Self {
            config,
            io,
            sequence: 0,
            transaction: 0,
            last_frame: Instant::now(),
        })
    }
    fn sample(&mut self, closed: &AtomicBool) -> Result<Sample, String> {
        let started = Instant::now();
        let x = self.read_bits(2, 0x0400 + self.config.x_start, self.config.x_count, closed)?;
        let y = self.read_bits(1, 0x0500 + self.config.y_start, self.config.y_count, closed)?;
        let mut values = Vec::with_capacity(x.len() + y.len());
        values.extend(x.into_iter().enumerate().map(|(i, v)| {
            (
                Device {
                    area: Area::X,
                    index: self.config.x_start + i as u16,
                },
                v,
            )
        }));
        values.extend(y.into_iter().enumerate().map(|(i, v)| {
            (
                Device {
                    area: Area::Y,
                    index: self.config.y_start + i as u16,
                },
                v,
            )
        }));
        self.sequence = self.sequence.saturating_add(1);
        Ok(Sample {
            values,
            sequence: self.sequence,
            latency_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        })
    }
    fn gap(&self) -> Duration {
        if self.config.baud > 19_200 {
            return Duration::from_micros(1750);
        }
        let bits = 1
            + u64::from(self.config.data_bits)
            + u64::from(self.config.stop_bits)
            + u64::from(self.config.parity != Parity::None);
        Duration::from_micros((bits * 3_500_000).div_ceil(u64::from(self.config.baud)))
    }
    fn read_bits(
        &mut self,
        function: u8,
        start: u16,
        count: u16,
        closed: &AtomicBool,
    ) -> Result<Vec<bool>, String> {
        let mut pdu = vec![function];
        pdu.extend(start.to_be_bytes());
        pdu.extend(count.to_be_bytes());
        if self.config.protocol == Protocol::Rtu {
            let gap = self.gap();
            if let Some(wait) = gap.checked_sub(self.last_frame.elapsed()) {
                std::thread::sleep(wait);
            }
        }
        if self.config.protocol != Protocol::Tcp && self.io.pending_serial()? != 0 {
            return Err("Beklenmeyen seri veri var; bağlantıyı yeniden açın.".into());
        }
        if closed.load(Ordering::Acquire) {
            return Err("Bağlantı kapatıldı.".into());
        }
        let deadline = Instant::now() + Duration::from_millis(self.config.timeout_ms);
        let (request, tid) = match self.config.protocol {
            Protocol::Tcp => {
                self.transaction = self.transaction.wrapping_add(1);
                let tid = self.transaction;
                let mut out = Vec::from(tid.to_be_bytes());
                out.extend([0, 0, 0, 6, self.config.station]);
                out.extend(&pdu);
                (out, Some(tid))
            }
            Protocol::Rtu => {
                let mut out = vec![self.config.station];
                out.extend(&pdu);
                let crc = crc16(&out);
                out.extend(crc.to_le_bytes());
                (out, None)
            }
            Protocol::Ascii => {
                let mut out = vec![self.config.station];
                out.extend(&pdu);
                (ascii_encode(&out), None)
            }
        };
        write_all(&mut self.io, &request, deadline)?;
        let payload = match self.config.protocol {
            Protocol::Tcp => read_tcp(&mut self.io, deadline, tid.unwrap(), self.config.station)?,
            Protocol::Rtu => read_rtu(&mut self.io, deadline, self.config.station, function)?,
            Protocol::Ascii => read_ascii(&mut self.io, deadline, self.config.station)?,
        };
        self.last_frame = Instant::now();
        if payload.first() == Some(&(function | 0x80)) {
            if payload.len() != 2 {
                return Err("Geçersiz Modbus istisna yanıtı.".into());
            }
            return Err(format!("Modbus istisnası: kod {}", payload[1]));
        }
        let bytes = usize::from(count).div_ceil(8);
        if payload.len() != bytes + 2
            || payload.first() != Some(&function)
            || payload.get(1) != Some(&(bytes as u8))
        {
            return Err("Modbus işlevi veya bit sayısı yanıtla eşleşmiyor.".into());
        }
        Ok((0..count)
            .map(|i| payload[2 + usize::from(i / 8)] & (1 << (i % 8)) != 0)
            .collect())
    }
}
/// Largest reply PDU used by program transfer (FC74 with 99 words plus framing headroom).
const TRANSFER_MAX_PDU: usize = 253;

impl Session {
    /// One request/response exchange with any function code. Used only by the explicit
    /// program-download path; the monitoring path keeps its FC01/FC02-only `read_bits`.
    fn transact(&mut self, function: u8, data: &[u8]) -> Result<Result<Vec<u8>, u8>, String> {
        if self.config.protocol == Protocol::Rtu
            && let Some(wait) = self.gap().checked_sub(self.last_frame.elapsed())
        {
            std::thread::sleep(wait);
        }
        if self.config.protocol != Protocol::Tcp && self.io.pending_serial()? != 0 {
            return Err("Beklenmeyen seri veri var; bağlantıyı yeniden açın.".into());
        }
        let mut pdu = vec![function];
        pdu.extend(data);
        let deadline = Instant::now() + Duration::from_millis(self.config.timeout_ms);
        let station = self.config.station;
        let payload = match self.config.protocol {
            Protocol::Tcp => {
                self.transaction = self.transaction.wrapping_add(1);
                let tid = self.transaction;
                let mut out = Vec::from(tid.to_be_bytes());
                out.extend([0, 0]);
                out.extend(((pdu.len() + 1) as u16).to_be_bytes());
                out.push(station);
                out.extend(&pdu);
                write_all(&mut self.io, &out, deadline)?;
                read_tcp_sized(&mut self.io, deadline, tid, station, TRANSFER_MAX_PDU)?
            }
            Protocol::Rtu => {
                let mut out = vec![station];
                out.extend(&pdu);
                out.extend(crc16(&out).to_le_bytes());
                write_all(&mut self.io, &out, deadline)?;
                read_rtu_any(&mut self.io, deadline, station, function)?
            }
            Protocol::Ascii => {
                let mut out = vec![station];
                out.extend(&pdu);
                write_all(&mut self.io, &ascii_encode(&out), deadline)?;
                read_ascii_sized(&mut self.io, deadline, station, 2 * TRANSFER_MAX_PDU + 7)?
            }
        };
        self.last_frame = Instant::now();
        match payload.first() {
            Some(&f) if f == function | 0x80 && payload.len() == 2 => Ok(Err(payload[1])),
            Some(&f) if f == function => Ok(Ok(payload[1..].to_vec())),
            _ => Err("Modbus yanıtının işlev kodu eşleşmiyor.".into()),
        }
    }
}
impl opendvp::plc_download::PduLink for Session {
    fn transact(&mut self, function: u8, data: &[u8]) -> Result<Result<Vec<u8>, u8>, String> {
        Session::transact(self, function, data)
    }
}

/// Download a compiled program on a dedicated connection (never the monitoring session).
/// The PLC is put in STOP for the transfer; `run_after` restarts it once readback matches.
pub async fn download_program(
    config: Config,
    words: Vec<u16>,
    run_after: bool,
    password: Option<String>,
    plc_id: Option<String>,
    initial: Vec<opendvp::plc_download::Initial>,
) -> Result<opendvp::plc_download::Report, String> {
    config.validate()?;
    background(move || {
        let mut session = Session::open(config)?;
        opendvp::plc_download::download(
            &mut session,
            &words,
            opendvp::plc_download::Options {
                target: opendvp::plc_download::Target::Plc,
                run_after,
                password,
                plc_id,
                initial,
            },
            |_| {},
        )
    })
    .await
}

/// Read-only connection test over the configured link (identity, RUN state, password
/// and PLC-ID status). Nothing is written to the PLC.
pub async fn probe_plc(config: Config) -> Result<opendvp::plc_download::Probe, String> {
    config.validate()?;
    background(move || {
        let mut session = Session::open(config)?;
        opendvp::plc_download::probe(&mut session)
    })
    .await
}

/// Read the program from the PLC up to END and decode it to IL (read only).
pub async fn read_plc_program(config: Config) -> Result<(String, usize), String> {
    config.validate()?;
    background(move || {
        let mut session = Session::open(config)?;
        let identity = opendvp::plc_download::identify(&mut session)?;
        let words = opendvp::plc_download::read_until_end(&mut session, identity.capacity_steps)?;
        let source = opendvp::es2_program_decode::decode(&words)
            .map_err(|e| format!("PLC programı IL'e çözülemedi: {e}"))?;
        Ok((source, words.len()))
    })
    .await
}

/// Read-only status snapshot (RUN, error code, scan times, clock).
pub async fn plc_status(config: Config) -> Result<opendvp::plc_control::Status, String> {
    config.validate()?;
    background(move || {
        let mut session = Session::open(config)?;
        opendvp::plc_control::status(&mut session)
    })
    .await
}

/// Switch the PLC to RUN or STOP on a dedicated connection.
pub async fn plc_set_run(config: Config, run: bool) -> Result<(), String> {
    config.validate()?;
    background(move || {
        let mut session = Session::open(config)?;
        opendvp::plc_control::set_run(&mut session, run)
    })
    .await
}

/// Read the right-side extension modules the PLC reports (D1140/D1142/D1320+). Read only.
pub async fn scan_modules(config: Config) -> Result<opendvp::plc_modules::ModuleScan, String> {
    config.validate()?;
    background(move || {
        let mut session = Session::open(config)?;
        opendvp::plc_modules::scan(&mut session)
    })
    .await
}

fn read_tcp_sized(
    io: &mut Io,
    deadline: Instant,
    tid: u16,
    station: u8,
    max_pdu: usize,
) -> Result<Vec<u8>, String> {
    let mut header = [0u8; 7];
    read_exact(io, &mut header, deadline)?;
    if u16::from_be_bytes([header[0], header[1]]) != tid
        || header[2] != 0
        || header[3] != 0
        || header[6] != station
    {
        return Err("Modbus TCP işlem, protokol veya istasyon kimliği eşleşmiyor.".into());
    }
    let length = usize::from(u16::from_be_bytes([header[4], header[5]]));
    if !(3..=max_pdu + 1).contains(&length) {
        return Err("Modbus TCP yanıt uzunluğu geçersiz.".into());
    }
    let mut pdu = vec![0; length - 1];
    read_exact(io, &mut pdu, deadline)?;
    Ok(pdu)
}
/// RTU reply of any function used by program transfer; the length rule depends on the code.
fn read_rtu_any(
    io: &mut Io,
    deadline: Instant,
    station: u8,
    function: u8,
) -> Result<Vec<u8>, String> {
    let mut raw = vec![0u8; 3];
    read_exact(io, &mut raw, deadline)?;
    if raw[0] != station || (raw[1] != function && raw[1] != (function | 0x80)) {
        return Err("RTU istasyon veya işlev eşleşmiyor.".into());
    }
    let remaining = if raw[1] & 0x80 != 0 {
        2
    } else {
        match function {
            // Echo of address + quantity/value.
            0x05 | 0x06 | 0x10 | 0x75 => 3 + 2,
            // Delta FC64: [type][command][length][data].
            0x64 => {
                let mut more = [0u8; 2];
                read_exact(io, &mut more, deadline)?;
                raw.extend(more);
                usize::from(more[1]) + 2
            }
            // Byte count follows the function code.
            _ => usize::from(raw[2]) + 2,
        }
    };
    if raw.len() + remaining > TRANSFER_MAX_PDU + 3 {
        return Err("RTU yanıt uzunluğu geçersiz.".into());
    }
    let mut tail = vec![0; remaining];
    read_exact(io, &mut tail, deadline)?;
    raw.extend(tail);
    let len = raw.len();
    if crc16(&raw[..len - 2]) != u16::from_le_bytes([raw[len - 2], raw[len - 1]]) {
        return Err("RTU CRC doğrulanamadı.".into());
    }
    Ok(raw[1..len - 2].to_vec())
}
fn read_ascii_sized(
    io: &mut Io,
    deadline: Instant,
    station: u8,
    max: usize,
) -> Result<Vec<u8>, String> {
    let mut frame = Vec::new();
    loop {
        let mut byte = [0];
        read_exact(io, &mut byte, deadline)?;
        frame.push(byte[0]);
        if frame.len() > max {
            return Err("ASCII yanıtı çok uzun.".into());
        }
        if frame.len() == 1 && frame[0] != b':' {
            return Err("ASCII çerçeve başlangıcı geçersiz.".into());
        }
        if frame.ends_with(b"\r\n") {
            break;
        }
    }
    let bytes = ascii_decode(&frame)?;
    if bytes.first() != Some(&station) {
        return Err("ASCII istasyon eşleşmiyor.".into());
    }
    Ok(bytes[1..].to_vec())
}

fn remain(deadline: Instant) -> Result<Duration, String> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| "Modbus yanıtı zaman aşımına uğradı.".into())
}
fn write_all(io: &mut Io, mut bytes: &[u8], deadline: Instant) -> Result<(), String> {
    while !bytes.is_empty() {
        io.timeout(remain(deadline)?)?;
        match io.write(bytes) {
            Ok(0) => return Err("Bağlantı yazma sırasında kapandı.".into()),
            Ok(n) => bytes = &bytes[n..],
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Modbus yazma hatası: {e}")),
        }
    }
    Ok(())
}
fn read_exact(io: &mut Io, mut bytes: &mut [u8], deadline: Instant) -> Result<(), String> {
    while !bytes.is_empty() {
        io.timeout(remain(deadline)?)?;
        match io.read(bytes) {
            Ok(0) => return Err("Bağlantı yanıt tamamlanmadan kapandı.".into()),
            Ok(n) => {
                let (_, rest) = bytes.split_at_mut(n);
                bytes = rest;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Modbus okuma hatası: {e}")),
        }
    }
    Ok(())
}
fn read_tcp(io: &mut Io, deadline: Instant, tid: u16, station: u8) -> Result<Vec<u8>, String> {
    let mut header = [0u8; 7];
    read_exact(io, &mut header, deadline)?;
    if u16::from_be_bytes([header[0], header[1]]) != tid
        || header[2] != 0
        || header[3] != 0
        || header[6] != station
    {
        return Err("Modbus TCP işlem, protokol veya istasyon kimliği eşleşmiyor.".into());
    }
    let length = usize::from(u16::from_be_bytes([header[4], header[5]]));
    if !(3..=35).contains(&length) {
        return Err("Modbus TCP yanıt uzunluğu geçersiz.".into());
    }
    let mut pdu = vec![0; length - 1];
    read_exact(io, &mut pdu, deadline)?;
    Ok(pdu)
}
fn read_rtu(io: &mut Io, deadline: Instant, station: u8, function: u8) -> Result<Vec<u8>, String> {
    let mut head = [0u8; 3];
    read_exact(io, &mut head, deadline)?;
    if head[0] != station || (head[1] != function && head[1] != (function | 0x80)) {
        return Err("RTU istasyon veya işlev eşleşmiyor.".into());
    }
    let remaining = if head[1] & 0x80 != 0 {
        2
    } else {
        usize::from(head[2]) + 2
    };
    if remaining > 34 {
        return Err("RTU yanıt uzunluğu geçersiz.".into());
    }
    let mut raw = head.to_vec();
    let mut tail = vec![0; remaining];
    read_exact(io, &mut tail, deadline)?;
    raw.extend(tail);
    let len = raw.len();
    if crc16(&raw[..len - 2]) != u16::from_le_bytes([raw[len - 2], raw[len - 1]]) {
        return Err("RTU CRC doğrulanamadı.".into());
    }
    Ok(raw[1..len - 2].to_vec())
}
fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for &byte in bytes {
        crc ^= u16::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xa001
            } else {
                crc >> 1
            };
        }
    }
    crc
}
fn ascii_encode(data: &[u8]) -> Vec<u8> {
    let mut bytes = data.to_vec();
    let sum = bytes.iter().fold(0u8, |s, b| s.wrapping_add(*b));
    bytes.push(sum.wrapping_neg());
    let hex = b"0123456789ABCDEF";
    let mut out = vec![b':'];
    for b in bytes {
        out.push(hex[(b >> 4) as usize]);
        out.push(hex[(b & 15) as usize]);
    }
    out.extend(b"\r\n");
    out
}
fn ascii_decode(raw: &[u8]) -> Result<Vec<u8>, String> {
    if raw.len() < 9
        || raw.first() != Some(&b':')
        || !raw.ends_with(b"\r\n")
        || !(raw.len() - 3).is_multiple_of(2)
    {
        return Err("ASCII çerçevesi geçersiz.".into());
    }
    let hex = |c: u8| -> Result<u8, String> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            _ => Err("ASCII hex geçersiz.".into()),
        }
    };
    let mut bytes = Vec::new();
    for pair in raw[1..raw.len() - 2].as_chunks::<2>().0 {
        bytes.push((hex(pair[0])? << 4) | hex(pair[1])?);
    }
    if bytes.iter().fold(0u8, |s, b| s.wrapping_add(*b)) != 0 {
        return Err("ASCII LRC doğrulanamadı.".into());
    }
    bytes.pop();
    Ok(bytes)
}
fn read_ascii(io: &mut Io, deadline: Instant, station: u8) -> Result<Vec<u8>, String> {
    let mut frame = Vec::new();
    loop {
        let mut byte = [0];
        read_exact(io, &mut byte, deadline)?;
        frame.push(byte[0]);
        if frame.len() > 75 {
            return Err("ASCII yanıtı çok uzun.".into());
        }
        if frame.len() == 1 && frame[0] != b':' {
            return Err("ASCII çerçeve başlangıcı geçersiz.".into());
        }
        if frame.ends_with(b"\r\n") {
            break;
        }
    }
    let bytes = ascii_decode(&frame)?;
    if bytes.first() != Some(&station) {
        return Err("ASCII istasyon eşleşmiyor.".into());
    }
    Ok(bytes[1..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    fn block_on<F: Future>(future: F) -> F::Output {
        struct WakeThread(std::thread::Thread);
        impl std::task::Wake for WakeThread {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = std::task::Waker::from(Arc::new(WakeThread(std::thread::current())));
        let mut context = std::task::Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(value) => return value,
                std::task::Poll::Pending => std::thread::park(),
            }
        }
    }

    fn tcp_config(port: u16) -> Config {
        Config {
            transport: Transport::Tcp,
            protocol: Protocol::Tcp,
            host: "127.0.0.1".into(),
            tcp_port: port,
            ..Config::default()
        }
    }
    fn listen() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }
    fn request(s: &mut TcpStream) -> [u8; 12] {
        let mut request = [0; 12];
        s.read_exact(&mut request).unwrap();
        request
    }
    fn response(request: &[u8; 12], value: u16) -> Vec<u8> {
        vec![
            request[0],
            request[1],
            0,
            0,
            0,
            5,
            request[6],
            request[7],
            2,
            value as u8,
            (value >> 8) as u8,
        ]
    }
    #[test]
    fn protocol_vectors_and_validation() {
        assert_eq!(crc16(&[1, 3, 0, 0, 0, 10]), 0xcdc5);
        let frame = ascii_encode(&[1, 2, 4, 0, 0, 16]);
        assert_eq!(frame, b":010204000010E9\r\n");
        assert_eq!(ascii_decode(&frame).unwrap(), vec![1, 2, 4, 0, 0, 16]);
        assert!(ascii_decode(b":010204000010EA\r\n").is_err());
        let mut c = Config::default();
        assert!(c.validate().is_err());
        c.port_name = "COM4".into();
        assert!(c.validate().is_ok());
        c.protocol = Protocol::Rtu;
        assert!(c.validate().is_err());
        c.data_bits = 8;
        assert!(c.validate().is_ok());
        c.x_start = 255;
        c.x_count = 2;
        assert!(c.validate().is_err());
        let mut tcp = tcp_config(502);
        tcp.station = 255;
        assert!(tcp.validate().is_ok());
        tcp.host = "plc.example".into();
        assert!(tcp.validate().is_err());
    }
    #[test]
    fn loopback_tcp_reads_native_values_from_partial_frames_and_disconnects() {
        let (listener, port) = listen();
        let peer = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let x = request(&mut s);
            assert_eq!(&x[7..], &[2, 4, 0, 0, 16]);
            let a = response(&x, 0x8001);
            for chunk in a.chunks(2) {
                s.write_all(chunk).unwrap();
            }
            let y = request(&mut s);
            assert_eq!(&y[7..], &[1, 5, 0, 0, 16]);
            let b = response(&y, 2);
            for chunk in b.chunks(3) {
                s.write_all(chunk).unwrap();
            }
        });
        let (client, sample) = block_on(connect(tcp_config(port))).unwrap();
        assert_eq!(sample.sequence, 1);
        assert_eq!(sample.values.len(), 32);
        assert_eq!(sample.values[0], ("X0".parse().unwrap(), true));
        assert_eq!(sample.values[15], ("X17".parse().unwrap(), true));
        assert_eq!(sample.values[16], ("Y0".parse().unwrap(), false));
        assert_eq!(sample.values[17], ("Y1".parse().unwrap(), true));
        client.disconnect();
        assert!(block_on(client.read()).is_err());
        peer.join().unwrap();
    }
    #[test]
    fn tcp_wrong_station_stale_transaction_exception_and_short_response_fail_closed() {
        for case in 0..4 {
            let (listener, port) = listen();
            let peer = std::thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                let req = request(&mut s);
                match case {
                    0 => {
                        let mut reply = response(&req, 0);
                        reply[6] = 2;
                        s.write_all(&reply).unwrap();
                    }
                    1 => {
                        let mut reply = response(&req, 0);
                        reply[1] = reply[1].wrapping_add(1);
                        s.write_all(&reply).unwrap();
                    }
                    2 => {
                        let reply = [req[0], req[1], 0, 0, 0, 3, req[6], req[7] | 0x80, 2];
                        s.write_all(&reply).unwrap();
                    }
                    _ => {
                        s.write_all(&[req[0], req[1], 0]).unwrap();
                    }
                }
            });
            assert!(block_on(connect(tcp_config(port))).is_err(), "case {case}");
            peer.join().unwrap();
        }
    }

    #[test]
    fn failed_later_read_retires_the_connected_client() {
        let (listener, port) = listen();
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            for _ in 0..2 {
                let req = request(&mut stream);
                stream.write_all(&response(&req, 0)).unwrap();
            }
            let req = request(&mut stream);
            let mut stale = response(&req, 0);
            stale[1] = stale[1].wrapping_sub(1);
            stream.write_all(&stale).unwrap();
        });
        let (client, _) = block_on(connect(tcp_config(port))).unwrap();
        assert!(block_on(client.read()).is_err());
        assert!(block_on(client.read()).is_err());
        assert!(client.closed.load(Ordering::Acquire));
        peer.join().unwrap();
    }

    #[test]
    fn disconnect_during_read_does_not_start_the_second_request() {
        let (listener, port) = listen();
        let (seen_tx, seen_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            for _ in 0..2 {
                let req = request(&mut stream);
                stream.write_all(&response(&req, 0)).unwrap();
            }
            let req = request(&mut stream);
            seen_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            stream.write_all(&response(&req, 0)).unwrap();
            let mut byte = [0];
            assert_eq!(stream.read(&mut byte).unwrap(), 0);
        });
        let (client, _) = block_on(connect(tcp_config(port))).unwrap();
        let reader = client.clone();
        let task = std::thread::spawn(move || block_on(reader.read()));
        seen_rx.recv().unwrap();
        client.disconnect();
        release_tx.send(()).unwrap();
        assert!(task.join().unwrap().is_err());
        peer.join().unwrap();
    }

    #[test]
    fn cancelled_before_open_never_connects() {
        let (listener, port) = listen();
        listener.set_nonblocking(true).unwrap();
        let cancelled = Arc::new(AtomicBool::new(true));
        assert!(block_on(connect_cancellable(tcp_config(port), cancelled)).is_err());
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn cancelled_during_initial_read_never_starts_second_request() {
        let (listener, port) = listen();
        let (seen_tx, seen_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let req = request(&mut stream);
            seen_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            stream.write_all(&response(&req, 0)).unwrap();
            let mut byte = [0];
            assert_eq!(stream.read(&mut byte).unwrap(), 0);
        });
        let cancelled = Arc::new(AtomicBool::new(false));
        let token = cancelled.clone();
        let connecting =
            std::thread::spawn(move || block_on(connect_cancellable(tcp_config(port), token)));
        seen_rx.recv().unwrap();
        cancelled.store(true, Ordering::Release);
        release_tx.send(()).unwrap();
        assert!(connecting.join().unwrap().is_err());
        peer.join().unwrap();
    }

    #[test]
    fn transfer_rtu_replies_use_per_function_lengths() {
        for (function, body) in [
            // FC64 1/0C reply: type, command, length, one data byte.
            (0x64u8, vec![0x64u8, 0x01, 0x0C, 0x01, 0x01]),
            // FC10 echo of address + quantity.
            (0x10, vec![0x10, 0x20, 0x00, 0x00, 0x03]),
            // FC74 byte count + words.
            (0x74, vec![0x74, 0x06, 0x00, 0x24, 0x00, 0xC5, 0x0F, 0x00]),
            // Exception.
            (0x64, vec![0xE4, 0x03]),
        ] {
            let (listener, port) = listen();
            let mut frame = vec![1u8];
            frame.extend(&body);
            frame.extend(crc16(&frame).to_le_bytes());
            let peer = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                for byte in frame {
                    stream.write_all(&[byte]).unwrap();
                }
            });
            let mut io = Io::Tcp(TcpStream::connect(("127.0.0.1", port)).unwrap());
            let reply = read_rtu_any(
                &mut io,
                Instant::now() + Duration::from_secs(2),
                1,
                function,
            );
            assert_eq!(reply.unwrap(), body);
            peer.join().unwrap();
        }
    }

    /// Run with a Modbus TCP endpoint in front of a Delta simulator, e.g.
    /// `bridge_capture.py ... --serve-only`, and OPENDVP_LIVE_TCP_PORT set.
    #[test]
    #[ignore]
    fn live_download_over_modbus_tcp() {
        let port: u16 = std::env::var("OPENDVP_LIVE_TCP_PORT")
            .unwrap()
            .parse()
            .unwrap();
        let report = block_on(download_program(
            tcp_config(port),
            vec![0x0024, 0x00C5, 0x0F00],
            true,
            None,
            None,
            Vec::new(),
        ));
        eprintln!("{report:?}");
        assert!(report.unwrap().running);
    }

    /// Serial framing (ASCII/RTU) exercised end to end over a byte stream, without a UART.
    /// ASCII: OPENDVP_LIVE_ASCII_PORT = an owned Delta simulator (it speaks Modbus ASCII
    /// over TCP). RTU: OPENDVP_LIVE_RTU_PORT = `bridge_capture.py --rtu --serve-only`.
    fn live_serial_framing(protocol: Protocol, variable: &str) {
        let port: u16 = std::env::var(variable).unwrap().parse().unwrap();
        let config = Config {
            protocol,
            data_bits: if protocol == Protocol::Rtu { 8 } else { 7 },
            ..Config::default()
        };
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_nodelay(true).unwrap();
        let mut session = Session {
            config,
            io: Io::Tcp(stream),
            sequence: 0,
            transaction: 0,
            last_frame: Instant::now(),
        };
        let options = opendvp::plc_download::Options {
            target: opendvp::plc_download::Target::Simulator,
            run_after: true,
            ..Default::default()
        };
        let report = opendvp::plc_download::download(
            &mut session,
            &[0x0024, 0x00C5, 0x0F00],
            options,
            |_| {},
        )
        .unwrap();
        eprintln!("{protocol:?}: {report:?}");
        assert!(report.running);
        let scan = opendvp::plc_modules::scan(&mut session).unwrap();
        assert!(scan.types.is_empty());
        // Delta FC64 exception path through the serial framing.
        assert!(
            opendvp::plc_modules::read_cr(&mut session, 0, 0)
                .unwrap_err()
                .contains("0x03")
        );
    }
    #[test]
    #[ignore]
    fn live_download_over_serial_ascii_framing() {
        live_serial_framing(Protocol::Ascii, "OPENDVP_LIVE_ASCII_PORT");
    }
    #[test]
    #[ignore]
    fn live_download_over_serial_rtu_framing() {
        live_serial_framing(Protocol::Rtu, "OPENDVP_LIVE_RTU_PORT");
    }

    #[test]
    fn rtu_reply_validates_crc_and_exception_frames() {
        for corrupt in [false, true] {
            let (listener, port) = listen();
            let peer = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut frame = vec![1, 2, 2, 1, 128];
                let crc = crc16(&frame);
                frame.extend(crc.to_le_bytes());
                if corrupt {
                    frame[5] ^= 1;
                }
                for byte in frame {
                    stream.write_all(&[byte]).unwrap();
                }
            });
            let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let mut io = Io::Tcp(stream);
            let result = read_rtu(&mut io, Instant::now() + Duration::from_secs(2), 1, 2);
            if corrupt {
                assert!(result.is_err());
            } else {
                assert_eq!(result.unwrap(), vec![2, 2, 1, 128]);
            }
            peer.join().unwrap();
        }
        let (listener, port) = listen();
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut frame = vec![1, 0x82, 2];
            frame.extend(crc16(&frame).to_le_bytes());
            stream.write_all(&frame).unwrap();
        });
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut io = Io::Tcp(stream);
        assert_eq!(
            read_rtu(&mut io, Instant::now() + Duration::from_secs(2), 1, 2).unwrap(),
            vec![0x82, 2]
        );
        peer.join().unwrap();
    }

    #[test]
    fn ascii_reply_handles_fragmented_stream_and_checks_station() {
        for station in [1, 2] {
            let (listener, port) = listen();
            let peer = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                for byte in ascii_encode(&[station, 2, 2, 1, 128]) {
                    stream.write_all(&[byte]).unwrap();
                }
            });
            let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let mut io = Io::Tcp(stream);
            let result = read_ascii(&mut io, Instant::now() + Duration::from_secs(2), 1);
            if station == 1 {
                assert_eq!(result.unwrap(), vec![2, 2, 1, 128]);
            } else {
                assert!(result.is_err());
            }
            peer.join().unwrap();
        }
    }
}

/// Force an X/Y device ON/OFF or release it, on a dedicated connection (FC64 `01 30 03`).
#[allow(dead_code)]
pub async fn plc_force_device(
    config: Config,
    device: Device,
    mode: opendvp::plc_force::Force,
) -> Result<(), String> {
    config.validate()?;
    background(move || {
        let mut session = Session::open(config)?;
        opendvp::plc_force::force(&mut session, device, mode)
    })
    .await
}

/// Release every forced device, on a dedicated connection (FC64 `01 30 00`).
#[allow(dead_code)]
pub async fn plc_release_forces(config: Config) -> Result<(), String> {
    config.validate()?;
    background(move || {
        let mut session = Session::open(config)?;
        opendvp::plc_force::release_all(&mut session)
    })
    .await
}
