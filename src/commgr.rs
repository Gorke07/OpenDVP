//! Delta COMMGR client transport.
//!
//! Source: `DeltaIA.DIAStudio.Communication.Commgr.dll` (Delta editor DIACommission),
//! decompiled with ILSpy: `CommgrBase` connects to `127.0.0.1:8895`; `CommgrRTU` prefixes
//! each Modbus request (station + function + data, no CRC) with a 10-byte header:
//!
//! | bytes | content |
//! |---|---|
//! | 0–1 | function, big-endian: 0x0100 send to device, 0x0107 bypass request |
//! | 2–3 | COMMGR driver ID, big-endian |
//! | 4–7 | remote IPv4 for Ethernet drivers, else zero |
//! | 8–9 | remote port, big-endian (0 when no IP) |
//!
//! Replies start with `01 00` (or `01 07`/`01 08`) followed by station + function + data,
//! or `81 00` + exception. Drivers are listed in
//! `%APPDATA%\Delta Industrial Automation\COMMGR\DriverInfo.dri`.
use crate::plc_download::PduLink;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

pub const PORT: u16 = 8895;
const SEND_TO_DEVICE: u16 = 0x0100;
const MAX_REPLY: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driver {
    pub id: u16,
    pub name: String,
    /// `CommInterface` attribute (Ethernet drivers use 3).
    pub interface: u8,
    pub remote: Vec<(Ipv4Addr, u16, String)>,
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

/// Parse COMMGR's `DriverInfo.dri` (flat XML written by COMMGR itself).
pub fn parse_drivers(xml: &str) -> Vec<Driver> {
    let mut drivers: Vec<Driver> = Vec::new();
    for tag in xml.split('<').skip(1) {
        let tag = tag.split('>').next().unwrap_or_default();
        if tag.starts_with("Driver ") {
            let (Some(id), Some(name)) = (attribute(tag, "DriverID"), attribute(tag, "DriverName"))
            else {
                continue;
            };
            let Ok(id) = id.parse() else { continue };
            drivers.push(Driver {
                id,
                name: name.to_owned(),
                interface: attribute(tag, "CommInterface")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                remote: Vec::new(),
            });
        } else if tag.starts_with("RemoteIP ")
            && let Some(driver) = drivers.last_mut()
            && let (Some(ip), Some(port)) = (
                attribute(tag, "RemoteIPAddr").and_then(|v| v.parse().ok()),
                attribute(tag, "RemotePort").and_then(|v| v.parse().ok()),
            )
        {
            let label = attribute(tag, "RemoteDeviceTypeStr").unwrap_or_default();
            driver.remote.push((ip, port, label.to_owned()));
        }
    }
    drivers
}

pub fn driver_info_path() -> Option<std::path::PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| {
        std::path::Path::new(&appdata)
            .join("Delta Industrial Automation")
            .join("COMMGR")
            .join("DriverInfo.dri")
    })
}

pub fn encode_request(
    driver: u16,
    remote: Option<(Ipv4Addr, u16)>,
    pdu_with_station: &[u8],
) -> Vec<u8> {
    let mut out = SEND_TO_DEVICE.to_be_bytes().to_vec();
    out.extend(driver.to_be_bytes());
    match remote {
        Some((ip, port)) => {
            out.extend(ip.octets());
            out.extend(port.to_be_bytes());
        }
        None => out.extend([0; 6]),
    }
    out.extend(pdu_with_station);
    out
}

/// Returns station + function + data, or the exception code.
pub fn decode_reply(reply: &[u8]) -> Result<Result<Vec<u8>, u8>, String> {
    match reply {
        [0x01, 0x00 | 0x07 | 0x08, body @ ..] if body.len() >= 2 => Ok(Ok(body.to_vec())),
        [0x81, 0x00, body @ ..] if body.len() >= 2 => Ok(Err(body[1])),
        _ => Err("COMMGR yanıt başlığı tanınmadı".into()),
    }
}

pub struct Link {
    stream: TcpStream,
    driver: u16,
    remote: Option<(Ipv4Addr, u16)>,
    station: u8,
    timeout: Duration,
}

impl Link {
    pub fn connect(
        driver: u16,
        remote: Option<(Ipv4Addr, u16)>,
        station: u8,
        timeout: Duration,
    ) -> Result<Self, String> {
        Self::connect_port(PORT, driver, remote, station, timeout)
    }
    pub fn connect_port(
        port: u16,
        driver: u16,
        remote: Option<(Ipv4Addr, u16)>,
        station: u8,
        timeout: Duration,
    ) -> Result<Self, String> {
        let stream =
            TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), timeout)
                .map_err(|e| format!("COMMGR'a bağlanılamadı (127.0.0.1:{port}): {e}"))?;
        stream.set_nodelay(true).map_err(|e| e.to_string())?;
        Ok(Self {
            stream,
            driver,
            remote,
            station,
            timeout,
        })
    }
}

impl PduLink for Link {
    fn transact(&mut self, function: u8, data: &[u8]) -> Result<Result<Vec<u8>, u8>, String> {
        let mut pdu = vec![self.station, function];
        pdu.extend(data);
        self.stream
            .write_all(&encode_request(self.driver, self.remote, &pdu))
            .map_err(|e| format!("COMMGR yazma hatası: {e}"))?;
        // COMMGR answers one request with one TCP message; read until it decodes.
        let deadline = Instant::now() + self.timeout;
        let mut reply = Vec::new();
        let mut buf = [0u8; 512];
        loop {
            let left = deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or("COMMGR yanıtı zaman aşımına uğradı")?;
            self.stream
                .set_read_timeout(Some(left))
                .map_err(|e| e.to_string())?;
            let n = self
                .stream
                .read(&mut buf)
                .map_err(|e| format!("COMMGR okuma hatası: {e}"))?;
            if n == 0 {
                return Err("COMMGR bağlantıyı kapattı".into());
            }
            reply.extend(&buf[..n]);
            if reply.len() > MAX_REPLY {
                return Err("COMMGR yanıtı çok uzun".into());
            }
            if let Ok(decoded) = decode_reply(&reply) {
                return Ok(match decoded {
                    Err(code) => Err(code),
                    Ok(body) if body[0] == self.station && body[1] == function => {
                        Ok(body[2..].to_vec())
                    }
                    Ok(body)
                        if body[0] == self.station
                            && body[1] == function | 0x80
                            && body.len() == 3 =>
                    {
                        Err(body[2])
                    }
                    Ok(_) if reply.len() < 4 => continue,
                    Ok(_) => return Err("COMMGR yanıtında istasyon/işlev eşleşmiyor".into()),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_matches_commgr_rtu() {
        // CommgrRTU.AppendCommgrHeader: function, driver, IP, port, then the RTU body.
        assert_eq!(
            encode_request(
                25033,
                Some((Ipv4Addr::new(192, 168, 1, 5), 502)),
                &[1, 3, 0x10, 0, 0, 1]
            ),
            [
                1, 0, 0x61, 0xC9, 192, 168, 1, 5, 0x01, 0xF6, 1, 3, 0x10, 0, 0, 1
            ]
        );
        assert_eq!(
            encode_request(2080, None, &[1, 1])[..10],
            [1, 0, 0x08, 0x20, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            decode_reply(&[1, 0, 1, 3, 2, 0, 7]),
            Ok(Ok(vec![1, 3, 2, 0, 7]))
        );
        assert_eq!(decode_reply(&[0x81, 0, 1, 2]), Ok(Err(2)));
        assert!(decode_reply(&[2, 0, 1, 3]).is_err());
    }

    #[test]
    fn parses_driver_info() {
        let xml = r#"<?xml version="1.0"?><DriverInfo FileVer="20000" DriverFileID="0" ServerPort="8895">
<Driver DriverID="2080" DriverName="Driver4" CommInterface="2" CommType="0"><ComInfo PortNumber="3"/></Driver>
<Driver DriverID="25033" DriverName="Driver1" CommInterface="3"><NetInfo LocalIPAddr="192.168.1.78">
<RemoteIP RemoteIPAddr="192.168.1.5" RemotePort="502" RemoteDeviceTypeStr="DVP-SE"/></NetInfo></Driver></DriverInfo>"#;
        let drivers = parse_drivers(xml);
        assert_eq!(drivers.len(), 2);
        assert_eq!(
            (
                drivers[0].id,
                drivers[0].name.as_str(),
                drivers[0].interface
            ),
            (2080, "Driver4", 2)
        );
        assert_eq!(
            drivers[1].remote,
            [(Ipv4Addr::new(192, 168, 1, 5), 502, "DVP-SE".into())]
        );
        assert!(parse_drivers(r#"<DriverInfo FileVer="20000"/>"#).is_empty());
    }

    #[test]
    fn link_round_trip_over_loopback() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let peer = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut request = [0u8; 16];
            s.read_exact(&mut request).unwrap();
            assert_eq!(request[..4], [1, 0, 0x08, 0x20]);
            s.write_all(&[1, 0, 1, 3, 2, 0x12, 0x34]).unwrap();
        });
        let mut link = Link::connect_port(port, 2080, None, 1, Duration::from_secs(2)).unwrap();
        assert_eq!(
            link.transact(3, &[0x10, 0, 0, 1]).unwrap(),
            Ok(vec![2, 0x12, 0x34])
        );
        peer.join().unwrap();
    }
}
