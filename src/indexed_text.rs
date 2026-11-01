//! Ordered ASCI/HEX/CCD bindings and bounded physical text cursors.
//! All forms retain seven original WORDs. VM owns power and pulse history.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifiers, read_resolved_value};
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::text_encoding::{TextAction, check_byte, hex_digit, nibble};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextOutcome {
    Nominal,
    Complete,
    PlcError { offset: u16 },
    Limitation(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextVisit {
    pub outcome: TextOutcome,
    pub observed: BTreeSet<Device>,
}

/// Parsed actions and retained typed legacy constructors need the same
/// resumable source-VM logic and pulse history before native header checks.
pub fn is_instruction(action: &crate::plc::Action) -> bool {
    use crate::plc::Action;
    match action {
        Action::Pulse(inner) => is_instruction(inner),
        Action::IndexedText(_)
        | Action::Text(
            TextAction::Ascii { .. } | TextAction::Hex { .. } | TextAction::Check { .. },
        ) => true,
        _ => false,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ascii,
    Hex,
    Check,
}

fn scalar(d: Device) -> CalendarValue {
    CalendarValue::Additional(AdditionalValue::Word(Operand::Register(d)))
}
fn limit(area: Area, m: &Memory) -> u16 {
    match area {
        Area::X | Area::Y | Area::T | Area::C => 255,
        Area::S => 1023,
        Area::M => m.relay_limit.unwrap_or(4095),
        Area::D => m.data_register_limit.unwrap_or(9999),
    }
}
fn roles(value: &AddressedValue, write: bool, check: bool) -> Result<(), String> {
    match value {
        AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Word(
            Operand::Constant(_),
        ))) if !write && !check => Ok(()),
        AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Word(
            Operand::Register(base),
        ))) if matches!(base.area, Area::T | Area::C | Area::D) => {
            word_range(*base, 1)?;
            Ok(())
        }
        AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Group {
            base,
            digits,
            wide: false,
        })) if (1..=4).contains(digits)
            && matches!(base.area, Area::X | Area::Y | Area::M | Area::S)
            && (!write || base.area != Area::X)
            && (!write || !check || base.area != Area::Y) =>
        {
            Ok(())
        }
        AddressedValue::Modified {
            address,
            digits,
            wide: false,
        } if match digits {
            Some(n) => {
                (1..=4).contains(n)
                    && matches!(address.base.area, Area::X | Area::Y | Area::M | Area::S)
                    && (!write || address.base.area != Area::X)
                    && (!write || !check || address.base.area != Area::Y)
            }
            None => {
                matches!(address.base.area, Area::T | Area::C | Area::D)
                    && (address.base.area != Area::C || address.base.index < 200)
            }
        } =>
        {
            Ok(())
        }
        _ => Err(
            "ASCI/HEX/CCD rolü Kn veya WORD T/C/D olmalı; sabit yalnız ASCI/HEX kaynağında".into(),
        ),
    }
}

/// A modifier-free captured base. Numeric offsets thereafter are PHYSICAL
/// WORDs: C199, C200.low, C200.high, C201.low, ... . CCD applies stricter
/// transition checks separately; it must not silently borrow this wide path.
#[derive(Debug, Clone, Copy)]
enum Cursor {
    Literal(u16),
    Numeric(Device),
    Group { base: Device, digits: u16 },
}
impl Cursor {
    fn width(self) -> u16 {
        match self {
            Self::Group { digits, .. } => digits * 4,
            _ => 16,
        }
    }
    fn numeric(self, offset: u16, m: &Memory, checked: bool) -> Option<(Device, bool)> {
        let Self::Numeric(base) = self else {
            return None;
        };
        if base.area == Area::C {
            if checked {
                let index = base.index.checked_add(offset)?;
                return (index < 200).then_some((Device { index, ..base }, false));
            }
            let physical = if base.index < 200 {
                u32::from(base.index)
            } else {
                200 + 2 * u32::from(base.index - 200)
            } + u32::from(offset);
            let (index, high) = if physical < 200 {
                (physical, false)
            } else {
                (200 + (physical - 200) / 2, physical % 2 == 1)
            };
            return (index <= 255).then_some((
                Device {
                    index: index as u16,
                    ..base
                },
                high,
            ));
        }
        let index = base.index.checked_add(offset)?;
        (index <= limit(base.area, m)).then_some((Device { index, ..base }, false))
    }
    fn bit(self, offset: u16, m: &Memory, write: bool) -> Result<Option<Device>, String> {
        let Self::Group { base, .. } = self else {
            return Err("Kn cursor gerekli".into());
        };
        let Some(index) = base.index.checked_add(offset) else {
            return Ok(None);
        };
        if index > limit(base.area, m) {
            return Ok(None);
        }
        // These raw pointer loops do not repair the physical M1535 gap.
        if base.area == Area::M && base.index < 1536 && index >= 1536 {
            return Err("Metin ham Kn cursor M1535 geçişi native olarak doğrulanmadı".into());
        }
        let d = Device { index, ..base };
        if write {
            d.to_string()
                .parse::<Device>()
                .map_err(|_| "Metin cursor ayrılmış/uygulanmamış cihazı yazıyor")?;
            if !d.writable_bit() {
                return Err("Metin cursor salt okunur bit üzerine yazıyor".into());
            }
        }
        Ok(Some(d))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedTextAction {
    kind: Kind,
    pulse_label: bool,
    source: AddressedValue,
    target: AddressedValue,
    count: Operand,
}

struct Context<'a, F> {
    m: &'a mut Memory,
    observed: &'a mut BTreeSet<Device>,
    fetch: F,
    captured: BTreeSet<Device>,
    written: BTreeSet<Device>,
}
impl<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>> Context<'_, F> {
    fn capture(&mut self, devices: BTreeSet<Device>) -> Result<(), String> {
        let missing = devices
            .difference(&self.captured)
            .copied()
            .filter(|d| !self.written.contains(d))
            .collect::<BTreeSet<_>>();
        if !missing.is_empty() {
            (self.fetch)(self.m, &missing)?;
        }
        self.captured.extend(&devices);
        self.observed.extend(devices);
        Ok(())
    }
    fn bind(&mut self, binding: &AddressedValue, write: bool) -> Result<Option<Cursor>, String> {
        self.capture(binding.modifier_devices().into_iter().collect())?;
        let (base, digits) = match binding {
            AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Word(
                Operand::Constant(v),
            ))) => return Ok(Some(Cursor::Literal(*v as u16))),
            AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Word(
                Operand::Register(d),
            ))) => (*d, None),
            AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Group {
                base,
                digits,
                ..
            })) => (*base, Some(*digits)),
            AddressedValue::Modified {
                address, digits, ..
            } => {
                let raw = address
                    .base
                    .index
                    .wrapping_add(address.index.read(self.m) as u16)
                    as i16;
                let Ok(index) = u16::try_from(raw) else {
                    return Ok(None);
                };
                if index > limit(address.base.area, self.m) {
                    return Ok(None);
                }
                let base = Device {
                    index,
                    ..address.base
                };
                if let Some(digits) = digits {
                    qualify_group(address.base, base, digits * 4, true)?;
                }
                (base, *digits)
            }
            _ => return Err("Geçersiz metin binding türü".into()),
        };
        if base.index > limit(base.area, self.m) {
            return Ok(None);
        }
        base.to_string()
            .parse::<Device>()
            .map_err(|_| "Metin başlangıcının ayrılmış cihaz eşlemesi uygulanmadı")?;
        if digits.is_some() && write && !base.writable_bit() {
            return Err("Metin Kn başlangıcı salt okunur".into());
        }
        Ok(Some(match digits {
            Some(digits) => Cursor::Group { base, digits },
            None => Cursor::Numeric(base),
        }))
    }
    fn mode(&mut self) -> Result<bool, String> {
        self.capture(BTreeSet::from([Memory::system_device(1161)]))?;
        Ok(self.m.system_bit(1161))
    }
    fn word(&mut self, d: Device, high: bool) -> Result<u16, String> {
        self.capture(BTreeSet::from([d]))?;
        if d.area == Area::C && d.index >= 200 {
            Ok((self.m.double_word(d) as u32 >> if high { 16 } else { 0 }) as u16)
        } else {
            Ok(read_resolved_value(&scalar(d), self.m, self.observed)? as u16)
        }
    }
    fn read_bit(&mut self, d: Device) -> Result<bool, String> {
        d.to_string()
            .parse::<Device>()
            .map_err(|_| "Metin cursor ayrılmış/uygulanmamış biti okuyor")?;
        self.capture(BTreeSet::from([d]))?;
        if d.area == Area::M
            && (1011..=1014).contains(&d.index)
            && self.m.system_relays.clock_levels().is_none()
        {
            return Err("İndeksli metin saat biti için kurulu ilk seviye gerekli".into());
        }
        Ok(self.m.bit(d))
    }
    fn put_bit(&mut self, d: Device, value: bool) {
        self.m.set_internal(d, value);
        self.written.insert(d);
        self.observed.insert(d);
    }
    fn put_word(&mut self, d: Device, high: bool, value: u16) -> Result<(), String> {
        if d.area == Area::C && d.index >= 200 {
            self.capture(BTreeSet::from([d]))?;
            let old = self.m.double_word(d) as u32;
            self.m.set_double_word(
                d,
                if high {
                    (old & 0xffff) | (u32::from(value) << 16)
                } else {
                    (old & 0xffff0000) | u32::from(value)
                } as i32,
            );
        } else {
            self.m.set_word(d, value as i16);
        }
        self.written.insert(d);
        self.observed.insert(d);
        Ok(())
    }
    fn error(&mut self, offset: u16) -> Result<TextOutcome, String> {
        let mut ds = BTreeSet::new();
        Memory::error_devices(&mut ds);
        self.capture(ds.clone())?;
        self.m.operation_error(0xe1a, offset);
        self.written.extend(ds);
        Ok(TextOutcome::PlcError { offset })
    }
    fn source_scratch(&mut self, literal: bool) {
        // Preserve an unknown private low byte rather than fabricate zero.
        self.m.native_numeric_scratch = self
            .m
            .native_numeric_scratch
            .map(|old| ((old as u16 & 0xff) | (u16::from(literal) << 8)) as i16);
    }
    fn count(&mut self, value: &Operand) -> Result<Option<i16>, String> {
        match value {
            Operand::Constant(n) => Ok(Some(*n)),
            Operand::Register(d) => {
                if d.index > limit(d.area, self.m) {
                    return Ok(None);
                }
                self.word(*d, false).map(|v| Some(v as i16))
            }
        }
    }
    fn numeric_word(
        &mut self,
        cursor: Cursor,
        offset: u16,
        checked: bool,
    ) -> Result<Option<u16>, String> {
        if let Cursor::Literal(v) = cursor {
            return Ok(Some(v));
        }
        match cursor.numeric(offset, self.m, checked) {
            Some((d, high)) => self.word(d, high).map(Some),
            None => Ok(None),
        }
    }
    /// Group significance is relative to a physical 16-bit slot. Every
    /// consumed position still advances the cursor, even a masked-out bit.
    #[allow(clippy::too_many_arguments)] // Explicit native cursor/gate/advance controls.
    fn group_read(
        &mut self,
        cursor: Cursor,
        pos: u16,
        significant: u16,
        bits: u16,
        seven: bool,
        checked: bool,
        final_advance: bool,
    ) -> Result<Option<u16>, String> {
        let mut value = 0;
        for i in 0..bits {
            let at = pos + i;
            let reads = significant + i < cursor.width() && (!seven || i != 7) && i < 8;
            // HEX's raw cursor advances even when Kn/bit7 masks the payload.
            // Its branch skips the dereference before moving the pointer;
            // masked padding past an area's end is therefore not a read.
            // ASCI/CCD retain their separate checked-advance obligations.
            if !checked && !reads {
                continue;
            }
            let Some(d) = cursor.bit(at, self.m, false)? else {
                return if checked {
                    Ok(None)
                } else {
                    Err("Metin unchecked Kn kaynak aralığı model dışına taşıyor".into())
                };
            };
            if reads && self.read_bit(d)? {
                value |= 1 << i;
            }
            let rollover = d.index % 8 == 7;
            if checked
                && rollover
                && (i + 1 < bits || final_advance)
                && cursor.bit(at + 1, self.m, false)?.is_none()
            {
                return Ok(None);
            }
        }
        Ok(Some(value))
    }
    fn group_write(
        &mut self,
        cursor: Cursor,
        pos: u16,
        bits: u16,
        value: u16,
        checked: bool,
        final_advance: bool,
    ) -> Result<bool, String> {
        for i in 0..bits {
            let at = pos + i;
            let Some(d) = cursor.bit(at, self.m, true)? else {
                return if checked {
                    Ok(false)
                } else {
                    Err("Metin unchecked Kn hedef aralığı model dışına taşıyor".into())
                };
            };
            self.put_bit(d, at % 16 < cursor.width() && value & (1 << i) != 0);
            if checked
                && d.index % 8 == 7
                && (i + 1 < bits || final_advance)
                && cursor.bit(at + 1, self.m, true)?.is_none()
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl IndexedTextAction {
    fn spec(name: &str) -> Option<(Kind, bool)> {
        Some(match name.to_ascii_uppercase().as_str() {
            "ASCI" | "ASCII" => (Kind::Ascii, false),
            "ASCIP" | "ASCIIP" => (Kind::Ascii, true),
            "HEX" => (Kind::Hex, false),
            "HEXP" => (Kind::Hex, true),
            "CCD" => (Kind::Check, false),
            "CCDP" => (Kind::Check, true),
            _ => return None,
        })
    }
    pub fn supports(name: &str) -> bool {
        Self::spec(name).is_some()
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let (kind, pulse_label) = Self::spec(name).ok_or("ASCI/HEX/CCD komutu gerekli")?;
        if args.len() != 3 {
            return Err("Kaynak, hedef ve uzunluk gerekli".into());
        }
        let source = AddressedValue::parse(&args[0], false, false)?;
        let target = AddressedValue::parse(&args[1], false, true)?;
        roles(&source, false, kind == Kind::Check)?;
        roles(&target, true, kind == Kind::Check)?;
        let count = Operand::parse(&args[2].to_ascii_uppercase())?;
        match &count {
            Operand::Constant(n) if (1..=256).contains(n) => {}
            Operand::Register(d) if kind == Kind::Check && d.area == Area::D => {
                word_range(*d, 1)?;
            }
            _ => return Err("ASCI/HEX n=K/H1..256; CCD n=K/H1..256 veya düz D olmalı".into()),
        }
        Ok(Self {
            kind,
            pulse_label,
            source,
            target,
            count,
        })
    }
    /// Preserve existing typed constructors without formatting/reparsing IL.
    pub fn from_legacy(action: &TextAction) -> Result<Self, String> {
        let (kind, source, target, count) = match action {
            TextAction::Ascii {
                source,
                dest,
                count,
            } => (Kind::Ascii, source, dest, Operand::Constant(*count as i16)),
            TextAction::Hex {
                source,
                dest,
                count,
            } => (Kind::Hex, source, dest, Operand::Constant(*count as i16)),
            TextAction::Check {
                source,
                dest,
                count,
            } => (Kind::Check, source, dest, count.clone()),
            _ => return Err("ASCI/HEX/CCD eylemi gerekli".into()),
        };
        let source = AddressedValue::Direct(CalendarValue::Additional(source.value().clone()));
        let target = AddressedValue::Direct(CalendarValue::Additional(target.value().clone()));
        roles(&source, false, kind == Kind::Check)?;
        roles(&target, true, kind == Kind::Check)?;
        Ok(Self {
            kind,
            pulse_label: false,
            source,
            target,
            count,
        })
    }

    fn run<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        c: &mut Context<'_, F>,
    ) -> Result<TextOutcome, String> {
        let Some(source) = c.bind(&self.source, false)? else {
            return c.error(1);
        };
        if self.kind == Kind::Check {
            if matches!(
                source,
                Cursor::Numeric(Device {
                    area: Area::C,
                    index: 200..,
                })
            ) {
                return c.error(1);
            }
            let Some(n) = c.count(&self.count)? else {
                return c.error(5);
            };
            if !(1..=256).contains(&n) {
                return c.error(5);
            }
            return self.check(c, source, n as u16);
        }
        c.source_scratch(matches!(source, Cursor::Literal(_)));
        let Some(target) = c.bind(&self.target, true)? else {
            return c.error(3);
        };
        let Some(n) = c.count(&self.count)? else {
            return Err("Metin sabit uzunluğu kayboldu".into());
        };
        if !(1..=256).contains(&n) {
            return Err("ASCI/HEX yalnız belgeli sabit 1..256 uzunluğunda yürütülür".into());
        }
        match self.kind {
            Kind::Ascii => self.ascii(c, source, target, n as u16),
            Kind::Hex => self.hex(c, source, target, n as u16),
            Kind::Check => unreachable!(),
        }
    }
    fn ascii<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        c: &mut Context<'_, F>,
        source: Cursor,
        target: Cursor,
        n: u16,
    ) -> Result<TextOutcome, String> {
        let initial_low = c.mode()?;
        let mut numeric_offset = i32::from(if initial_low { n - 1 } else { (n - 1) / 2 });
        let mut group_end = i32::from(n) * if initial_low { 16 } else { 8 };
        for i in 0..n {
            let value = match source {
                Cursor::Group { .. } => {
                    let Some(v) = c.group_read(source, i * 4, (i % 4) * 4, 4, false, true, true)?
                    else {
                        return c.error(7);
                    };
                    v as u8
                }
                _ => {
                    let word = c
                        .numeric_word(source, i / 4, false)?
                        .ok_or("ASCI unchecked sayısal kaynak aralığı model dışına taşıyor")?;
                    (word >> (4 * (i % 4))) as u8
                }
            };
            let byte = hex_digit(value);
            let low = c.mode()?;
            let high = (n - 1 - i) % 2 == 1;
            match target {
                Cursor::Group { .. } => {
                    let bits = if low { 16 } else { 8 };
                    let start = group_end - bits;
                    if start < 0 {
                        return Err(
                            "ASCI canlı mod hedef cursor'u başlangıcın önüne taşıyor".into()
                        );
                    }
                    if i == 0 && !low && !high && target.width() > 8 {
                        for k in 0..target.width() - 8 {
                            let d = target
                                .bit(group_end as u16 + k, c.m, true)?
                                .ok_or("ASCI kullanılmayan üst byte model dışına taşıyor")?;
                            c.put_bit(d, false);
                        }
                    }
                    // Native consumes the destination backwards, with Kn's
                    // significant position tied to iteration parity.
                    for k in (0..bits as u16).rev() {
                        let d = target
                            .bit(start as u16 + k, c.m, true)?
                            .ok_or("ASCI unchecked Kn hedef aralığı model dışına taşıyor")?;
                        let significant = if low || !high { k } else { k + 8 };
                        c.put_bit(
                            d,
                            significant < target.width() && k < 8 && byte & (1 << k) != 0,
                        );
                    }
                    group_end = start;
                }
                Cursor::Numeric(_) => {
                    let offset = u16::try_from(numeric_offset)
                        .map_err(|_| "ASCI canlı mod sayısal cursor'u başlangıcın önüne taşıyor")?;
                    let (d, half) = target
                        .numeric(offset, c.m, false)
                        .ok_or("ASCI unchecked sayısal hedef aralığı model dışına taşıyor")?;
                    // This full WORD is read even where the branch replaces it.
                    let old = c.word(d, half)?;
                    let new = if low {
                        u16::from(byte)
                    } else if high {
                        (old & 0xff) | (u16::from(byte) << 8)
                    } else if i == 0 {
                        u16::from(byte)
                    } else {
                        (old & 0xff00) | u16::from(byte)
                    };
                    c.put_word(d, half, new)?;
                    if low || !high {
                        numeric_offset -= 1;
                    }
                }
                Cursor::Literal(_) => unreachable!(),
            }
        }
        Ok(TextOutcome::Complete)
    }
    fn hex<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        c: &mut Context<'_, F>,
        source: Cursor,
        target: Cursor,
        n: u16,
    ) -> Result<TextOutcome, String> {
        let mut numeric_offset = 0;
        let mut group_pos = 0;
        for i in 0..n {
            let low = c.mode()?;
            let byte = match source {
                Cursor::Group { .. } => {
                    let consumed = if low { 16 } else { 8 };
                    let sig = if low { 0 } else { (i % 2) * 8 };
                    let value = c
                        .group_read(source, group_pos, sig, consumed, true, false, false)?
                        .ok_or("HEX Kn kaynak cursor'u geçersiz")?;
                    group_pos += consumed;
                    value as u8
                }
                _ => {
                    let value = c
                        .numeric_word(source, numeric_offset, false)?
                        .ok_or("HEX unchecked sayısal kaynak aralığı model dışına taşıyor")?;
                    let byte = (value >> if low || i % 2 == 0 { 0 } else { 8 }) as u8;
                    if low || i % 2 == 1 {
                        numeric_offset += 1;
                    }
                    byte
                }
            };
            let Some(value) = nibble(byte) else {
                return c.error(7);
            };
            let at = n - 1 - i;
            let shift = (at % 4) * 4;
            match target {
                Cursor::Group { .. } => {
                    if i == 0 {
                        for b in shift + 4..16 {
                            let d = target
                                .bit((at / 4) * 16 + b, c.m, true)?
                                .ok_or("HEX öncü Kn temizleme aralığı model dışına taşıyor")?;
                            c.put_bit(d, false);
                        }
                    }
                    c.group_write(target, at * 4, 4, u16::from(value), false, false)?;
                }
                Cursor::Numeric(_) => {
                    let (d, high) = target
                        .numeric(at / 4, c.m, false)
                        .ok_or("HEX unchecked sayısal hedef aralığı model dışına taşıyor")?;
                    let old = c.word(d, high)?;
                    // Native's high-nibble assignment also clears the next
                    // low nibble in its byte; grouped writes do not.
                    let keep = match at % 4 {
                        3 => old & 0x00ff,
                        2 => old & if i == 0 { 0x00ff } else { 0xf0ff },
                        1 => {
                            if i == 0 {
                                0
                            } else {
                                old & 0xff00
                            }
                        }
                        _ => {
                            if i == 0 {
                                0
                            } else {
                                old & 0xfff0
                            }
                        }
                    };
                    c.put_word(d, high, keep | (u16::from(value) << shift))?;
                }
                Cursor::Literal(_) => unreachable!(),
            }
        }
        Ok(TextOutcome::Complete)
    }
    fn check<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        c: &mut Context<'_, F>,
        source: Cursor,
        n: u16,
    ) -> Result<TextOutcome, String> {
        let mut numeric_offset = 0;
        let mut group_pos = 0;
        let mut sum = 0;
        let mut parity = 0;
        for i in 0..n {
            let low = c.mode()?;
            let byte = match source {
                Cursor::Group { .. } => {
                    let consumed = if low { 16 } else { 8 };
                    let sig = if low { 0 } else { (i % 2) * 8 };
                    let Some(v) =
                        c.group_read(source, group_pos, sig, consumed, false, true, i + 1 < n)?
                    else {
                        return c.error(5);
                    };
                    group_pos += consumed;
                    v as u8
                }
                Cursor::Numeric(_) => {
                    let Some(v) = c.numeric_word(source, numeric_offset, true)? else {
                        return c.error(5);
                    };
                    let byte = (v >> if low || i % 2 == 0 { 0 } else { 8 }) as u8;
                    if low || i % 2 == 1 {
                        numeric_offset += 1;
                    }
                    byte
                }
                Cursor::Literal(_) => unreachable!(),
            };
            check_byte(&mut sum, &mut parity, byte);
        }
        c.m.native_numeric_scratch = Some(sum as i16);
        let Some(target) = c.bind(&self.target, true)? else {
            return c.error(3);
        };
        if matches!(
            target,
            Cursor::Numeric(Device {
                area: Area::C,
                index: 200..,
            })
        ) {
            return c.error(7);
        }
        for (offset, value) in [sum, parity].into_iter().enumerate() {
            if matches!(target, Cursor::Group { .. }) {
                if !c.group_write(target, offset as u16 * 16, 16, value, true, offset == 0)? {
                    return c.error(3);
                }
            } else {
                let Some((d, high)) = target.numeric(offset as u16, c.m, true) else {
                    return c.error(3);
                };
                c.put_word(d, high, value)?;
            }
        }
        Ok(TextOutcome::Complete)
    }

    pub fn execute(
        &self,
        m: &mut Memory,
        power: bool,
        observed: &mut BTreeSet<Device>,
    ) -> TextOutcome {
        if !power {
            return TextOutcome::Nominal;
        }
        let outcome = self
            .run(&mut Context {
                m,
                observed,
                fetch: |_: &mut Memory, _: &BTreeSet<Device>| Ok(()),
                captured: BTreeSet::new(),
                written: BTreeSet::new(),
            })
            .unwrap_or_else(TextOutcome::Limitation);
        if let TextOutcome::Limitation(reason) = &outcome {
            m.execution_limitations.insert(reason.clone());
        }
        outcome
    }
    /// Disposable actual-input replay. Capture failures are qualification
    /// failures, never invented PLC errors. Parent separately compares actual
    /// native/local header gates before executing the native instruction.
    pub fn qualify_native_with<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        m: &Memory,
        fetch: F,
    ) -> TextVisit {
        let mut clone = m.clone();
        let mut observed = BTreeSet::new();
        let outcome = self
            .run(&mut Context {
                m: &mut clone,
                observed: &mut observed,
                fetch,
                captured: BTreeSet::new(),
                written: BTreeSet::new(),
            })
            .unwrap_or_else(TextOutcome::Limitation);
        TextVisit { outcome, observed }
    }
    pub fn qualify_native(&self, m: &Memory) -> TextVisit {
        self.qualify_native_with(m, |_, _| Ok(()))
    }
    pub fn nominal_words(&self) -> u16 {
        7
    }
    pub fn dest(&self) -> Device {
        self.target.base().expect("validated text target")
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.source
            .modifier_devices()
            .into_iter()
            .chain(self.target.modifier_devices())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn control_devices(&self) -> Vec<Device> {
        vec![Memory::system_device(1161)]
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.source.devices(out);
        self.target.devices(out);
        if let Operand::Register(d) = self.count {
            out.insert(d);
        }
        out.extend(self.control_devices());
        Memory::error_devices(out);
        // Only actual initial storage is unconditional. Span lengths depend on
        // mode/count and checked CCD can retain a partial destination write.
    }
    pub fn monitor_input_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut ds: BTreeSet<_> = self.modifier_devices().into_iter().collect();
        ds.extend(self.control_devices());
        if let Operand::Register(d) = self.count {
            ds.insert(d);
        }
        validate_model(m, &ds)?;
        Ok(ds)
    }
    /// Potential bounded display candidates only; no payload or error replay.
    /// Caller must hydrate monitor_input_devices from valid actual rows first.
    pub fn capture_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut out = self.monitor_input_devices(m)?;
        let n = self.count.read(m);
        if !(1..=256).contains(&n) {
            return Ok(out);
        }
        let n = n as u16;
        let low = m.system_bit(1161);
        let mut copy = m.clone();
        let mut ignored = BTreeSet::new();
        let mut c = Context {
            m: &mut copy,
            observed: &mut ignored,
            fetch: |_: &mut Memory, _: &BTreeSet<Device>| Ok(()),
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
        };
        for (binding, write) in [(&self.source, false), (&self.target, true)] {
            let Some(cursor) = c.bind(binding, write)? else {
                continue;
            };
            let words = match (self.kind, write) {
                (Kind::Ascii, false) => n.div_ceil(4),
                (Kind::Ascii, true) => {
                    if low {
                        n
                    } else {
                        n.div_ceil(2)
                    }
                }
                (Kind::Hex, false) | (Kind::Check, false) => {
                    if low {
                        n
                    } else {
                        n.div_ceil(2)
                    }
                }
                (Kind::Hex, true) => n.div_ceil(4),
                (Kind::Check, true) => 2,
            };
            match cursor {
                Cursor::Literal(_) => {}
                Cursor::Numeric(_) => {
                    for i in 0..words {
                        if let Some((d, _)) = cursor.numeric(i, m, self.kind == Kind::Check) {
                            out.insert(d);
                        } else {
                            break;
                        }
                    }
                }
                Cursor::Group { .. } => {
                    for i in 0..words * 16 {
                        if let Some(d) = cursor.bit(i, m, write)? {
                            if d.to_string().parse::<Device>().is_ok() {
                                out.insert(d);
                            }
                        } else {
                            break;
                        }
                    }
                }
            }
        }
        Ok(out)
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(ds) = self.capture_devices(m) {
            out.extend(ds);
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        let mut copy = m.clone();
        let mut seen = BTreeSet::new();
        let mut c = Context {
            m: &mut copy,
            observed: &mut seen,
            fetch: |_: &mut Memory, _: &BTreeSet<Device>| Ok(()),
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
        };
        match c.bind(&self.target, true) {
            Ok(Some(Cursor::Numeric(d))) => format!(
                "H{:04X}",
                if d.area == Area::C && d.index >= 200 {
                    m.double_word(d) as u16
                } else {
                    m.word(d) as u16
                }
            ),
            Ok(Some(Cursor::Group { base, digits })) => {
                let value = (0..digits * 4).fold(0u16, |v, bit| {
                    v | (u16::from(m.bit(Device {
                        index: base.index + bit,
                        ..base
                    })) << bit)
                });
                format!("H{value:04X}")
            }
            _ => "Metin hedefi çözümlenemedi".into(),
        }
    }
    pub fn label(&self) -> String {
        format!(
            "{}{} {} {} {}",
            match self.kind {
                Kind::Ascii => "ASCI",
                Kind::Hex => "HEX",
                Kind::Check => "CCD",
            },
            if self.pulse_label { "P" } else { "" },
            self.source,
            self.target,
            self.count
        )
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut out = vec![
            match self.kind {
                Kind::Ascii => 0xb400,
                Kind::Hex => 0xb600,
                Kind::Check => 0xb800,
            } | if self.pulse_label { 0x10 } else { 0 },
        ];
        out.extend(self.source.native_words()?);
        out.extend(self.target.native_words()?);
        out.extend(crate::es2_encoder::calendar_operand(
            &CalendarValue::Additional(AdditionalValue::Word(self.count.clone())),
        )?);
        Ok(out)
    }
}
