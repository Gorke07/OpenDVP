//! Bounded Modbus ASCII transport for the locally installed ES2 simulator.
//! No discovery, PLC connection, or program encoding is performed here.
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, SocketAddrV4, TcpStream};
use std::time::{Duration, Instant};

pub const MAX_FRAME_BYTES: usize = 426;
const MAX_DATA_BYTES: usize = (MAX_FRAME_BYTES - 3) / 2 - 3;
const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub station: u8,
    pub function: u8,
    pub data: Vec<u8>,
}

pub fn encode(station: u8, function: u8, data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() > MAX_DATA_BYTES {
        return Err(format!(
            "ES2 request exceeds the {MAX_FRAME_BYTES}-byte frame limit"
        ));
    }
    let mut decoded = Vec::with_capacity(data.len() + 3);
    decoded.extend_from_slice(&[station, function]);
    decoded.extend_from_slice(data);
    let sum = decoded.iter().fold(0u8, |sum, b| sum.wrapping_add(*b));
    decoded.push(sum.wrapping_neg());
    let mut frame = Vec::with_capacity(decoded.len() * 2 + 3);
    frame.push(b':');
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in decoded {
        frame.push(HEX[usize::from(byte >> 4)]);
        frame.push(HEX[usize::from(byte & 15)]);
    }
    frame.extend_from_slice(b"\r\n");
    Ok(frame)
}

pub fn decode(frame: &[u8]) -> Result<Frame, String> {
    if !(9..=MAX_FRAME_BYTES).contains(&frame.len()) {
        return Err("ES2 response has an invalid frame length".into());
    }
    if frame.first() != Some(&b':') || !frame.ends_with(b"\r\n") {
        return Err("ES2 response must begin with ':' and end with CRLF".into());
    }
    let hex = &frame[1..frame.len() - 2];
    if !hex.len().is_multiple_of(2) {
        return Err("ES2 response contains an incomplete hex byte".into());
    }
    let nibble = |b: u8| match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err("ES2 response contains a non-uppercase-hex character".to_owned()),
    };
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_chunks::<2>().0 {
        bytes.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
    }
    if bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)) != 0 {
        return Err("ES2 response LRC mismatch".into());
    }
    Ok(Frame {
        station: bytes[0],
        function: bytes[1],
        data: bytes[2..bytes.len() - 1].to_vec(),
    })
}

/// An owned connection; `&mut self` serializes requests and prevents overlapping I/O.
/// Framing, transport and response-identity errors retire the connection so that a
/// late/stale response cannot silently satisfy a subsequent command.
#[derive(Debug)]
pub struct Client {
    stream: TcpStream,
    station: u8,
    faulted: bool,
}

impl Client {
    pub fn connect(port: u16, station: u8) -> Result<Self, String> {
        Self::connect_addr(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port), station)
    }

    /// Explicit addresses are useful for tests; non-loopback addresses are rejected.
    pub fn connect_addr(address: SocketAddrV4, station: u8) -> Result<Self, String> {
        if !address.ip().is_loopback() {
            return Err("Only local IPv4 loopback ES2 simulator connections are allowed".into());
        }
        if !(1..=247).contains(&station) {
            return Err("ES2 station must be 1..247; broadcasts have no response".into());
        }
        let stream = TcpStream::connect_timeout(&SocketAddr::V4(address), TIMEOUT)
            .map_err(|e| format!("ES2 connection failed: {e}"))?;
        stream
            .set_read_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        stream.set_nodelay(true).map_err(|e| e.to_string())?;
        Ok(Self {
            stream,
            station,
            faulted: false,
        })
    }

    fn retire(&mut self) {
        self.faulted = true;
        let _ = self.stream.shutdown(Shutdown::Both);
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, String> {
        let deadline = Instant::now() + TIMEOUT;
        let remaining = || {
            deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or_else(|| "ES2 request deadline exceeded".to_owned())
        };
        let mut sent = 0;
        while sent < request.len() {
            self.stream
                .set_write_timeout(Some(remaining()?))
                .map_err(|e| e.to_string())?;
            match self.stream.write(&request[sent..]) {
                Ok(0) => return Err("ES2 connection closed during write".into()),
                Ok(n) => sent += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("ES2 write failed: {e}")),
            }
        }
        let mut response = Vec::with_capacity(MAX_FRAME_BYTES);
        let mut buf = [0u8; MAX_FRAME_BYTES + 1];
        loop {
            self.stream
                .set_read_timeout(Some(remaining()?))
                .map_err(|e| e.to_string())?;
            let available = MAX_FRAME_BYTES + 1 - response.len();
            let n = match self.stream.read(&mut buf[..available]) {
                Ok(0) => return Err("ES2 connection closed before a complete response".into()),
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("ES2 read failed: {e}")),
            };
            response.extend_from_slice(&buf[..n]);
            if response.len() > MAX_FRAME_BYTES {
                return Err("ES2 response exceeds the frame limit".into());
            }
            if let Some(end) = response.windows(2).position(|b| b == b"\r\n") {
                if end + 2 != response.len() {
                    return Err("Unsolicited/coalesced data follows the ES2 response".into());
                }
                return Ok(response);
            }
        }
    }

    pub fn request(&mut self, function: u8, data: &[u8]) -> Result<Vec<u8>, String> {
        self.request_reply(function, data)?
            .map_err(|code| format!("ES2 exception 0x{code:02X} for function 0x{function:02X}"))
    }

    /// Like [`Client::request`], but a well-formed Modbus exception is returned as
    /// `Ok(Err(code))` so callers can tell an unsupported command from a broken link.
    pub fn request_reply(
        &mut self,
        function: u8,
        data: &[u8],
    ) -> Result<Result<Vec<u8>, u8>, String> {
        if self.faulted {
            return Err(
                "ES2 connection is faulted; reconnect before sending another request".into(),
            );
        }
        if !(1..=127).contains(&function) {
            return Err("ES2 request function must be 1..127".into());
        }
        let frame = encode(self.station, function, data)?;
        let response = self.exchange(&frame).and_then(|raw| decode(&raw));
        let response = match response {
            Ok(response) => response,
            Err(e) => {
                self.retire();
                return Err(e);
            }
        };
        if response.station != self.station {
            self.retire();
            return Err(format!(
                "ES2 station mismatch: expected {}, received {}",
                self.station, response.station
            ));
        }
        if response.function == function | 0x80 {
            if response.data.len() != 1 {
                self.retire();
                return Err("Malformed ES2 exception response".into());
            }
            return Ok(Err(response.data[0]));
        }
        if response.function != function {
            self.retire();
            return Err(format!(
                "ES2 function mismatch: expected 0x{function:02X}, received 0x{:02X}",
                response.function
            ));
        }
        Ok(Ok(response.data))
    }

    /// Explicit private simulator RTC snapshot, in year/weekday/month/day/h/m/s
    /// order. This reads an external clock input; it does not freeze that clock.
    pub fn read_rtc_shadow(&mut self) -> Result<[u8; 7], String> {
        let out = self.request(0x64, &[0, 0x1d, 0])?;
        match decode_rtc_shadow(&out) {
            Ok(fields) => Ok(fields),
            Err(reason) => {
                self.retire();
                Err(reason)
            }
        }
    }
    /// Set only the owned simulator's private RTC, never Windows system time.
    /// Raw bytes deliberately preserve the observed simulator protocol domain.
    pub fn write_rtc_shadow(&mut self, fields: [u8; 7]) -> Result<(), String> {
        let data = rtc_seed_payload(fields);
        if self.request(0x64, &data)? != data {
            self.retire();
            return Err("ES2 RTC seed echo mismatch".into());
        }
        Ok(())
    }
    pub fn read_bits(&mut self, address: u16, quantity: u16) -> Result<Vec<bool>, String> {
        self.read_bit_area(0x01, address, quantity)
    }

    /// The original simulator reads the X input area through function 02.
    pub fn read_inputs(&mut self, address: u16, quantity: u16) -> Result<Vec<bool>, String> {
        self.read_bit_area(0x02, address, quantity)
    }

    fn read_bit_area(
        &mut self,
        function: u8,
        address: u16,
        quantity: u16,
    ) -> Result<Vec<bool>, String> {
        let byte_count = usize::from(quantity).div_ceil(8);
        if quantity == 0 || quantity > 2000 || byte_count + 1 > MAX_DATA_BYTES {
            return Err("Bit quantity exceeds the ES2 response frame limit (1..1656)".into());
        }
        checked_range(address, usize::from(quantity))?;
        let response = self.request(function, &range_payload(address, quantity))?;
        if response.len() != byte_count + 1 || usize::from(response[0]) != byte_count {
            self.retire();
            return Err("ES2 bit response byte count does not match the requested quantity".into());
        }
        Ok((0..usize::from(quantity))
            .map(|i| response[1 + i / 8] & (1 << (i % 8)) != 0)
            .collect())
    }

    pub fn read_words(&mut self, address: u16, quantity: u16) -> Result<Vec<u16>, String> {
        let byte_count = usize::from(quantity) * 2;
        if quantity == 0 || quantity > 125 || byte_count + 1 > MAX_DATA_BYTES {
            return Err("Word quantity exceeds the ES2 response frame limit (1..103)".into());
        }
        checked_range(address, usize::from(quantity))?;
        let response = self.request(0x03, &range_payload(address, quantity))?;
        if response.len() != byte_count + 1 || usize::from(response[0]) != byte_count {
            self.retire();
            return Err(
                "ES2 word response byte count does not match the requested quantity".into(),
            );
        }
        Ok(response[1..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect())
    }

    pub fn write_bit(&mut self, address: u16, value: bool) -> Result<(), String> {
        let data = range_payload(address, if value { 0xFF00 } else { 0 });
        let response = self.request(0x05, &data)?;
        if response != data {
            self.retire();
            return Err("ES2 write-bit echo mismatch".into());
        }
        Ok(())
    }

    pub fn write_words(&mut self, address: u16, values: &[u16]) -> Result<(), String> {
        if values.is_empty() || values.len() > 123 || 5 + values.len() * 2 > MAX_DATA_BYTES {
            return Err("Write quantity exceeds the ES2 request frame limit (1..101 words)".into());
        }
        checked_range(address, values.len())?;
        let quantity = values.len() as u16;
        let expected = range_payload(address, quantity);
        let mut data = expected.to_vec();
        data.push((values.len() * 2) as u8);
        for value in values {
            data.extend_from_slice(&value.to_be_bytes());
        }
        if self.request(0x10, &data)? != expected {
            self.retire();
            return Err("ES2 write-words address/quantity echo mismatch".into());
        }
        Ok(())
    }
}

fn range_payload(address: u16, quantity: u16) -> [u8; 4] {
    let a = address.to_be_bytes();
    let q = quantity.to_be_bytes();
    [a[0], a[1], q[0], q[1]]
}
pub(crate) fn rtc_seed_payload(fields: [u8; 7]) -> Vec<u8> {
    [0, 0x11, 0]
        .into_iter()
        .chain(fields.into_iter().rev())
        .collect()
}
pub(crate) fn decode_rtc_shadow(data: &[u8]) -> Result<[u8; 7], String> {
    if data.len() != 10 || data[..3] != [0, 0x1d, 0] {
        return Err("ES2 RTC shadow response shape mismatch".into());
    }
    Ok(std::array::from_fn(|i| data[9 - i]))
}
fn checked_range(address: u16, quantity: usize) -> Result<(), String> {
    if quantity > usize::from(u16::MAX) + 1 - usize::from(address) {
        Err("ES2 address range wraps past 0xFFFF".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rtc_private_shadow_has_exact_wire_order_and_validates_reply_shape() {
        use super::*;
        let fields = [24, 4, 2, 29, 23, 59, 58];
        let (mut client, worker) = server(|request| {
            assert_eq!(request.function, 0x64);
            assert_eq!(request.data, [0, 0x1d, 0]);
            vec![encode(1, 0x64, &[0, 0x1d, 0, 58, 59, 23, 29, 2, 4, 24]).unwrap()]
        });
        assert_eq!(client.read_rtc_shadow().unwrap(), fields);
        worker.join().unwrap();
        let (mut client, worker) = server(|request| {
            assert_eq!(request.data, [0, 0x11, 0, 58, 59, 23, 29, 2, 4, 24]);
            vec![encode(1, 0x64, &request.data).unwrap()]
        });
        client.write_rtc_shadow(fields).unwrap();
        worker.join().unwrap();
        assert!(decode_rtc_shadow(&[0, 0x1d, 0, 1]).is_err());
        assert!(decode_rtc_shadow(&[0, 0x11, 0, 1, 2, 3, 4, 5, 6, 7]).is_err());
    }
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn server(
        reply: impl FnOnce(Frame) -> Vec<Vec<u8>> + Send + 'static,
    ) -> (Client, thread::JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(TIMEOUT)).unwrap();
            let mut raw = Vec::new();
            while !raw.ends_with(b"\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                raw.push(byte[0]);
            }
            let request = decode(&raw).unwrap();
            for chunk in reply(request) {
                stream.write_all(&chunk).unwrap();
                thread::sleep(Duration::from_millis(2));
            }
        });
        (Client::connect(port, 1).unwrap(), worker)
    }

    #[test]
    fn known_lrc_frame_and_roundtrip() {
        let raw = encode(1, 3, &[0, 0, 0, 1]).unwrap();
        assert_eq!(raw, b":010300000001FB\r\n");
        assert_eq!(
            decode(&raw).unwrap(),
            Frame {
                station: 1,
                function: 3,
                data: vec![0, 0, 0, 1]
            }
        );
    }
    #[test]
    fn malformed_frames_are_rejected() {
        for raw in [
            &b":010300000001FA\r\n"[..],
            b":0103fb\r\n",
            b"010300000001FB\r\n",
            b":010300000001FB\n",
            b":010300000001F\r\n",
            b":0103GG\r\n",
            b":010300000001FB\r\nextra",
        ] {
            assert!(decode(raw).is_err(), "{raw:?}");
        }
        assert!(encode(1, 1, &[0; MAX_DATA_BYTES + 1]).is_err());
        let largest = encode(1, 1, &[0; MAX_DATA_BYTES]).unwrap();
        assert!(largest.len() <= MAX_FRAME_BYTES);
        decode(&largest).unwrap();
    }
    #[test]
    fn rejects_nonlocal_and_broadcast_without_connecting() {
        assert!(
            Client::connect_addr(SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 1), 502), 1).is_err()
        );
        assert!(Client::connect(1, 0).is_err());
    }
    #[test]
    fn fragmented_read_words_uses_big_endian_values() {
        let (mut client, worker) = server(|request| {
            assert_eq!(request.function, 3);
            assert_eq!(request.data, [0x10, 0, 0, 2]);
            encode(1, 3, &[4, 0x7f, 0xff, 0x80, 0])
                .unwrap()
                .chunks(3)
                .map(<[u8]>::to_vec)
                .collect()
        });
        assert_eq!(client.read_words(0x1000, 2).unwrap(), [0x7fff, 0x8000]);
        worker.join().unwrap();
    }
    #[test]
    fn bit_order_and_exact_count_are_checked() {
        let (mut client, worker) = server(|_| vec![encode(1, 1, &[2, 0x81, 0x01]).unwrap()]);
        assert_eq!(
            client.read_bits(0, 9).unwrap(),
            [true, false, false, false, false, false, false, true, true]
        );
        worker.join().unwrap();
        let (mut client, worker) = server(|_| vec![encode(1, 3, &[2, 0, 1]).unwrap()]);
        assert!(client.read_words(0, 2).unwrap_err().contains("byte count"));
        assert!(client.request(3, &[]).unwrap_err().contains("faulted"));
        worker.join().unwrap();
    }
    #[test]
    fn inputs_use_function_02() {
        let (mut client, worker) = server(|request| {
            assert_eq!(request.function, 2);
            assert_eq!(request.data, [0x04, 0, 0, 2]);
            vec![encode(1, 2, &[1, 2]).unwrap()]
        });
        assert_eq!(client.read_inputs(0x0400, 2).unwrap(), [false, true]);
        worker.join().unwrap();
    }
    #[test]
    fn mismatched_identity_and_exceptions_are_rejected() {
        for (station, function, data) in [
            (2, 3, vec![2, 0, 1]),
            (1, 4, vec![2, 0, 1]),
            (1, 0x83, vec![2]),
        ] {
            let (mut client, worker) =
                server(move |_| vec![encode(station, function, &data).unwrap()]);
            assert!(client.read_words(0, 1).is_err());
            worker.join().unwrap();
        }
    }
    #[test]
    fn coalesced_unsolicited_frames_are_rejected() {
        let (mut client, worker) = server(|_| {
            let mut raw = encode(1, 3, &[2, 0, 1]).unwrap();
            raw.extend(encode(1, 3, &[2, 0, 2]).unwrap());
            vec![raw]
        });
        assert!(client.read_words(0, 1).unwrap_err().contains("coalesced"));
        worker.join().unwrap();
    }
    #[test]
    fn write_echoes_and_address_ranges_are_checked() {
        let (mut client, worker) = server(|request| {
            assert_eq!(request.function, 5);
            assert_eq!(request.data, [0x08, 0, 0xff, 0]);
            vec![encode(1, 5, &request.data).unwrap()]
        });
        client.write_bit(0x0800, true).unwrap();
        worker.join().unwrap();
        let (mut client, worker) = server(|request| {
            assert_eq!(request.function, 0x10);
            assert_eq!(request.data, [0x10, 0, 0, 2, 4, 0x12, 0x34, 0xab, 0xcd]);
            vec![encode(1, 0x10, &request.data[..4]).unwrap()]
        });
        client.write_words(0x1000, &[0x1234, 0xabcd]).unwrap();
        worker.join().unwrap();
        assert!(checked_range(0xffff, 2).is_err());
        assert!(checked_range(0xffff, 1).is_ok());
    }
}
