//! Read-only clock observations from the already owned, exact-image child.
//! Repeated reads are bounded evidence, never an atomic or lossless trace.
use super::Session;
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct RtcNativeObservation {
    pub owned_pid: u32,
    pub image_sha256: String,
    pub monotonic_begin_us: u64,
    pub monotonic_end_us: u64,
    pub host_epoch_second_begin: u64,
    pub host_epoch_second_end: u64,
    /// Host stamp7, private shadow7, previous sampled second1.
    pub clock_before: [u8; 15],
    pub clock_after: [u8; 15],
    pub public_year_first: [u16; 7],
    pub control_before: [u8; 2],
    pub control_after: [u8; 2],
    pub pc_before: u16,
    pub pc_after: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct RtcDecoderObservation {
    pub owned_pid: u32,
    pub image_sha256: String,
    pub monotonic_begin_us: u64,
    pub monotonic_end_us: u64,
    pub pc_before: u16,
    pub pc_after: u16,
    pub status_before: u8,
    pub status_after: u8,
}
impl RtcNativeObservation {
    pub fn stable_fields(&self) -> bool {
        self.clock_before == self.clock_after
            && self.control_before == self.control_after
            && self.pc_before == self.pc_after
            && self.host_epoch_second_begin == self.host_epoch_second_end
    }
    pub fn shadow(&self) -> [u8; 7] {
        self.clock_after[7..14]
            .try_into()
            .expect("fixed RTC field width")
    }
    pub fn host_stamp(&self) -> [u8; 7] {
        self.clock_after[..7]
            .try_into()
            .expect("fixed RTC field width")
    }
    pub fn controls(&self) -> [bool; 3] {
        [
            self.control_after[0] & 1 != 0,
            self.control_after[0] & 2 != 0,
            self.control_after[1] & 4 != 0,
        ]
    }
    /// Native-only service phase; no local expected result participates.
    pub fn service_phase_settled(&self) -> bool {
        let [four_digit, correction, acknowledged] = self.controls();
        let mut display = self.shadow().map(u16::from);
        if four_digit {
            display[0] += 2000;
        }
        self.stable_fields()
            && self.clock_after[14] == self.clock_after[6]
            && self.public_year_first == display
            && (!correction || (self.shadow()[6] == 0 && acknowledged))
    }
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Addresses {
    clock: usize,
    public: usize,
    controls: [usize; 2],
    pc: usize,
    decoder: usize,
}
#[cfg(any(windows, test))]
fn addresses(digest: &str) -> Result<Addresses, String> {
    match digest {
        super::COMMGR_ES2_SHA256 => Ok(Addresses {
            clock: 0x554834,
            public: 0x54772e,
            controls: [0x54667b, 0x546683],
            pc: 0x553f2e,
            decoder: 0x553fb1,
        }),
        super::COMMGR_SS2_SHA256 => Ok(Addresses {
            clock: 0x5367d8,
            public: 0x52fc02,
            controls: [0x52eb4f, 0x52eb57],
            pc: 0x535ed2,
            decoder: 0x535f55,
        }),
        _ => Err("Inconclusive RTC observation: unqualified executable digest".into()),
    }
}

impl Session {
    /// Paired read-only internal-status diagnostic before another IL operand
    /// can be decoded. This internal bit is not a public PLC error latch.
    #[cfg(windows)]
    pub fn observe_rtc_decoder(
        &mut self,
        origin: Instant,
    ) -> Result<RtcDecoderObservation, String> {
        use super::{owns_listener, random_gate::windows_read};
        use std::{os::windows::io::AsRawHandle, sync::atomic::Ordering};
        let map = addresses(&self.digest)?;
        if self.faulted || self.online_active || self.cancel.load(Ordering::Relaxed) {
            return Err("Inconclusive RTC decoder observation: owned session unavailable".into());
        }
        let begin = u64::try_from(origin.elapsed().as_micros())
            .map_err(|_| "RTC diagnostic duration overflow")?;
        let (handle, pid) = {
            let child = self
                .native
                .as_mut()
                .and_then(|n| n._child.0.as_mut())
                .ok_or("No owned RTC decoder child")?;
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("Owned RTC decoder child exited".into());
            }
            (child.as_raw_handle(), child.id())
        };
        if !owns_listener(pid, self.port)? {
            return Err("RTC decoder listener is not owned by this child".into());
        }
        windows_read::qualify_image_base(handle)?;
        let pc_before = u16::from_le_bytes(windows_read::read::<2>(handle, map.pc)?);
        let status_before = windows_read::read::<1>(handle, map.decoder)?[0];
        let status_after = windows_read::read::<1>(handle, map.decoder)?[0];
        let pc_after = u16::from_le_bytes(windows_read::read::<2>(handle, map.pc)?);
        let child = self
            .native
            .as_mut()
            .and_then(|n| n._child.0.as_mut())
            .ok_or("Owned RTC decoder child disappeared")?;
        if child.id() != pid
            || child.try_wait().map_err(|e| e.to_string())?.is_some()
            || !owns_listener(pid, self.port)?
        {
            return Err(
                "Inconclusive RTC decoder observation: owned child/listener changed".into(),
            );
        }
        Ok(RtcDecoderObservation {
            owned_pid: pid,
            image_sha256: self.digest.clone(),
            monotonic_begin_us: begin,
            monotonic_end_us: u64::try_from(origin.elapsed().as_micros())
                .map_err(|_| "RTC diagnostic duration overflow")?,
            pc_before,
            pc_after,
            status_before,
            status_after,
        })
    }
    #[cfg(not(windows))]
    pub fn observe_rtc_decoder(&mut self, _: Instant) -> Result<RtcDecoderObservation, String> {
        Err("Owned RTC decoder observations require Windows".into())
    }
    /// Does not attach to other processes, write memory, change the system clock,
    /// or initialize local arithmetic. The caller retains every returned capture.
    #[cfg(windows)]
    pub fn observe_rtc_boundary(
        &mut self,
        origin: Instant,
    ) -> Result<RtcNativeObservation, String> {
        use super::{PC, owns_listener, random_gate::windows_read};
        use std::{
            os::windows::io::AsRawHandle,
            sync::atomic::Ordering,
            time::{SystemTime, UNIX_EPOCH},
        };
        let map = addresses(&self.digest)?;
        if self.faulted || self.online_active || self.cancel.load(Ordering::Relaxed) {
            return Err(
                "Inconclusive RTC observation: owned session unavailable for paused observation"
                    .into(),
            );
        }
        let mono = || {
            u64::try_from(origin.elapsed().as_micros())
                .map_err(|_| "RTC observation duration overflow".to_owned())
        };
        let host = || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .map_err(|e| e.to_string())
        };
        let monotonic_begin_us = mono()?;
        let host_epoch_second_begin = host()?;
        let (handle, pid) = {
            let child = self
                .native
                .as_mut()
                .and_then(|n| n._child.0.as_mut())
                .ok_or("No owned RTC child")?;
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("Owned RTC child has exited".into());
            }
            (child.as_raw_handle(), child.id())
        };
        if !owns_listener(pid, self.port)? {
            return Err("RTC listener is not owned by the expected child".into());
        }
        windows_read::qualify_image_base(handle)?;
        let protocol_before = self.word_read(PC)?;
        let pc_before = u16::from_le_bytes(windows_read::read::<2>(handle, map.pc)?);
        if pc_before != protocol_before {
            return Err(
                "Inconclusive RTC observation: protocol/memory PC disagreement before capture"
                    .into(),
            );
        }
        let clock_before = windows_read::read::<15>(handle, map.clock)?;
        let control_before = [
            windows_read::read::<1>(handle, map.controls[0])?[0],
            windows_read::read::<1>(handle, map.controls[1])?[0],
        ];
        let raw_public = windows_read::read::<14>(handle, map.public)?;
        let public_year_first = std::array::from_fn(|i| {
            u16::from_le_bytes([raw_public[(6 - i) * 2], raw_public[(6 - i) * 2 + 1]])
        });
        let control_after = [
            windows_read::read::<1>(handle, map.controls[0])?[0],
            windows_read::read::<1>(handle, map.controls[1])?[0],
        ];
        let clock_after = windows_read::read::<15>(handle, map.clock)?;
        let pc_after = u16::from_le_bytes(windows_read::read::<2>(handle, map.pc)?);
        let protocol_after = self.word_read(PC)?;
        let child = self
            .native
            .as_mut()
            .and_then(|n| n._child.0.as_mut())
            .ok_or("Owned RTC child disappeared")?;
        if child.id() != pid
            || child.try_wait().map_err(|e| e.to_string())?.is_some()
            || !owns_listener(pid, self.port)?
        {
            return Err(
                "Inconclusive RTC observation: owned process/listener changed during capture"
                    .into(),
            );
        }
        if pc_after != protocol_after || pc_before != pc_after {
            return Err("Inconclusive RTC observation: paused PC moved during capture".into());
        }
        Ok(RtcNativeObservation {
            owned_pid: pid,
            image_sha256: self.digest.clone(),
            monotonic_begin_us,
            monotonic_end_us: mono()?,
            host_epoch_second_begin,
            host_epoch_second_end: host()?,
            clock_before,
            clock_after,
            public_year_first,
            control_before,
            control_after,
            pc_before,
            pc_after,
        })
    }
    #[cfg(not(windows))]
    pub fn observe_rtc_boundary(&mut self, _: Instant) -> Result<RtcNativeObservation, String> {
        Err("Owned RTC observations require Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rtc_observation_maps_only_current_exact_images() {
        assert_eq!(
            addresses(super::super::COMMGR_ES2_SHA256).unwrap().clock,
            0x554834
        );
        assert_eq!(
            addresses(super::super::COMMGR_SS2_SHA256).unwrap().public,
            0x52fc02
        );
        for hash in [
            super::super::TARGET_SHA256,
            super::super::SS2_SHA256,
            "unknown",
        ] {
            assert!(addresses(hash).is_err());
        }
    }
    fn observation() -> RtcNativeObservation {
        let clock = [26, 6, 10, 30, 12, 0, 10, 24, 4, 2, 29, 12, 30, 7, 10];
        RtcNativeObservation {
            owned_pid: 1,
            image_sha256: String::new(),
            monotonic_begin_us: 0,
            monotonic_end_us: 10,
            host_epoch_second_begin: 10,
            host_epoch_second_end: 10,
            clock_before: clock,
            clock_after: clock,
            public_year_first: [24, 4, 2, 29, 12, 30, 7],
            control_before: [0, 0],
            control_after: [0, 0],
            pc_before: 0,
            pc_after: 0,
        }
    }
    #[test]
    fn rtc_observation_phase_uses_only_native_publication_and_level_correction() {
        let mut o = observation();
        assert!(o.service_phase_settled());
        o.public_year_first[0] = 2024;
        assert!(!o.service_phase_settled());
        o.control_before[0] = 1;
        o.control_after[0] = 1;
        assert!(o.service_phase_settled());
        o.control_before[0] = 3;
        o.control_after[0] = 3;
        assert!(!o.service_phase_settled());
        o.clock_before[13] = 0;
        o.clock_after[13] = 0;
        o.public_year_first[6] = 0;
        assert!(!o.service_phase_settled());
        o.control_before[1] = 4;
        o.control_after[1] = 4;
        assert!(o.service_phase_settled());
    }
    #[test]
    fn rtc_observation_rejects_torn_fields_sample_change_flags_and_pc() {
        for kind in 0..5 {
            let mut o = observation();
            match kind {
                0 => o.clock_before[14] += 1,
                1 => o.control_before[0] = 1,
                2 => o.pc_after = 3,
                3 => o.host_epoch_second_end += 1,
                _ => o.public_year_first[6] += 1,
            }
            assert!(!o.service_phase_settled());
        }
    }
}
