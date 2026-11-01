//! Right-side extension modules of a DVP PLC: discovery and CR access.
//!
//! Source: Delta's NTC Wizard (Delta editor), decompiled with ILSpy —
//! `DeltaIA.DIAStudio.Communication.CommProcessDVP.dll` (`DVPCommProcessMgr`),
//! `...SecondProtocol.dll` (`SecondProtocolLib.SecondTransfer`) and
//! `...CommProcessMgr.dll` (`DVPModuleName`).
//!
//! Discovery uses plain FC03 reads of D1140 (module count), D1142/D1143 (module X/Y
//! totals) and D1320+i (module type). CR access is Delta's FC64 "second protocol":
//! `[01][command][length][module][CR][data…]` with command 0x16/0x17 (one CR read/write)
//! and 0x20/0x21 (two CRs). The reply's last bytes carry the value; 16-bit values are
//! big-endian and 32-bit values are low word first, each word big-endian.
//!
//! The installed simulators have no modules and reject these FC64 commands (exception 03),
//! so CR access is verified only against the decompiled source, not live.
use crate::plc_download::PduLink;

const MODULE_COUNT: u16 = 0x1000 + 1140;
const MODULE_X_TOTAL: u16 = 0x1000 + 1142;
const MODULE_TYPES: u16 = 0x1000 + 1320;
const MAX_MODULES: usize = 8;

const READ_ONE: u8 = 0x16;
const WRITE_ONE: u8 = 0x17;
const READ_TWO: u8 = 0x20;
const WRITE_TWO: u8 = 0x21;

/// `DVPModuleName` values (module type word in D1320+i, also each module's CR0).
pub const MODULE_TYPES_TABLE: &[(u16, &str)] = &[
    (38, "DVP-RC10-E2"),
    (65, "DVP02DA-E2"),
    (69, "DVP02PU-E2"),
    (73, "DVP02DA-S"),
    (128, "DVP04AD-E2"),
    (129, "DVP04DA-E2"),
    (130, "DVP04PT-E2"),
    (131, "DVP04TC-E2"),
    (136, "DVP04AD-S"),
    (137, "DVP04DA-S"),
    (138, "DVP04PT-S"),
    (139, "DVP04TC-S"),
    (144, "DVP04AD-S2"),
    (145, "DVP04DA-S2"),
    (154, "DVP04PT-SH"),
    (194, "DVP06PT-E2"),
    (196, "DVP06XA-E2"),
    (200, "DVP06AD-S"),
    (204, "DVP06XA-S"),
    (212, "DVP06XA-S2"),
    (216, "DVP04AD-SH"),
    (220, "DVP06XA-SH"),
    (269, "DVP08NTC-S"),
    (272, "DVP01PU-S"),
    (335, "DVP02TUL-S"),
    (560, "DVP02DT-H2"),
    (576, "DVP02CP-H2"),
    (591, "DVP02TUN-S"),
    (592, "DVP02PF-H2"),
    (847, "DVP02TUR-S"),
    (1103, "DVP02TKL-S"),
    (1359, "DVP02TKN-S"),
    (1615, "DVP02TKR-S"),
    (1871, "DVP02UHL-S"),
    (16449, "DVPSCM12-SL"),
    (16450, "DVPSCM52-SL"),
    (16464, "DVPEN01-SL"),
    (16646, "DVP01LC-SL"),
    (16688, "DVPCOPM-SL"),
    (16689, "DVPDNET-SL"),
    (16902, "DVP02LC-SL"),
    (17408, "DVP04AD-SL"),
    (17409, "DVP04DA-SL"),
    (20742, "DVP201LC-SL"),
    (20998, "DVP202LC-SL"),
    (22790, "DVP211LC-SL"),
    (24848, "DVP01PU-H2"),
    (24864, "DVP01HC-H2"),
    (25600, "DVP04AD-H2"),
    (25601, "DVP04DA-H2"),
    (25602, "DVP04PT-H2"),
    (25603, "DVP04TC-H2"),
    (25607, "DVP04AD-H3"),
    (25609, "DVP06XA-H3"),
    (26116, "DVP06XA-H2"),
    (26628, "DVP08TC-H2"),
];

pub fn module_name(code: u16) -> Option<&'static str> {
    MODULE_TYPES_TABLE
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, name)| *name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleScan {
    /// Module type codes in slot order (slot 0 is next to the CPU).
    pub types: Vec<u16>,
    pub x_points: u16,
    pub y_points: u16,
}

fn read_registers(link: &mut dyn PduLink, address: u16, count: u16) -> Result<Vec<u16>, String> {
    let mut request = address.to_be_bytes().to_vec();
    request.extend(count.to_be_bytes());
    let reply = link
        .transact(0x03, &request)?
        .map_err(|code| format!("D register okuma: Modbus istisnası {code:#04x}"))?;
    if reply.len() != 1 + 2 * usize::from(count) || usize::from(reply[0]) != 2 * usize::from(count)
    {
        return Err("D register yanıt uzunluğu geçersiz".into());
    }
    Ok(reply[1..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|w| u16::from_be_bytes(*w))
        .collect())
}

pub fn scan(link: &mut dyn PduLink) -> Result<ModuleScan, String> {
    let count = usize::from(read_registers(link, MODULE_COUNT, 1)?[0]);
    if count > MAX_MODULES {
        return Err(format!(
            "PLC {count} sağ modül bildirdi; en fazla {MAX_MODULES} olabilir"
        ));
    }
    let totals = read_registers(link, MODULE_X_TOTAL, 2)?;
    let types = if count == 0 {
        Vec::new()
    } else {
        read_registers(link, MODULE_TYPES, count as u16)?
    };
    Ok(ModuleScan {
        types,
        x_points: totals[0],
        y_points: totals[1],
    })
}

fn second_protocol(
    link: &mut dyn PduLink,
    command: u8,
    module: u8,
    cr: u8,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    if usize::from(module) >= MAX_MODULES {
        return Err(format!("Modül numarası 0–{} olmalı", MAX_MODULES - 1));
    }
    let mut request = vec![0x01, command, (2 + data.len()) as u8, module, cr];
    request.extend(data);
    let reply = link
        .transact(0x64, &request)?
        .map_err(|code| format!("Modül {module} CR{cr}: Modbus istisnası {code:#04x}"))?;
    // SecondProtocolLib accepts any reply whose function is 0x64; the value is at the end.
    if reply.len() < 2 || reply[..2] != [0x01, command] {
        return Err(format!("Modül {module} CR{cr}: FC64 yanıt biçimi geçersiz"));
    }
    Ok(reply)
}

fn tail<const N: usize>(reply: &[u8]) -> Result<[u8; N], String> {
    reply
        .len()
        .checked_sub(N)
        .and_then(|start| reply[start..].try_into().ok())
        .ok_or_else(|| "CR yanıtı kısa".to_owned())
}

pub fn read_cr(link: &mut dyn PduLink, module: u8, cr: u8) -> Result<u16, String> {
    let reply = second_protocol(link, READ_ONE, module, cr, &[])?;
    Ok(u16::from_be_bytes(tail::<2>(&reply)?))
}

pub fn write_cr(link: &mut dyn PduLink, module: u8, cr: u8, value: u16) -> Result<(), String> {
    second_protocol(link, WRITE_ONE, module, cr, &value.to_be_bytes()).map(drop)
}

/// CRn (low word) and CRn+1 (high word).
pub fn read_cr32(link: &mut dyn PduLink, module: u8, cr: u8) -> Result<u32, String> {
    let reply = second_protocol(link, READ_TWO, module, cr, &[])?;
    let [l0, l1, h0, h1] = tail::<4>(&reply)?;
    Ok(u32::from_be_bytes([h0, h1, l0, l1]))
}

pub fn write_cr32(link: &mut dyn PduLink, module: u8, cr: u8, value: u32) -> Result<(), String> {
    let [h0, h1, l0, l1] = value.to_be_bytes();
    second_protocol(link, WRITE_TWO, module, cr, &[l0, l1, h0, h1]).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Script(Vec<(Vec<u8>, Result<Vec<u8>, u8>)>);
    impl PduLink for Script {
        fn transact(&mut self, function: u8, data: &[u8]) -> Result<Result<Vec<u8>, u8>, String> {
            let mut pdu = vec![function];
            pdu.extend(data);
            let (expected, reply) = self.0.remove(0);
            assert_eq!(pdu, expected);
            Ok(reply)
        }
    }

    #[test]
    fn scan_reads_count_totals_and_types() {
        let mut link = Script(vec![
            (vec![0x03, 0x14, 0x74, 0, 1], Ok(vec![2, 0, 2])),
            (vec![0x03, 0x14, 0x76, 0, 2], Ok(vec![4, 0, 0, 0, 0])),
            (
                vec![0x03, 0x15, 0x28, 0, 2],
                Ok(vec![4, 0, 0x80, 0x01, 0x0D]),
            ),
        ]);
        let scan = scan(&mut link).unwrap();
        assert_eq!(scan.types, [0x80, 269]);
        assert_eq!(module_name(scan.types[0]), Some("DVP04AD-E2"));
        assert_eq!(module_name(scan.types[1]), Some("DVP08NTC-S"));
    }

    #[test]
    fn cr_frames_match_second_protocol() {
        // ReadSingleCR(module 1, CR 6): [01][16][02][01][06]; value = last two bytes, BE.
        let mut link = Script(vec![(
            vec![0x64, 0x01, 0x16, 0x02, 0x01, 0x06],
            Ok(vec![0x01, 0x16, 0x04, 0x01, 0x06, 0x12, 0x34]),
        )]);
        assert_eq!(read_cr(&mut link, 1, 6).unwrap(), 0x1234);
        // WriteSingleCR(0, 35, 0x5555) as in dispatch_TurnOnDownloadResistanceTableFlag_Second.
        let mut link = Script(vec![(
            vec![0x64, 0x01, 0x17, 0x04, 0x00, 35, 0x55, 0x55],
            Ok(vec![0x01, 0x17, 0x04, 0x00, 35, 0x55, 0x55]),
        )]);
        write_cr(&mut link, 0, 35, 0x5555).unwrap();
        // 32-bit: low word first.
        let mut link = Script(vec![(
            vec![0x64, 0x01, 0x21, 0x06, 0x00, 37, 0x56, 0x78, 0x12, 0x34],
            Ok(vec![0x01, 0x21, 0x02, 0x00, 37]),
        )]);
        write_cr32(&mut link, 0, 37, 0x1234_5678).unwrap();
        let mut link = Script(vec![(
            vec![0x64, 0x01, 0x20, 0x02, 0x00, 37],
            Ok(vec![0x01, 0x20, 0x06, 0x00, 37, 0x56, 0x78, 0x12, 0x34]),
        )]);
        assert_eq!(read_cr32(&mut link, 0, 37).unwrap(), 0x1234_5678);
    }

    #[test]
    fn rejects_bad_modules_and_exceptions() {
        let mut link = Script(vec![]);
        assert!(read_cr(&mut link, 8, 0).is_err());
        let mut link = Script(vec![(vec![0x64, 0x01, 0x16, 0x02, 0x00, 0x00], Err(3))]);
        assert!(read_cr(&mut link, 0, 0).unwrap_err().contains("istisnası"));
        let mut link = Script(vec![(vec![0x03, 0x14, 0x74, 0, 1], Ok(vec![2, 0, 9]))]);
        assert!(scan(&mut link).is_err());
    }
}
