//! Ordinary 16-bit CNT and direct numeric RST operands. CNT history belongs to its
//! instruction PC; reset inhibition survives END until a visited false RST.
use crate::calendar_operand::CalendarDestination;
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordCounterPreset {
    Constant(i16),
    Data(Device),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordCounterAction {
    pub counter: Device,
    pub preset: WordCounterPreset,
}
impl WordCounterAction {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        if args.len() != 2 {
            return Err("CNT C0..C199 K0..K32767/D bekleniyor".into());
        }
        let counter: Device = args[0].parse()?;
        if counter.area != Area::C || counter.index >= 200 {
            return Err("CNT hedefi C0..C199 olmalı".into());
        }
        let preset = if let Some(raw) = args[1].strip_prefix('K') {
            WordCounterPreset::Constant(
                raw.parse::<i16>()
                    .ok()
                    .filter(|v| *v >= 0)
                    .ok_or("CNT sabiti K0..K32767 olmalı")?,
            )
        } else if let Some(raw) = args[1].strip_prefix('H') {
            WordCounterPreset::Constant(
                i16::from_str_radix(raw, 16)
                    .ok()
                    .filter(|v| *v >= 0)
                    .ok_or("CNT hex sabiti H0..H7FFF olmalı")?,
            )
        } else {
            let d: Device = args[1].parse()?;
            if d.area != Area::D {
                return Err("CNT değişken ayarı bir D kelimesi olmalı".into());
            }
            WordCounterPreset::Data(d)
        };
        Ok(Self { counter, preset })
    }
    pub fn label(&self) -> String {
        let preset = match self.preset {
            WordCounterPreset::Constant(v) => format!("K{v}"),
            WordCounterPreset::Data(d) => d.to_string(),
        };
        format!("CNT {} {preset}", self.counter)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.insert(self.counter);
        if let WordCounterPreset::Data(d) = self.preset {
            out.insert(d);
        }
    }
    pub fn validate_span(&self, m: &Memory) -> Result<(), String> {
        if let WordCounterPreset::Data(d) = self.preset
            && d.index > m.data_register_limit.unwrap_or(9999)
        {
            return Err("CNT ayar D kelimesi seçilen model aralığını aşıyor".into());
        }
        Ok(())
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        // The native powered handler reads the preset before updating history.
        if power && let Err(reason) = self.validate_span(m) {
            m.execution_limitations.insert(reason);
            return;
        }
        let preset = if power {
            match self.preset {
                WordCounterPreset::Constant(v) => v,
                WordCounterPreset::Data(d) => m.word(d),
            }
        } else {
            0
        };
        let was = m
            .word_counters
            .edges
            .insert(pc.wrapping_add(3), power)
            .unwrap_or(false);
        if !power
            || was
            || m.bit(self.counter)
            || m.word_counters.reset_inhibit.contains(&self.counter)
        {
            return;
        }
        let next = m.word(self.counter).wrapping_add(1);
        let done = next >= preset;
        m.set_word(self.counter, if done { preset } else { next });
        if done {
            m.set_bit(self.counter, true);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WordCounterState {
    edges: BTreeMap<u16, bool>,
    reset_inhibit: BTreeSet<Device>,
}
impl WordCounterState {
    pub fn visit_reset(m: &mut Memory, counter: Device, power: bool) {
        if power {
            m.word_counters.reset_inhibit.insert(counter);
            m.reset(counter);
        } else {
            m.word_counters.reset_inhibit.remove(&counter);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NumericResetAction(CalendarDestination);
impl NumericResetAction {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        if args.len() != 1 {
            return Err("RST bir D/E/F kelimesi gerektirir".into());
        }
        let raw = &args[0];
        if !raw.starts_with(['D', 'E', 'F']) || crate::indexed_operand::has_modifier(raw) {
            return Err("Sayısal RST hedefi doğrudan D/E/F kelimesi olmalı".into());
        }
        let target = CalendarDestination::parse(raw, false)?;
        if target.base().area != Area::D {
            return Err("Sayısal RST hedefi D/E/F olmalı".into());
        }
        Ok(Self(target))
    }
    pub fn dest(&self) -> Device {
        self.0.base()
    }
    pub fn target(&self) -> &CalendarDestination {
        &self.0
    }
    pub fn label(&self) -> String {
        format!("RST {}", self.0)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.0.devices(out);
    }
    pub fn validate_span(&self, m: &Memory) -> Result<(), String> {
        if self.dest().index > m.data_register_limit.unwrap_or(9999) {
            return Err("RST D kelimesi seçilen model aralığını aşıyor".into());
        }
        Ok(())
    }
    pub fn execute(&self, m: &mut Memory, power: bool) {
        if !power {
            return;
        }
        match self.validate_span(m) {
            Ok(()) => self.0.write(m, 0),
            Err(reason) => {
                m.execution_limitations.insert(reason);
            }
        }
    }
}
