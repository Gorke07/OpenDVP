//! Modbus serial framing and an explicitly configured virtual slave.
//!
//! These are whole frames: byte arrival times, timeouts and PLC device packing
//! belong to `serial_io`. This is separate from the WPL simulator control link.
//! Wire limits follow Modbus Application Protocol V1.1b3 and Serial Line V1.02.
//! The strict local profile rejects nonzero unused coil padding bits; the
//! application specification says those bits SHOULD be zero.
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireMode {
    Ascii,
    Rtu,
    /// Explicit Delta module option; standard Modbus sends CRC low byte first.
    RtuSwappedCrc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub station: u8,
    pub function: u8,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Bits(Vec<bool>),
    Registers(Vec<u16>),
    WriteAck,
    Exception(u8),
}

fn crc(bytes: &[u8]) -> u16 {
    let mut result = 0xffffu16;
    for byte in bytes {
        result ^= u16::from(*byte);
        for _ in 0..8 {
            result = (result >> 1) ^ if result & 1 != 0 { 0xa001 } else { 0 };
        }
    }
    result
}

pub fn encode(mode: WireMode, frame: &Frame) -> Result<Vec<u8>, String> {
    if frame.data.len() > 252 {
        return Err("Modbus PDU exceeds 253 bytes".into());
    }
    let mut payload = vec![frame.station, frame.function];
    payload.extend_from_slice(&frame.data);
    match mode {
        WireMode::Ascii => {
            payload.push(0u8.wrapping_sub(payload.iter().fold(0u8, |a, b| a.wrapping_add(*b))));
            let mut out = Vec::with_capacity(payload.len() * 2 + 3);
            out.push(b':');
            for byte in payload {
                out.push(b"0123456789ABCDEF"[(byte >> 4) as usize]);
                out.push(b"0123456789ABCDEF"[(byte & 15) as usize]);
            }
            out.extend_from_slice(b"\r\n");
            Ok(out)
        }
        WireMode::Rtu | WireMode::RtuSwappedCrc => {
            let check = crc(&payload);
            payload.extend_from_slice(&if mode == WireMode::Rtu {
                check.to_le_bytes()
            } else {
                check.to_be_bytes()
            });
            Ok(payload)
        }
    }
}

pub fn decode(mode: WireMode, bytes: &[u8]) -> Result<Frame, String> {
    let payload = match mode {
        WireMode::Ascii => {
            if !(9..=513).contains(&bytes.len())
                || bytes.first() != Some(&b':')
                || !bytes.ends_with(b"\r\n")
                || !(bytes.len() - 3).is_multiple_of(2)
            {
                return Err("Invalid ASCII frame length or delimiters".into());
            }
            let nibble = |b: u8| match b {
                b'0'..=b'9' => Ok(b - b'0'),
                b'A'..=b'F' => Ok(b - b'A' + 10),
                _ => Err("ASCII frames require uppercase hexadecimal digits".to_owned()),
            };
            let mut decoded = Vec::with_capacity((bytes.len() - 3) / 2);
            for pair in bytes[1..bytes.len() - 2].as_chunks::<2>().0 {
                decoded.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
            }
            if decoded.iter().fold(0u8, |a, b| a.wrapping_add(*b)) != 0 {
                return Err("ASCII LRC mismatch".into());
            }
            decoded.pop();
            decoded
        }
        WireMode::Rtu | WireMode::RtuSwappedCrc => {
            if !(4..=256).contains(&bytes.len()) {
                return Err("Invalid RTU frame length".into());
            }
            let end = bytes.len() - 2;
            let check = if mode == WireMode::Rtu {
                u16::from_le_bytes([bytes[end], bytes[end + 1]])
            } else {
                u16::from_be_bytes([bytes[end], bytes[end + 1]])
            };
            if crc(&bytes[..end]) != check {
                return Err("RTU CRC mismatch".into());
            }
            bytes[..end].to_vec()
        }
    };
    Ok(Frame {
        station: payload[0],
        function: payload[1],
        data: payload[2..].to_vec(),
    })
}

fn word(bytes: &[u8], index: usize) -> u16 {
    u16::from_be_bytes([bytes[index], bytes[index + 1]])
}

fn span(address: u16, count: u16, max: u16) -> Result<(), u8> {
    if count == 0 || count > max {
        return Err(3);
    }
    if u32::from(address) + u32::from(count) > 65536 {
        return Err(2);
    }
    Ok(())
}

fn unpack_bits(bytes: &[u8], count: u16) -> Result<Vec<bool>, u8> {
    if bytes.len() != usize::from(count).div_ceil(8) {
        return Err(3);
    }
    let used = count % 8;
    if used != 0 && bytes.last().is_some_and(|last| last >> used != 0) {
        return Err(3);
    }
    Ok((0..usize::from(count))
        .map(|i| bytes[i / 8] & (1 << (i % 8)) != 0)
        .collect())
}

fn pack_bits(bits: &[bool]) -> Vec<u8> {
    let mut bytes = vec![0u8; bits.len().div_ceil(8)];
    for (i, bit) in bits.iter().enumerate() {
        if *bit {
            bytes[i / 8] |= 1 << (i % 8);
        }
    }
    bytes
}

enum Request {
    ReadBits {
        start: u16,
        count: u16,
    },
    ReadWords {
        start: u16,
        count: u16,
    },
    WriteBits {
        start: u16,
        values: Vec<bool>,
    },
    WriteWords {
        start: u16,
        values: Vec<u16>,
    },
    ReadWrite {
        read_start: u16,
        read_count: u16,
        write_start: u16,
        values: Vec<u16>,
    },
}

fn request(frame: &Frame) -> Result<Request, u8> {
    use Request::*;
    let b = &frame.data;
    match frame.function {
        1..=6 => {
            if b.len() != 4 {
                return Err(3);
            }
            let start = word(b, 0);
            let arg = word(b, 2);
            Ok(match frame.function {
                1 | 2 => {
                    span(start, arg, 2000)?;
                    ReadBits { start, count: arg }
                }
                3 | 4 => {
                    span(start, arg, 125)?;
                    ReadWords { start, count: arg }
                }
                5 => WriteBits {
                    start,
                    values: vec![match arg {
                        0 => false,
                        0xff00 => true,
                        _ => return Err(3),
                    }],
                },
                _ => WriteWords {
                    start,
                    values: vec![arg],
                },
            })
        }
        15 | 16 => {
            if b.len() < 5 || usize::from(b[4]) != b.len() - 5 {
                return Err(3);
            }
            let start = word(b, 0);
            let count = word(b, 2);
            span(start, count, if frame.function == 15 { 1968 } else { 123 })?;
            if frame.function == 15 {
                Ok(WriteBits {
                    start,
                    values: unpack_bits(&b[5..], count)?,
                })
            } else {
                if usize::from(count) * 2 != b.len() - 5 {
                    return Err(3);
                }
                Ok(WriteWords {
                    start,
                    values: b[5..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|x| word(x, 0))
                        .collect(),
                })
            }
        }
        23 => {
            if b.len() < 9 || usize::from(b[8]) != b.len() - 9 {
                return Err(3);
            }
            let read_start = word(b, 0);
            let read_count = word(b, 2);
            let write_start = word(b, 4);
            let write_count = word(b, 6);
            span(read_start, read_count, 125)?;
            span(write_start, write_count, 121)?;
            if usize::from(write_count) * 2 != b.len() - 9 {
                return Err(3);
            }
            Ok(ReadWrite {
                read_start,
                read_count,
                write_start,
                values: b[9..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|x| word(x, 0))
                    .collect(),
            })
        }
        _ => Err(1),
    }
}

pub fn validate_response(request_frame: &Frame, response: &Frame) -> Result<Reply, String> {
    let parsed = request(request_frame).map_err(|e| format!("Invalid request (exception {e})"))?;
    if request_frame.station == 0 || response.station != request_frame.station {
        return Err("Response station mismatch or response to broadcast".into());
    }
    if response.function == request_frame.function | 0x80 {
        if response.data.len() != 1 || response.data[0] == 0 {
            return Err("Invalid exception response".into());
        }
        return Ok(Reply::Exception(response.data[0]));
    }
    if response.function != request_frame.function {
        return Err("Response function mismatch".into());
    }
    match parsed {
        Request::ReadBits { count, .. } => {
            if response.data.first().map(|n| usize::from(*n))
                != Some(response.data.len().saturating_sub(1))
            {
                return Err("Bit response byte count mismatch".into());
            }
            unpack_bits(&response.data[1..], count)
                .map(Reply::Bits)
                .map_err(|_| "Invalid bit response count or padding".into())
        }
        Request::ReadWords { count, .. }
        | Request::ReadWrite {
            read_count: count, ..
        } => {
            if response.data.len() != usize::from(count) * 2 + 1
                || response.data.first().copied() != Some((count * 2) as u8)
            {
                return Err("Register response byte count mismatch".into());
            }
            Ok(Reply::Registers(
                response.data[1..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|x| word(x, 0))
                    .collect(),
            ))
        }
        Request::WriteBits { .. } | Request::WriteWords { .. } => {
            if response.data != request_frame.data[..4] {
                return Err("Write acknowledgement does not echo address and value/count".into());
            }
            Ok(Reply::WriteAck)
        }
    }
}

/// A sparse, explicit fixture. Unmapped addresses produce exception 02;
/// they never acquire invented zero values. No OS port or hardware is opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualModbusPeer {
    station: u8,
    pub coils: BTreeMap<u16, bool>,
    pub discrete_inputs: BTreeMap<u16, bool>,
    pub holding: BTreeMap<u16, u16>,
    pub input_registers: BTreeMap<u16, u16>,
}

impl VirtualModbusPeer {
    pub fn new(station: u8) -> Result<Self, String> {
        if !(1..=247).contains(&station) {
            return Err("Virtual slave station must be 1..247".into());
        }
        Ok(Self {
            station,
            coils: BTreeMap::new(),
            discrete_inputs: BTreeMap::new(),
            holding: BTreeMap::new(),
            input_registers: BTreeMap::new(),
        })
    }

    pub fn station(&self) -> u8 {
        self.station
    }

    pub fn respond(&mut self, mode: WireMode, wire: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let frame = decode(mode, wire)?;
        let broadcast = frame.station == 0;
        if frame.station != self.station && !broadcast {
            return Ok(None);
        }
        if broadcast && !matches!(frame.function, 5 | 6 | 15 | 16) {
            return Ok(None);
        }
        let result = request(&frame).and_then(|req| self.apply(frame.function, req));
        if broadcast {
            return Ok(None);
        }
        let response = match result {
            Ok(data) => Frame { data, ..frame },
            Err(code) => Frame {
                function: frame.function | 0x80,
                data: vec![code],
                ..frame
            },
        };
        encode(mode, &response).map(Some)
    }

    fn apply(&mut self, function: u8, req: Request) -> Result<Vec<u8>, u8> {
        use Request::*;
        match req {
            ReadBits { start, count } => {
                let source = if function == 1 {
                    &self.coils
                } else {
                    &self.discrete_inputs
                };
                let packed = pack_bits(&read_map(source, start, count)?);
                let mut out = vec![packed.len() as u8];
                out.extend_from_slice(&packed);
                Ok(out)
            }
            ReadWords { start, count } => {
                let source = if function == 3 {
                    &self.holding
                } else {
                    &self.input_registers
                };
                Ok(register_reply(&read_map(source, start, count)?))
            }
            WriteBits { start, values } => {
                preflight(&self.coils, start, values.len() as u16)?;
                for (i, value) in values.iter().enumerate() {
                    self.coils.insert(start + i as u16, *value);
                }
                let arg = if function == 5 {
                    if values[0] { 0xff00 } else { 0 }
                } else {
                    values.len() as u16
                };
                Ok(ack(start, arg))
            }
            WriteWords { start, values } => {
                preflight(&self.holding, start, values.len() as u16)?;
                for (i, value) in values.iter().enumerate() {
                    self.holding.insert(start + i as u16, *value);
                }
                Ok(ack(
                    start,
                    if function == 6 {
                        values[0]
                    } else {
                        values.len() as u16
                    },
                ))
            }
            ReadWrite {
                read_start,
                read_count,
                write_start,
                values,
            } => {
                preflight(&self.holding, read_start, read_count)?;
                preflight(&self.holding, write_start, values.len() as u16)?;
                for (i, value) in values.iter().enumerate() {
                    self.holding.insert(write_start + i as u16, *value);
                }
                // Function 17h performs writes before reads, including overlap.
                Ok(register_reply(&read_map(
                    &self.holding,
                    read_start,
                    read_count,
                )?))
            }
        }
    }
}

fn preflight<T>(map: &BTreeMap<u16, T>, start: u16, count: u16) -> Result<(), u8> {
    span(start, count, u16::MAX)?;
    if (0..count).all(|i| map.contains_key(&(start + i))) {
        Ok(())
    } else {
        Err(2)
    }
}

fn read_map<T: Copy>(map: &BTreeMap<u16, T>, start: u16, count: u16) -> Result<Vec<T>, u8> {
    preflight(map, start, count)?;
    Ok((0..count).map(|i| map[&(start + i)]).collect())
}

fn register_reply(values: &[u16]) -> Vec<u8> {
    let mut out = vec![(values.len() * 2) as u8];
    for value in values {
        out.extend_from_slice(&value.to_be_bytes());
    }
    out
}

fn ack(start: u16, arg: u16) -> Vec<u8> {
    [start.to_be_bytes(), arg.to_be_bytes()].concat()
}

#[cfg(test)]
#[path = "serial_protocol_tests.rs"]
mod tests;
