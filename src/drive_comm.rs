//! Drive commands on the shared explicit serial bus; VFD-A codec remains open.
use crate::calendar_operand::CalendarValue;
use crate::plc::{Area, Device, Memory};
use crate::serial_io::SerialBus;
use crate::serial_protocol::Frame;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DriveFamilyPolarity {
    #[default]
    DetailPages,
    SystemTablePolarity,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DriveCarrier {
    #[default]
    StrictDocumented,
    FactoryRegisterCandidate,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DrivePortPolicy {
    #[default]
    Com2Only,
    Com3ThenCom2Candidate,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DriveProfile {
    pub polarity: DriveFamilyPolarity,
    pub carrier: DriveCarrier,
    pub ports: DrivePortPolicy,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveCommand {
    Fwd,
    Rev,
    Stop,
    Rdst,
    Rstef,
}
#[derive(Debug, Clone, PartialEq)]
pub struct DriveAction {
    pub command: DriveCommand,
    pub station: CalendarValue,
    pub frequency: Option<CalendarValue>,
    pub mode: CalendarValue,
}
impl DriveAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let command = match name {
            "FWD" => DriveCommand::Fwd,
            "REV" => DriveCommand::Rev,
            "STOP" => DriveCommand::Stop,
            "RDST" => DriveCommand::Rdst,
            "RSTEF" => DriveCommand::Rstef,
            _ => return Err("FWD/REV/STOP/RDST/RSTEF bekleniyor; P/D biçimi yok".into()),
        };
        let three = matches!(
            command,
            DriveCommand::Fwd | DriveCommand::Rev | DriveCommand::Stop
        );
        if args.len() != if three { 3 } else { 2 } {
            return Err(format!(
                "{name} {} operand gerektirir",
                if three { 3 } else { 2 }
            ));
        }
        let value = |i: usize| -> Result<CalendarValue, String> {
            if !args[i].starts_with(['K', 'H', 'D']) {
                return Err("Sürücü operandı yalnız K/H/D olmalı".into());
            }
            let value = CalendarValue::parse(&args[i], false)?;
            if !matches!(
                &value,
                CalendarValue::Additional(crate::additional_processing::AdditionalValue::Word(
                    crate::plc::Operand::Constant(_)
                        | crate::plc::Operand::Register(Device { area: Area::D, .. })
                ))
            ) {
                return Err("Sürücü operandı skaler K/H/D olmalı; Kn grubu kabul edilmez".into());
            }
            Ok(value)
        };
        Ok(Self {
            command,
            station: value(0)?,
            frequency: if three { Some(value(1)?) } else { None },
            mode: value(if three { 2 } else { 1 })?,
        })
    }
    pub fn name(&self) -> &'static str {
        match self.command {
            DriveCommand::Fwd => "FWD",
            DriveCommand::Rev => "REV",
            DriveCommand::Stop => "STOP",
            DriveCommand::Rdst => "RDST",
            DriveCommand::Rstef => "RSTEF",
        }
    }
    pub fn words(&self) -> u16 {
        if self.frequency.is_some() { 7 } else { 5 }
    }
    pub fn wide(&self) -> bool {
        false
    }
    pub fn header(&self) -> u16 {
        match self.command {
            DriveCommand::Fwd => 0xc600,
            DriveCommand::Rev => 0xc800,
            DriveCommand::Stop => 0xca00,
            DriveCommand::Rdst => 0xcc00,
            DriveCommand::Rstef => 0xce00,
        }
    }
    pub fn operands(&self) -> Vec<CalendarValue> {
        let mut v = vec![self.station.clone()];
        if let Some(f) = &self.frequency {
            v.push(f.clone());
        }
        v.push(self.mode.clone());
        v
    }
    pub fn dest(&self) -> Device {
        Device {
            area: Area::D,
            index: 1070,
        }
    }
    pub fn label(&self) -> String {
        format!(
            "{} {}",
            self.name(),
            self.operands()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
    pub fn display_value(&self, m: &Memory) -> String {
        format!("D1070={}", m.word(self.dest()))
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for v in self.operands() {
            v.devices(out);
        }
        out.extend(
            [
                1120, 1121, 1122, 1124, 1125, 1127, 1129, 1131, 1136, 1142, 1143, 1177, 1316, 1317,
                1318, 1319, 1320,
            ]
            .map(Memory::system_device),
        );
        out.extend([1109, 1120, 1129, 1130, 1252, 1253].map(|index| Device {
            area: Area::D,
            index,
        }));
        out.extend((1070..=1074).map(|index| Device {
            area: Area::D,
            index,
        }));
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let now = m.timing_io.now_us;
        if now != m.serial_io.now_us {
            SerialBus::advance(m, now);
        }
        let mut bus = std::mem::take(&mut m.serial_io);
        bus.visit_drive(m, self, power, pc);
        m.serial_io = bus;
    }
    pub(crate) fn prepare(&self, m: &Memory, profile: DriveProfile) -> Result<Frame, String> {
        let scalar = |v: &CalendarValue| -> Result<i32, String> {
            let mut ds = BTreeSet::new();
            v.devices(&mut ds);
            for d in ds {
                if d.area != Area::D || d.index > m.data_register_limit.unwrap_or(9999) {
                    return Err("Sürücü kaynak D adresi model sınırını aşıyor".into());
                }
            }
            Ok(v.read(m))
        };
        let other = match profile.polarity {
            DriveFamilyPolarity::DetailPages => m.system_bit(1177),
            DriveFamilyPolarity::SystemTablePolarity => !m.system_bit(1177),
        };
        if !other {
            return Err("VFD-A genel C/Q istek/checksum/broadcast codec'i doğrulanmadı; Modbus yerine geçirilemez".into());
        }
        let station = scalar(&self.station)?;
        let n = if matches!(self.command, DriveCommand::Fwd | DriveCommand::Rev) {
            scalar(&self.mode)?
        } else {
            // Other-VFD STOP/RDST/RSTEF reserve n: do not dereference it.
            0
        };
        if !(if self.command == DriveCommand::Rdst {
            1
        } else {
            0
        }..=255)
            .contains(&station)
        {
            return Err("Sürücü adresi 0..255, RDST için 1..255 olmalı".into());
        }
        let mut data = Vec::new();
        let mut word = |v: u16| data.extend(v.to_be_bytes());
        let function = match self.command {
            DriveCommand::Rdst => {
                word(0x2100);
                word(5);
                3
            }
            DriveCommand::Rstef => {
                word(0x2002);
                word(2);
                6
            }
            DriveCommand::Stop => {
                if profile.carrier != DriveCarrier::FactoryRegisterCandidate {
                    return Err(
                        "STOP FC06 taşıyıcısı açık FactoryRegisterCandidate profili gerektirir"
                            .into(),
                    );
                }
                word(0x2000);
                word(1);
                6
            }
            DriveCommand::Fwd | DriveCommand::Rev => {
                let candidate = self.command == DriveCommand::Rev || n == 1;
                if candidate && profile.carrier != DriveCarrier::FactoryRegisterCandidate {
                    return Err(
                        "REV/JOG taşıyıcısı açık FactoryRegisterCandidate profili gerektirir"
                            .into(),
                    );
                }
                let direction = if self.command == DriveCommand::Rev {
                    0x20
                } else {
                    0x10
                };
                if n == 1 {
                    word(0x2000);
                    word(direction | 3);
                    6
                } else {
                    let frequency =
                        scalar(self.frequency.as_ref().ok_or("Frekans operandı eksik")?)? as u16;
                    word(0x2000);
                    word(2);
                    data.push(4);
                    data.extend((direction | 2u16).to_be_bytes());
                    data.extend(frequency.to_be_bytes());
                    16
                }
            }
        };
        Ok(Frame {
            station: station as u8,
            function,
            data,
        })
    }
}

#[cfg(test)]
#[path = "drive_comm_tests.rs"]
mod tests;
