//! Force X/Y devices on a real PLC, the way Delta editor does (reverse engineered from
//! `PopForceOnClick` / `PopForceOffClick` / `PopForceReleaseClick` / `PopForceReleaseAllClick`
//! and the transmit-queue consumer `routine`).
//!
//! Wire format is the same FC64 envelope as the download path (`64 01 <cmd> <len> <data>`):
//! command `0x30`, data = device address (big endian; X = 0x0400+n, Y = 0x0500+n, the same map
//! as the monitor reads) + value byte `FF` force ON, `00` force OFF, `5A` release that device.
//! Release-all is the same command with no data. The PLC holds the forced state itself, so
//! no PC-side keep-alive is needed; a forced device survives until released.
//! Observed from the binary only; not yet verified against a physical PLC.
use crate::plc::{Area, Device};
use crate::plc_download::{PduLink, fc64};

const FORCE_COMMAND: u8 = 0x30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Force {
    On,
    Off,
    Release,
}

impl Force {
    fn byte(self) -> u8 {
        match self {
            Force::On => 0xFF,
            Force::Off => 0x00,
            Force::Release => 0x5A,
        }
    }
}

/// Wire address of a forceable device (Delta editor only offers X and Y).
pub fn address(device: Device) -> Result<u16, String> {
    let base: u16 = match device.area {
        Area::X => 0x0400,
        Area::Y => 0x0500,
        _ => return Err(format!("{device}: yalnız X ve Y aygıtları zorlanabilir.")),
    };
    base.checked_add(device.index)
        .filter(|a| *a < base + 0x100)
        .ok_or_else(|| format!("{device}: adres aralık dışında."))
}

fn expect_ok(
    reply: Result<Result<Vec<u8>, u8>, String>,
    what: &str,
) -> Result<(), String> {
    match reply? {
        Ok(_) => Ok(()),
        Err(code) => Err(format!("{what}: PLC Modbus istisnası {code:#04x}")),
    }
}

pub fn force(link: &mut dyn PduLink, device: Device, mode: Force) -> Result<(), String> {
    let addr = address(device)?.to_be_bytes();
    let what = format!("Zorlama {device}");
    expect_ok(
        fc64(link, FORCE_COMMAND, &[addr[0], addr[1], mode.byte()], &what),
        &what,
    )
}

pub fn release_all(link: &mut dyn PduLink) -> Result<(), String> {
    expect_ok(
        fc64(link, FORCE_COMMAND, &[], "Tüm zorlamaları kaldır"),
        "Tüm zorlamaları kaldır",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Script(Vec<(Vec<u8>, Result<Vec<u8>, u8>)>);
    impl PduLink for Script {
        fn transact(&mut self, f: u8, d: &[u8]) -> Result<Result<Vec<u8>, u8>, String> {
            let mut pdu = vec![f];
            pdu.extend(d);
            let (want, reply) = self.0.remove(0);
            assert_eq!(pdu, want);
            Ok(reply)
        }
    }
    fn dev(s: &str) -> Device {
        s.parse().unwrap()
    }

    #[test]
    fn frames_match_editor() {
        let mut link = Script(vec![
            (vec![0x64, 1, 0x30, 3, 0x05, 0x00, 0xFF], Ok(vec![1, 0x30, 0])),
            (vec![0x64, 1, 0x30, 3, 0x05, 0x03, 0x00], Ok(vec![1, 0x30, 0])),
            (vec![0x64, 1, 0x30, 3, 0x04, 0x01, 0x5A], Ok(vec![1, 0x30, 0])),
            (vec![0x64, 1, 0x30, 0], Ok(vec![1, 0x30, 0])),
        ]);
        force(&mut link, dev("Y0"), Force::On).unwrap();
        force(&mut link, dev("Y3"), Force::Off).unwrap();
        force(&mut link, dev("X1"), Force::Release).unwrap();
        release_all(&mut link).unwrap();
    }

    #[test]
    fn rejects_other_areas_and_reports_exceptions() {
        assert!(address(dev("M0")).is_err());
        let mut link = Script(vec![(vec![0x64, 1, 0x30, 3, 0x05, 0x00, 0xFF], Err(1))]);
        assert!(force(&mut link, dev("Y0"), Force::On).unwrap_err().contains("0x01"));
    }
}
