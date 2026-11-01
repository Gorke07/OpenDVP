//! Documented VRRD/VRSC reads from explicitly supplied virtual knob samples.
//! No ADC, raw-to-scale calibration curve, or installed-simulator parity is implied.
use crate::additional_processing::AdditionalValue;
use crate::plc::{Area, Device, Memory, Operand};
use std::collections::BTreeSet;

fn at(area: Area, index: u16) -> Device {
    Device { area, index }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobRead {
    Raw,
    Scaled,
}
#[derive(Debug, Clone, PartialEq)]
pub struct KnobAction {
    pub kind: KnobRead,
    pub channel: u8,
    pub destination: AdditionalValue,
}
impl KnobAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let kind = match name {
            "VRRD" => KnobRead::Raw,
            "VRSC" => KnobRead::Scaled,
            _ => {
                return Err(
                    "VRRD/VRSC gerekli; P dış kenar sarmalayıcısıdır, 32 bit biçim yoktur".into(),
                );
            }
        };
        if args.len() != 2 {
            return Err(format!("{name} K/H0..1 ve hedef gerektirir"));
        }
        let channel = if let Some(n) = args[0].strip_prefix('K') {
            n.parse::<u8>().map_err(|_| "VR numarası K/H0..1 olmalı")?
        } else if let Some(n) = args[0].strip_prefix('H') {
            u8::from_str_radix(n, 16).map_err(|_| "VR numarası K/H0..1 olmalı")?
        } else {
            return Err("VR numarası yalnız K/H sabiti olmalı".into());
        };
        if channel > 1 {
            return Err("Yalnız VR0 ve VR1 belgelenmiştir".into());
        }
        let destination = AdditionalValue::parse(&args[1], false, true)?;
        let result = Self {
            kind,
            channel,
            destination,
        };
        result.destination_shape()?;
        Ok(result)
    }
    pub fn name(&self) -> &'static str {
        match self.kind {
            KnobRead::Raw => "VRRD",
            KnobRead::Scaled => "VRSC",
        }
    }
    pub fn header(&self) -> u16 {
        match self.kind {
            KnobRead::Raw => 0xba00,
            KnobRead::Scaled => 0xbc00,
        }
    }
    pub fn words(&self) -> u16 {
        5
    }
    pub fn wide(&self) -> bool {
        false
    }
    pub fn dest(&self) -> Device {
        self.destination.base().expect("validated knob destination")
    }
    pub fn label(&self) -> String {
        format!("{} K{} {}", self.name(), self.channel, self.destination)
    }
    pub fn operands(&self) -> Vec<AdditionalValue> {
        vec![
            AdditionalValue::Word(Operand::Constant(i16::from(self.channel))),
            self.destination.clone(),
        ]
    }
    pub fn display_value(&self, m: &Memory) -> String {
        format!("{} = {}", self.destination, self.destination.read(m))
    }
    fn destination_shape(&self) -> Result<(), String> {
        match &self.destination {
            AdditionalValue::Word(Operand::Register(v))
                if matches!(v.area, Area::T | Area::C | Area::D) =>
            {
                Ok(())
            }
            AdditionalValue::Group {
                base,
                digits,
                wide: false,
            } if matches!(base.area, Area::Y | Area::M | Area::S) && (1..=4).contains(digits) => {
                Ok(())
            }
            _ => Err("VR hedefi T/C/D veya K1..K4 Y/M/S bit grubu olmalı".into()),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.destination.devices(out);
        for channel in 0..2 {
            out.insert(at(Area::M, 1178 + channel));
            out.insert(at(Area::D, 1178 + channel));
        }
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    pub fn validate_destination(&self, m: &Memory) -> Result<(), String> {
        self.destination_shape()?;
        let mut out = BTreeSet::new();
        self.destination.devices(&mut out);
        for v in out {
            if m.analog_knob.profile != KnobProfile::Unconfigured
                && v.area == Area::D
                && matches!(v.index, 1178 | 1179)
            {
                return Err(
                    "D1178/D1179 yapılandırılmış VR profilinde salt okunur izleme registerlarıdır"
                        .into(),
                );
            }
            if (v.area == Area::D && v.index > m.data_register_limit.unwrap_or(9999))
                || (v.area == Area::M && v.index > m.relay_limit.unwrap_or(4095))
                || v.to_string().parse::<Device>().is_err()
                || (matches!(v.area, Area::Y | Area::M | Area::S) && !v.writable_bit())
            {
                return Err(format!(
                    "{} hedef aralığı model belleği dışında",
                    self.name()
                ));
            }
            if v.area == Area::C && v.index >= 200 {
                return Err("VR komutunun 32 bit C200+ hedef erişimi kanıtlanmadı".into());
            }
        }
        Ok(())
    }
    pub fn execute(&self, m: &mut Memory, power: bool, _pc: u16) {
        if !power {
            return;
        }
        let result = self
            .validate_destination(m)
            .and_then(|()| m.analog_knob.value(self.channel, self.kind));
        match result {
            Ok(value) => self.destination.write(m, i32::from(value)),
            Err(reason) => {
                m.execution_limitations
                    .insert(format!("{}: {reason}", self.name()));
            }
        }
    }
}

/// All configured profiles are virtual. SA2/SX2 identifies the documented
/// two-knob model; Extension never claims an ES2/SS2 built-in ADC exists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KnobProfile {
    #[default]
    Unconfigured,
    VirtualSa2,
    VirtualSx2,
    VirtualExtension,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HalfwayPolicy {
    /// The manual says rounded off, but does not select the exact half tie.
    #[default]
    HoldUnspecified,
    HalfUp,
    HalfDown,
    HalfToEven,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KnobInput {
    pub raw: Option<u8>,
    /// Explicit measured graduation in thousandths,0..10000. This is NOT
    /// inferred from the independent raw eight-bit reading.
    pub scaled_milli: Option<u16>,
}
#[derive(Debug, Clone, Default)]
pub struct KnobState {
    pub profile: KnobProfile,
    pub halfway: HalfwayPolicy,
    pub notices: BTreeSet<String>,
    channels: [KnobInput; 2],
}
impl KnobState {
    pub fn set_profile(&mut self, profile: KnobProfile) {
        self.profile = profile;
        if profile != KnobProfile::Unconfigured {
            self.notices.insert(
                "Açık sanal VR girişi: ADC veya ES2/SS2 fiziksel ayar düğmesi eşdeğerliği değildir"
                    .into(),
            );
        }
    }
    pub fn input(&self, channel: u8) -> Result<KnobInput, String> {
        self.channels
            .get(usize::from(channel))
            .copied()
            .ok_or_else(|| "VR numarası0..1 olmalı".into())
    }
    fn input_mut(&mut self, channel: u8) -> Result<&mut KnobInput, String> {
        self.channels
            .get_mut(usize::from(channel))
            .ok_or_else(|| "VR numarası0..1 olmalı".into())
    }
    pub fn set_raw(&mut self, channel: u8, raw: u8) -> Result<(), String> {
        self.input_mut(channel)?.raw = Some(raw);
        Ok(())
    }
    pub fn set_graduation(&mut self, channel: u8, graduation: u8) -> Result<(), String> {
        if graduation > 10 {
            return Err("VR mezuniyeti0..10 olmalı".into());
        }
        self.set_scaled_milli(channel, u16::from(graduation) * 1000)
    }
    pub fn set_scaled_milli(&mut self, channel: u8, scaled_milli: u16) -> Result<(), String> {
        if scaled_milli > 10_000 {
            return Err("VR ölçekli ölçümü0..10000 binde mezuniyet olmalı".into());
        }
        self.input_mut(channel)?.scaled_milli = Some(scaled_milli);
        Ok(())
    }
    pub fn clear_input(&mut self, channel: u8) -> Result<(), String> {
        *self.input_mut(channel)? = KnobInput::default();
        Ok(())
    }
    pub fn value(&self, channel: u8, kind: KnobRead) -> Result<u16, String> {
        if self.profile == KnobProfile::Unconfigured {
            return Err("Sanal VR profili yapılandırılmadı; sahte sıfır ölçüm üretilmedi".into());
        }
        let input = self.input(channel)?;
        match kind {
            KnobRead::Raw => input
                .raw
                .map(u16::from)
                .ok_or_else(|| "Açık ham VR ölçümü yok".into()),
            KnobRead::Scaled => {
                let value = input.scaled_milli.ok_or(
                    "Açık ölçekli VR ölçümü yok; ham byte'tan kalibrasyon eğrisi türetilmez",
                )?;
                let integer = value / 1000;
                let fraction = value % 1000;
                if fraction < 500 {
                    return Ok(integer);
                }
                if fraction > 500 {
                    return Ok(integer + 1);
                }
                match self.halfway {
                    HalfwayPolicy::HoldUnspecified => Err("Tam yarım mezuniyette yuvarlama yönü PDF'de belirtilmedi; açık politika seçilmeli".into()),
                    HalfwayPolicy::HalfUp => Ok(integer + 1),
                    HalfwayPolicy::HalfDown => Ok(integer),
                    HalfwayPolicy::HalfToEven => Ok(integer + integer % 2),
                }
            }
        }
    }
    /// Explicit local publication seam, without an invented ADC clock. Direct
    /// VRRD/VRSC do not require enable flags (the PDF examples omit them).
    /// M1178/79 gate only automatic raw D1178/79 publication; OFF holds the word.
    pub fn service(m: &mut Memory) {
        if m.analog_knob.profile == KnobProfile::Unconfigured {
            return;
        }
        for channel in 0..2u8 {
            if m.system_bit(1178 + u16::from(channel))
                && let Some(raw) = m.analog_knob.channels[usize::from(channel)].raw
            {
                m.publish_analog_knob(channel, raw);
            }
        }
    }
}

#[cfg(test)]
#[path = "analog_knob_tests.rs"]
mod tests;
