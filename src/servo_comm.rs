//! ASDRW's documented selector map on an explicitly selected Modbus carrier.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::drive_comm::DrivePortPolicy;
use crate::plc::{Area, Device, Memory, Operand};
use crate::serial_io::{SerialBus, SerialOperand};
use crate::serial_protocol::Frame;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ServoModel {
    #[default]
    Unconfigured,
    LegacyA,
    Ab,
    APlus,
    B,
    A2,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ServoCarrier {
    #[default]
    RequireExplicitSelection,
    StandardModbusCandidate,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mapping23 {
    #[default]
    Unspecified,
    Read,
    Write,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ServoPairOrder {
    #[default]
    Unspecified,
    LowWordFirst,
    HighWordFirst,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BroadcastRead {
    #[default]
    RejectUnspecified,
    TransmitWithoutReplyCandidate,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServoProfile {
    pub model: ServoModel,
    pub carrier: ServoCarrier,
    pub mapping23: Mapping23,
    pub pair_order: ServoPairOrder,
    pub broadcast_read: BroadcastRead,
    pub ports: DrivePortPolicy,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ServoAction {
    pub station: CalendarValue,
    pub selector: CalendarValue,
    pub data: Device,
}
impl ServoAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if name != "ASDRW" || args.len() != 3 {
            return Err("ASDRW üç operand gerektirir; P/D biçimi yok".into());
        }
        let value = |i: usize| -> Result<CalendarValue, String> {
            if !args[i].starts_with(['K', 'H', 'D']) {
                return Err("ASDRW istasyon/seçici yalnız K/H/D olmalı".into());
            }
            let v = CalendarValue::parse(&args[i], false)?;
            if !matches!(
                &v,
                CalendarValue::Additional(AdditionalValue::Word(
                    Operand::Constant(_) | Operand::Register(Device { area: Area::D, .. })
                ))
            ) {
                return Err("ASDRW skaler K/H/D gerektirir".into());
            }
            Ok(v)
        };
        let data: Device = args[2].parse()?;
        if data.area != Area::D {
            return Err("ASDRW veri tabanı çıplak D olmalı".into());
        }
        Ok(Self {
            station: value(0)?,
            selector: value(1)?,
            data,
        })
    }
    pub fn name(&self) -> &'static str {
        "ASDRW"
    }
    pub fn header(&self) -> u16 {
        0xba01
    }
    pub fn words(&self) -> u16 {
        7
    }
    pub fn wide(&self) -> bool {
        false
    }
    pub fn dest(&self) -> Device {
        self.data
    }
    pub fn operands(&self) -> Vec<SerialOperand> {
        vec![
            SerialOperand::Value(self.station.clone()),
            SerialOperand::Value(self.selector.clone()),
            SerialOperand::Data(self.data),
        ]
    }
    pub fn label(&self) -> String {
        format!("ASDRW {} {} {}", self.station, self.selector, self.data)
    }
    pub fn display_value(&self, m: &Memory) -> String {
        format!("{}={}", self.data, m.word(self.data))
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.station.devices(out);
        self.selector.devices(out);
        out.insert(self.data);
        out.extend(
            [
                1120, 1121, 1122, 1124, 1125, 1127, 1129, 1136, 1140, 1141, 1143, 1316, 1317, 1318,
                1319, 1320,
            ]
            .map(Memory::system_device),
        );
        out.extend(
            [1109, 1120, 1121, 1129, 1130, 1252, 1253, 1255].map(|index| Device {
                area: Area::D,
                index,
            }),
        );
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok((_, n, _)) = self.layout(m, m.serial_io.servo_profile)
            && let Ok(span) = self.span(m, n)
        {
            out.extend(span);
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let now = m.timing_io.now_us;
        if now != m.serial_io.now_us {
            SerialBus::advance(m, now);
        }
        let mut s = std::mem::take(&mut m.serial_io);
        s.visit_servo(m, self, power, pc);
        m.serial_io = s;
    }
    fn scalar(m: &Memory, v: &CalendarValue) -> Result<i32, String> {
        let mut ds = BTreeSet::new();
        v.devices(&mut ds);
        if ds
            .iter()
            .any(|d| d.area != Area::D || d.index > m.data_register_limit.unwrap_or(9999))
        {
            return Err("ASDRW kaynak D adresi model sınırını aşıyor".into());
        }
        Ok(v.read(m))
    }
    fn span(&self, m: &Memory, n: u16) -> Result<Vec<Device>, String> {
        if self.data.area != Area::D
            || self
                .data
                .index
                .checked_add(n - 1)
                .is_none_or(|end| end > m.data_register_limit.unwrap_or(9999))
        {
            return Err("ASDRW tam veri bloğu model D sınırını aşıyor".into());
        }
        Ok((0..n)
            .map(|i| Device {
                area: Area::D,
                index: self.data.index + i,
            })
            .collect())
    }
    /// (remote address, local word count, read). Selectors are not function codes.
    fn layout(&self, m: &Memory, p: ServoProfile) -> Result<(u16, u16, bool), String> {
        let selector = Self::scalar(m, &self.selector)?;
        if p.model == ServoModel::Unconfigured {
            return Err("ASDRW açık ASDA model profili gerektirir".into());
        }
        if p.model == ServoModel::A2 {
            match selector {
                16 => Ok((0x12, 10, true)),
                17 => Ok((0x22, 8, false)),
                18 => Ok((0x32, 8, false)),
                19 => Ok((0x40a, 1, false)),
                20 => Ok((0x23c, 1, false)),
                21 => Ok((0x112, 6, false)),
                22 => Ok((0x118, 6, false)),
                23 => match p.mapping23 {
                    Mapping23::Read => Ok((0x46, 8, true)),
                    Mapping23::Write => Ok((0x46, 8, false)),
                    Mapping23::Unspecified => Err(
                        "ASDA-A2 seçici23 read/write yönü PDF'de açıklanmıyor; açık yön seçilmeli"
                            .into(),
                    ),
                },
                _ => Err("ASDA-A2 seçicisi 16..23 olmalı".into()),
            }
        } else {
            match selector {
                0 => Ok((4, 5, true)),
                1 | 2 if p.model == ServoModel::B => Err("ASDA-B seçici1/2 desteklemiyor".into()),
                1 => Ok((9, 8, true)),
                2 => Ok((9, 8, false)),
                3 => Ok((0x405, 1, false)),
                4 => Ok((0x21e, 1, false)),
                5 => Ok((0x109, 3, false)),
                6 => Ok((0x10c, 3, false)),
                _ => Err("Legacy ASDA seçicisi 0..6 olmalı".into()),
            }
        }
    }
    pub(crate) fn prepare(
        &self,
        m: &Memory,
        p: ServoProfile,
    ) -> Result<(Frame, Option<Device>), String> {
        let station = Self::scalar(m, &self.station)?;
        if !(0..=254).contains(&station) {
            return Err("ASDRW istasyon0..254 olmalı".into());
        }
        let (address, count, read) = self.layout(m, p)?;
        let devices = self.span(m, count)?;
        if p.carrier != ServoCarrier::StandardModbusCandidate {
            return Err(
                "ASDRW tam factory wire örneği yok; açık StandardModbusCandidate seçilmeli".into(),
            );
        }
        if read && station == 0 && p.broadcast_read != BroadcastRead::TransmitWithoutReplyCandidate
        {
            return Err("ASDRW broadcast read anlamı belirsiz; varsayılan profil reddeder".into());
        }
        let selector = Self::scalar(m, &self.selector)?;
        let mut words = if read {
            Vec::new()
        } else {
            devices
                .iter()
                .map(|d| m.word(*d) as u16)
                .collect::<Vec<_>>()
        };
        if !read {
            let limit = |ok: bool| {
                if ok {
                    Ok(())
                } else {
                    Err("ASDRW değer servo seçici aralığının dışında".to_string())
                }
            };
            if p.model == ServoModel::A2 {
                match selector {
                    19 => limit((1..=5000).contains(&words[0]) || words[0] == 0)?,
                    20 => words[0] = u16::from(words[0] == 1),
                    21 | 22 => {
                        if p.pair_order == ServoPairOrder::Unspecified {
                            return Err("ASDA-A2 signed32 çift WORD sırası açık seçilmeli".into());
                        }
                        let max = if selector == 21 { 60000 } else { 300 };
                        for pair in words.as_chunks_mut::<2>().0 {
                            let value = (u32::from(pair[0]) | (u32::from(pair[1]) << 16)) as i32;
                            limit((-max..=max).contains(&value))?;
                            if p.pair_order == ServoPairOrder::HighWordFirst {
                                pair.swap(0, 1);
                            }
                        }
                    }
                    _ => {}
                }
            } else {
                match selector {
                    3 => {
                        limit((1..=3000).contains(&words[0]) || (4998..=5000).contains(&words[0]))?
                    }
                    4 => words[0] = u16::from(words[0] == 1),
                    5 | 6 => {
                        let max = if selector == 5 { 5000 } else { 300 };
                        for v in &words {
                            limit((-max..=max).contains(&i32::from(*v as i16)))?;
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut data = address.to_be_bytes().to_vec();
        let function = if read {
            data.extend(count.to_be_bytes());
            3
        } else if count == 1 {
            data.extend(words[0].to_be_bytes());
            6
        } else {
            data.extend(count.to_be_bytes());
            data.push((count * 2) as u8);
            for word in words {
                data.extend(word.to_be_bytes());
            }
            16
        };
        Ok((
            Frame {
                station: station as u8,
                function,
                data,
            },
            read.then_some(self.data),
        ))
    }
}
#[cfg(test)]
#[path = "servo_comm_tests.rs"]
mod tests;
