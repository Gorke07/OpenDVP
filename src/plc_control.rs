//! Stand-alone PLC control and status over the same Modbus link as the download path:
//! RUN/STOP through M1072 (what DVPtool writes around a download) and a read-only status
//! snapshot of the special relays/registers Delta editor shows (error flag/code, scan times, RTC).
//! Device map: M0 = 0x0800, D0 = 0x1000. Not yet verified on a physical PLC.
use crate::plc_download::{PduLink, read_coil, read_words, write_coil};

const M_BASE: u16 = 0x0800;
const D_BASE: u16 = 0x1000;
const RUN_COIL: u16 = M_BASE + 1072;
const ERROR_COIL: u16 = M_BASE + 1004;
const YEAR4_COIL: u16 = M_BASE + 1016;

pub fn set_run(link: &mut dyn PduLink, run: bool) -> Result<(), String> {
    write_coil(
        link,
        RUN_COIL,
        run,
        if run { "RUN (M1072)" } else { "STOP (M1072)" },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rtc {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    /// 0 = Sunday … 6 = Saturday.
    pub weekday: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub running: bool,
    /// M1004: illegal instruction / program error flag.
    pub error: bool,
    /// D1004 error code and D1008 step address where it occurred.
    pub error_code: u16,
    pub error_step: u16,
    /// D1010..D1012 in 0.1 ms: current, minimum, maximum scan time.
    pub scan_current: u16,
    pub scan_min: u16,
    pub scan_max: u16,
    /// D1313..D1319; `None` for models without an RTC or when the read was refused.
    pub rtc: Option<Rtc>,
}

pub fn status(link: &mut dyn PduLink) -> Result<Status, String> {
    let running = read_coil(link, RUN_COIL, "RUN durumu (M1072)")?;
    let error = read_coil(link, ERROR_COIL, "Hata bayrağı (M1004)")?;
    let d = |link: &mut dyn PduLink, index: u16, count: usize, what: &str| {
        read_words(link, 0x03, D_BASE + index, count, what)
    };
    let code = d(link, 1004, 1, "Hata kodu (D1004)")?[0];
    let step = d(link, 1008, 1, "Hata adımı (D1008)")?[0];
    let scan = d(link, 1010, 3, "Tarama süresi (D1010..D1012)")?;
    let year4 = read_coil(link, YEAR4_COIL, "Yıl biçimi (M1016)").unwrap_or(false);
    let rtc = d(link, 1313, 7, "Saat (D1313..D1319)").ok().map(|w| Rtc {
        second: w[0],
        minute: w[1],
        hour: w[2],
        day: w[3],
        month: w[4],
        weekday: w[5],
        year: if year4 { w[6] } else { 2000 + w[6] },
    });
    Ok(Status {
        running,
        error,
        error_code: code,
        error_step: step,
        scan_current: scan[0],
        scan_min: scan[1],
        scan_max: scan[2],
        rtc,
    })
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

    #[test]
    fn run_and_stop_write_m1072() {
        let mut link = Script(vec![
            (vec![5, 0x0C, 0x30, 0xFF, 0], Ok(vec![0x0C, 0x30, 0xFF, 0])),
            (vec![5, 0x0C, 0x30, 0, 0], Ok(vec![0x0C, 0x30, 0, 0])),
        ]);
        set_run(&mut link, true).unwrap();
        set_run(&mut link, false).unwrap();
    }

    #[test]
    fn status_reads_flags_registers_and_clock() {
        let words = |v: &[u16]| {
            let mut r = vec![(v.len() * 2) as u8];
            r.extend(v.iter().flat_map(|w| w.to_be_bytes()));
            Ok(r)
        };
        let mut link = Script(vec![
            (vec![1, 0x0C, 0x30, 0, 1], Ok(vec![1, 1])),
            (vec![1, 0x0B, 0xEC, 0, 1], Ok(vec![1, 0])),
            (vec![3, 0x13, 0xEC, 0, 1], words(&[0])),
            (vec![3, 0x13, 0xF0, 0, 1], words(&[0])),
            (vec![3, 0x13, 0xF2, 0, 3], words(&[25, 10, 40])),
            (vec![1, 0x0B, 0xF8, 0, 1], Ok(vec![1, 0])),
            (vec![3, 0x15, 0x21, 0, 7], words(&[5, 4, 3, 2, 1, 6, 26])),
        ]);
        let s = status(&mut link).unwrap();
        assert!(s.running && !s.error);
        assert_eq!((s.scan_current, s.scan_min, s.scan_max), (25, 10, 40));
        let rtc = s.rtc.unwrap();
        assert_eq!((rtc.year, rtc.month, rtc.day), (2026, 1, 2));
        assert_eq!((rtc.hour, rtc.minute, rtc.second, rtc.weekday), (3, 4, 5, 6));
    }
}
