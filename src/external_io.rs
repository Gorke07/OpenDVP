//! API70/71/72/74/75/77 bounded virtual peripherals. Original ES2/SS2
//! handlers are PC-only stubs; this is an explicit documented local profile.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarIndices, CalendarValue};
use crate::plc::{Area, Device, Memory, Operand};
use crate::shift::bit_span;
use crate::timing_io::ResourceOwner;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn d(area: Area, index: u16) -> Device {
    Device { area, index }
}
fn limit(m: &mut Memory, s: impl Into<String>) {
    m.execution_limitations.insert(s.into());
}
fn literal(raw: &str, max: u8, min: u8) -> Result<u8, String> {
    let v = if let Some(n) = raw.strip_prefix('K') {
        n.parse::<u8>().ok()
    } else {
        raw.strip_prefix('H')
            .and_then(|n| u8::from_str_radix(n, 16).ok())
    };
    v.filter(|v| *v >= min && *v <= max)
        .ok_or_else(|| format!("K/H{min}..{max} sabiti gerekli"))
}
fn bits(raw: &str, n: u16, write: bool, x_only: bool, y_only: bool) -> Result<Device, String> {
    let b: Device = raw.parse()?;
    bit_span(b, n, write)?;
    if x_only && b.area != Area::X || y_only && b.area != Area::Y {
        return Err("Harici I/O giriş X, fiziksel çıkış Y olmalı".into());
    }
    Ok(b)
}
fn target(raw: &str, wide: bool, grouped: bool) -> Result<CalendarValue, String> {
    if raw.starts_with(['E', 'F']) {
        return CalendarValue::parse(raw, wide);
    }
    let a = AdditionalValue::parse(raw, wide, true)?;
    if !grouped && matches!(a, AdditionalValue::Group { .. }) {
        return Err("Bu hedef yalnız T/C/D/E/F register olabilir".into());
    }
    Ok(CalendarValue::Additional(a))
}
#[derive(Debug, Clone, PartialEq)]
pub enum ExternalIoAction {
    TenKey {
        wide: bool,
        source: Device,
        value: CalendarValue,
        status: Device,
    },
    HexKey {
        wide: bool,
        source: Device,
        strobe: Device,
        value: CalendarValue,
        status: Device,
    },
    Dip {
        source: Device,
        strobe: Device,
        value: Device,
        groups: u8,
    },
    Segment {
        source: CalendarValue,
        output: Device,
        mode: u8,
    },
    Arrows {
        source: Device,
        value: CalendarValue,
        output: Device,
        mode: u8,
    },
    Print {
        source: Device,
        output: Device,
    },
}
impl ExternalIoAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let wide = name.starts_with('D');
        match (name, args.len()) {
            ("TKY" | "DTKY", 3) => Ok(Self::TenKey {
                wide,
                source: bits(&args[0], 10, false, false, false)?,
                value: target(&args[1], wide, true)?,
                status: bits(&args[2], 11, true, false, false)?,
            }),
            ("HKY" | "DHKY", 4) => Ok(Self::HexKey {
                wide,
                source: bits(&args[0], 4, false, true, false)?,
                strobe: bits(&args[1], 4, true, false, true)?,
                value: target(&args[2], wide, false)?,
                status: bits(&args[3], 8, true, false, false)?,
            }),
            ("DSW", 4) => {
                let groups = literal(&args[3], 2, 1)?;
                let value: Device = args[2].parse()?;
                if !matches!(value.area, Area::T | Area::C | Area::D) {
                    return Err("DSW hedefi T/C/D olmalı".into());
                }
                Ok(Self::Dip {
                    source: bits(&args[0], u16::from(groups) * 4, false, true, false)?,
                    strobe: bits(&args[1], 4, true, false, true)?,
                    value,
                    groups,
                })
            }
            ("SEGL", 3) => {
                let mode = literal(&args[2], 7, 0)?;
                Ok(Self::Segment {
                    source: CalendarValue::parse(&args[0], false)?,
                    output: bits(&args[1], if mode >= 4 { 12 } else { 8 }, true, false, true)?,
                    mode,
                })
            }
            ("ARWS", 4) => {
                let output = bits(&args[2], 8, true, false, true)?;
                if output.index % 8 != 0 {
                    return Err("ARWS Y adresi sekizlik10 katı olmalı".into());
                }
                Ok(Self::Arrows {
                    source: bits(&args[0], 4, false, false, false)?,
                    value: target(&args[1], false, false)?,
                    output,
                    mode: literal(&args[3], 3, 0)?,
                })
            }
            ("PR", 2) => {
                let source: Device = args[0].parse()?;
                if !matches!(source.area, Area::T | Area::C | Area::D) {
                    return Err("PR kaynağı T/C/D olmalı".into());
                }
                Ok(Self::Print {
                    source,
                    output: bits(&args[1], 10, true, false, true)?,
                })
            }
            _ => Err(
                "TKY/DTKY/HKY/DHKY/DSW/SEGL/ARWS/PR operandları geçersiz; P formu belgelenmedi"
                    .into(),
            ),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::TenKey { wide: false, .. } => "TKY",
            Self::TenKey { wide: true, .. } => "DTKY",
            Self::HexKey { wide: false, .. } => "HKY",
            Self::HexKey { wide: true, .. } => "DHKY",
            Self::Dip { .. } => "DSW",
            Self::Segment { .. } => "SEGL",
            Self::Arrows { .. } => "ARWS",
            Self::Print { .. } => "PR",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::TenKey { wide: false, .. } => 0x9C00,
            Self::TenKey { wide: true, .. } => 0x9D00,
            Self::HexKey { wide: false, .. } => 0x9E00,
            Self::HexKey { wide: true, .. } => 0x9F00,
            Self::Dip { .. } => 0xA000,
            Self::Segment { .. } => 0xA400,
            Self::Arrows { .. } => 0xA600,
            Self::Print { .. } => 0xAA00,
        }
    }
    pub fn words(&self) -> u16 {
        match self {
            Self::TenKey { wide: false, .. } | Self::Segment { .. } => 7,
            Self::TenKey { wide: true, .. } => 13,
            Self::HexKey { wide: false, .. } | Self::Dip { .. } | Self::Arrows { .. } => 9,
            Self::HexKey { wide: true, .. } => 17,
            Self::Print { .. } => 5,
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::TenKey { value, .. }
            | Self::HexKey { value, .. }
            | Self::Arrows { value, .. } => value.base().expect("writable value"),
            Self::Dip { value, .. } => *value,
            Self::Segment { output, .. } | Self::Print { output, .. } => *output,
        }
    }
    pub fn destination(&self) -> Device {
        self.dest()
    }
    pub fn wide(&self) -> bool {
        matches!(
            self,
            Self::TenKey { wide: true, .. } | Self::HexKey { wide: true, .. }
        )
    }
    pub fn label(&self) -> String {
        match self {
            Self::TenKey {
                source,
                value,
                status,
                ..
            } => format!("{} {source} {value} {status}", self.name()),
            Self::HexKey {
                source,
                strobe,
                value,
                status,
                ..
            } => format!("{} {source} {strobe} {value} {status}", self.name()),
            Self::Dip {
                source,
                strobe,
                value,
                groups,
            } => format!("DSW {source} {strobe} {value} K{groups}"),
            Self::Segment {
                source,
                output,
                mode,
            } => format!("SEGL {source} {output} K{mode}"),
            Self::Arrows {
                source,
                value,
                output,
                mode,
            } => format!("ARWS {source} {value} {output} K{mode}"),
            Self::Print { source, output } => format!("PR {source} {output}"),
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        match self {
            Self::TenKey { value, .. }
            | Self::HexKey { value, .. }
            | Self::Arrows { value, .. } => value.read(m).to_string(),
            Self::Dip { value, .. } => m.word(*value).to_string(),
            Self::Segment { source, .. } => source.read(m).to_string(),
            Self::Print { .. } => m
                .external_io
                .runs
                .values()
                .find(|r| r.action == *self)
                .map(|r| format!("ASCII {}/{}", r.sent, r.bytes.len()))
                .unwrap_or_else(|| "ASCII çıkışı".into()),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        let (input, output) = self.pin_spans();
        out.extend(input);
        out.extend(output);
        match self {
            Self::TenKey { value, status, .. } => {
                value.devices(out);
                out.extend(span(*status, 11));
            }
            Self::HexKey { value, status, .. } => {
                value.devices(out);
                out.extend(span(*status, 8));
                out.insert(d(Area::M, 1167));
            }
            Self::Dip { value, groups, .. } => {
                out.extend(span(*value, u16::from(*groups)));
            }
            Self::Segment { source, mode, .. } => {
                source.devices(out);
                if *mode >= 4
                    && let Ok(second) = next(source)
                {
                    second.devices(out);
                }
            }
            Self::Arrows { value, .. } => value.devices(out),
            Self::Print { source, .. } => {
                out.extend(span(*source, 8));
                out.insert(d(Area::M, 1027));
            }
        }
        out.insert(d(Area::M, 1029));
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Self::Print { source, .. } = self
            && m.system_bit(1027)
            && m.external_io.long_packing == LongPacking::LowByteWords
        {
            out.extend(span(*source, 16));
        }
    }
    fn family(&self) -> u8 {
        match self {
            Self::TenKey { .. } => 0,
            Self::HexKey { .. } => 1,
            Self::Dip { .. } => 2,
            Self::Segment { .. } => 3,
            Self::Arrows { .. } => 4,
            Self::Print { .. } => 5,
        }
    }
    fn pin_spans(&self) -> (Vec<Device>, Vec<Device>) {
        match self {
            Self::TenKey { source, status, .. } => (
                span(*source, 10),
                if status.area == Area::Y {
                    span(*status, 11)
                } else {
                    vec![]
                },
            ),
            Self::HexKey {
                source,
                strobe,
                status,
                ..
            } => {
                let mut output = span(*strobe, 4);
                if status.area == Area::Y {
                    output.extend(span(*status, 8));
                }
                (span(*source, 4), output)
            }
            Self::Dip {
                source,
                strobe,
                groups,
                ..
            } => (span(*source, u16::from(*groups) * 4), span(*strobe, 4)),
            Self::Segment { output, mode, .. } => {
                (vec![], span(*output, if *mode >= 4 { 12 } else { 8 }))
            }
            Self::Arrows { source, output, .. } => (span(*source, 4), span(*output, 8)),
            Self::Print { output, .. } => (vec![], span(*output, 10)),
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let now = m.timing_io.now_us;
        if now != m.external_io.now_us {
            let raw = m.timing_io.physical_inputs.clone();
            ExternalIoState::advance(m, now, &raw);
        }
        let mut state = std::mem::take(&mut m.external_io);
        state.visit(m, self, power, pc);
        m.external_io = state;
    }
}
fn span(base: Device, n: u16) -> Vec<Device> {
    (0..n).map(|i| d(base.area, base.index + i)).collect()
}
fn check(m: &Memory, value: &CalendarValue) -> Result<(), String> {
    let mut set = BTreeSet::new();
    value.devices(&mut set);
    if set.iter().any(|v| !fits(m, *v)) {
        return Err("Harici I/O operandının tam aralığı model belleğini aşıyor".into());
    }
    if let Some(v) = value.base()
        && v.area == Area::C
        && (!value.wide() && v.index >= 200 || value.wide() && v.index == 199)
    {
        return Err("Harici I/O doğal C kelime/genişlik sınırı eşlemesi doğrulanmadı".into());
    }
    Ok(())
}
fn fits(m: &Memory, v: Device) -> bool {
    v.index
        <= match v.area {
            Area::D => m.data_register_limit.unwrap_or(9999),
            Area::M => m.relay_limit.unwrap_or(4095),
            Area::S => 1023,
            _ => 255,
        }
}
fn write(m: &mut Memory, value: &CalendarValue, v: i32) {
    match value {
        CalendarValue::Additional(a) => a.write(m, v),
        CalendarValue::Index {
            index,
            wide,
            high_bank,
        } => {
            if *wide {
                CalendarIndices::write_double(m, *index, v).expect("typed index");
            } else {
                CalendarIndices::write_word(m, *index, *high_bank, v as i16).expect("typed index");
            }
        }
    }
}
fn next(value: &CalendarValue) -> Result<CalendarValue, String> {
    match value {
        CalendarValue::Additional(AdditionalValue::Word(Operand::Register(v))) => Ok(
            CalendarValue::Additional(AdditionalValue::Word(Operand::Register(d(
                v.area,
                v.index.checked_add(1).ok_or("SEGL kaynak taşıması")?,
            )))),
        ),
        CalendarValue::Additional(AdditionalValue::Group {
            base,
            digits,
            wide: false,
        }) => AdditionalValue::parse(
            &format!(
                "K{digits}{}",
                d(
                    base.area,
                    base.index
                        .checked_add(digits * 4)
                        .ok_or("SEGL grup taşıması")?
                )
            ),
            false,
            false,
        )
        .map(CalendarValue::Additional),
        CalendarValue::Index {
            index, high_bank, ..
        } => {
            let v = CalendarIndices::alias_device(*index, *high_bank)?;
            Ok(CalendarValue::Additional(AdditionalValue::Word(
                Operand::Register(d(v.area, v.index + 1)),
            )))
        }
        _ => Err("İki setli SEGL sabit kaynağının ikinci değeri PDF'de açıklanmadı".into()),
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LongPacking {
    #[default]
    PackedWords,
    LowByteWords,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VisitCadence {
    #[default]
    EachInvocation,
    OncePerScan,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LongHeld {
    RepeatMessage,
    #[default]
    HoldCompleted,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ArrowBus {
    #[default]
    SelectedDigit,
    MultiplexDisplay,
}
#[derive(Debug, Clone)]
struct Run {
    action: ExternalIoAction,
    active: bool,
    phase: u8,
    previous: u16,
    selected: u8,
    keys: u16,
    locked: Option<u8>,
    deadline: Option<u64>,
    digits: [[u8; 4]; 2],
    bytes: Vec<u8>,
    observed_bytes: Vec<u8>,
    sent: usize,
    long: bool,
    stop_after: bool,
    done: bool,
    last_epoch: u64,
}
impl Run {
    fn new(action: ExternalIoAction) -> Self {
        Self {
            action,
            active: true,
            phase: 0,
            previous: 0,
            selected: 3,
            keys: 0,
            locked: None,
            deadline: None,
            digits: [[0; 4]; 2],
            bytes: vec![],
            observed_bytes: vec![],
            sent: 0,
            long: false,
            stop_after: false,
            done: false,
            last_epoch: u64::MAX,
        }
    }
}
#[derive(Debug, Clone)]
struct KeyEvent {
    pc: u16,
    key: u8,
    down: bool,
}
#[derive(Debug, Clone)]
pub struct ExternalIoState {
    pub now_us: u64,
    pub cadence: VisitCadence,
    pub long_packing: LongPacking,
    pub long_held: LongHeld,
    pub arrow_bus: ArrowBus,
    pub notices: BTreeSet<String>,
    pub display_latches: BTreeMap<u16, [u16; 2]>,
    pub display_commands: BTreeMap<u16, [u16; 2]>,
    pub printer_frames: BTreeMap<u16, VecDeque<Vec<u8>>>,
    pub printer_commands: BTreeMap<u16, VecDeque<Vec<u8>>>,
    runs: BTreeMap<u16, Run>,
    keyboards: BTreeMap<u16, BTreeMap<u8, u64>>,
    dips: BTreeMap<u16, [u16; 2]>,
    events: BTreeMap<(u64, u64), KeyEvent>,
    sequence: u64,
    raw: BTreeMap<Device, bool>,
    epoch: u64,
    completion_emitted: bool,
    completion_aged: bool,
    completion_revision: u64,
    in_scan: bool,
}
impl Default for ExternalIoState {
    fn default() -> Self {
        Self {
            now_us: 0,
            cadence: VisitCadence::EachInvocation,
            long_packing: LongPacking::PackedWords,
            long_held: LongHeld::HoldCompleted,
            arrow_bus: ArrowBus::SelectedDigit,
            notices: BTreeSet::new(),
            display_latches: BTreeMap::new(),
            display_commands: BTreeMap::new(),
            printer_frames: BTreeMap::new(),
            printer_commands: BTreeMap::new(),
            runs: BTreeMap::new(),
            keyboards: BTreeMap::new(),
            dips: BTreeMap::new(),
            events: BTreeMap::new(),
            sequence: 0,
            raw: BTreeMap::new(),
            epoch: 0,
            completion_emitted: false,
            completion_aged: false,
            completion_revision: 0,
            in_scan: false,
        }
    }
}
impl ExternalIoState {
    pub fn set_keyboard(&mut self, pc: u16, keys: &[u8]) -> Result<(), String> {
        if keys.iter().any(|k| *k > 15)
            || self.keyboards.len() >= 512 && !self.keyboards.contains_key(&pc)
        {
            return Err("Sanal HKY klavyesi0..15, en fazla512site olmalı".into());
        }
        let old = self.keyboards.entry(pc).or_default();
        old.retain(|k, _| keys.contains(k));
        for &key in keys {
            if let std::collections::btree_map::Entry::Vacant(e) = old.entry(key) {
                self.sequence = self.sequence.wrapping_add(1);
                e.insert(self.sequence);
            }
        }
        self.notices.insert(format!(
            "HKY STEP{pc}: açık sanal matris tuş kaynağı bağlı; fiziksel gecikme kanıtı değildir"
        ));
        Ok(())
    }
    pub fn queue_key(&mut self, at_us: u64, pc: u16, key: u8, down: bool) -> Result<(), String> {
        if at_us < self.now_us || key > 15 || self.events.len() >= 4096 {
            return Err("Sanal tuş olayı geçmişte/geçersiz veya4096sınırını aşıyor".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Tuş olay sırası taştı")?;
        self.events
            .insert((at_us, self.sequence), KeyEvent { pc, key, down });
        Ok(())
    }
    pub fn set_dip(&mut self, pc: u16, values: [u16; 2]) -> Result<(), String> {
        if values.iter().any(|v| *v > 9999)
            || self.dips.len() >= 512 && !self.dips.contains_key(&pc)
        {
            return Err("DSW sanal değerleri0..9999/en fazla512site olmalı".into());
        }
        self.dips.insert(pc, values);
        self.notices.insert(format!(
            "DSW STEP{pc}: açık sanal BCD anahtar kaynağı bağlı"
        ));
        Ok(())
    }
    pub fn selected_digit(&self, pc: u16) -> Option<u8> {
        self.runs.get(&pc).map(|r| r.selected)
    }
    pub fn active_count(&self) -> usize {
        self.runs.values().filter(|r| r.active).count()
    }
    pub fn claimed_outputs(&self) -> BTreeMap<Device, ResourceOwner> {
        self.runs
            .iter()
            .filter(|(_, r)| r.active)
            .flat_map(|(&pc, r)| {
                r.action
                    .pin_spans()
                    .1
                    .into_iter()
                    .map(move |v| (v, ResourceOwner::External(format!("EXTIO:{pc}"))))
            })
            .collect()
    }
    pub fn claimed_inputs(&self) -> BTreeMap<Device, ResourceOwner> {
        self.runs
            .iter()
            .filter(|(_, r)| r.active)
            .flat_map(|(&pc, r)| {
                r.action
                    .pin_spans()
                    .0
                    .into_iter()
                    .filter(|v| v.area == Area::X)
                    .map(move |v| (v, ResourceOwner::External(format!("EXTIO:{pc}"))))
            })
            .collect()
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.runs
            .values()
            .filter(|r| r.active)
            .filter_map(|r| r.deadline)
            .chain(self.events.first_key_value().map(|(k, _)| k.0))
            .min()
    }
    pub fn begin_scan(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.external_io);
        s.epoch = s.epoch.wrapping_add(1);
        if s.completion_emitted && s.completion_aged {
            if m.completion_flag_revision() == s.completion_revision {
                m.set_system(1029, false);
            }
            s.completion_emitted = false;
        }
        s.completion_aged = s.completion_emitted;
        s.in_scan = true;
        for r in s.runs.values().filter(|r| r.active) {
            if let ExternalIoAction::HexKey { status, .. } = r.action {
                set_bits(m, status, 6, 2, 0);
            }
        }
        m.external_io = s;
    }
    pub fn end_scan(m: &mut Memory) {
        m.external_io.in_scan = false;
    }
    pub fn abort(m: &mut Memory) {
        m.external_io.runs.clear();
        m.timing_io.input_owners.retain(|_, v| !is_ours(v));
        m.timing_io.output_owners.retain(|_, v| !is_ours(v));
    }
    fn emit(&mut self, m: &mut Memory) {
        m.set_system(1029, true);
        self.completion_revision = m.completion_flag_revision();
        self.completion_emitted = true;
        self.completion_aged = self.in_scan;
    }
    fn claims(&self, m: &mut Memory) {
        m.timing_io.input_owners.retain(|_, v| !is_ours(v));
        m.timing_io.output_owners.retain(|_, v| !is_ours(v));
        for (v, o) in self.claimed_inputs() {
            m.timing_io.input_owners.entry(v).or_insert(o);
        }
        for (v, o) in self.claimed_outputs() {
            m.timing_io.output_owners.entry(v).or_insert(o);
        }
    }
    fn can_claim(&self, m: &Memory, a: &ExternalIoAction, pc: u16) -> Result<(), String> {
        let family = a.family();
        let max = if family == 5 { 2 } else { 1 };
        if self
            .runs
            .iter()
            .filter(|(id, r)| **id != pc && r.active && r.action.family() == family)
            .count()
            >= max
        {
            return Err(format!(
                "{} aynı anda en fazla{max}etkin site kullanabilir",
                a.name()
            ));
        }
        let owner = ResourceOwner::External(format!("EXTIO:{pc}"));
        let (input, output) = a.pin_spans();
        let (matrix_inputs, matrix_outputs) = m.refresh_io.matrix_pins();
        if input.iter().any(|pin| matrix_inputs.contains(pin))
            || output.iter().any(|pin| matrix_outputs.contains(pin))
        {
            return Err("Harici I/O uçları etkin MTR matrisine ait".into());
        }
        if input.iter().chain(output.iter()).any(|v| !fits(m, *v)) {
            return Err("Harici I/O tam pin aralığı model sınırını aşıyor".into());
        }
        if input
            .iter()
            .filter(|v| v.area == Area::X)
            .any(|v| m.timing_io.input_owners.get(v).is_some_and(|o| o != &owner))
            || output.iter().any(|v| {
                m.timing_io
                    .output_owners
                    .get(v)
                    .is_some_and(|o| o != &owner)
            })
        {
            return Err(
                "Harici I/O giriş/çıkışı MTR/darbe/hız/sayaç/başka çevre birimine ait".into(),
            );
        }
        if input.iter().filter(|v|v.area==Area::X).any(|v|m.interrupts.definitions().keys().any(|i|matches!(i,crate::interrupt::InterruptId::External{input,..} if u16::from(*input)==v.index))) {return Err("Harici I/O matrisi harici kesme girişini paylaşamaz; fiziksel öncelik doğrulanmadı".into());}
        Ok(())
    }
    fn visit(&mut self, m: &mut Memory, a: &ExternalIoAction, power: bool, pc: u16) {
        if let ExternalIoAction::TenKey { status, .. } | ExternalIoAction::HexKey { status, .. } = a
        {
            let count = if matches!(a, ExternalIoAction::TenKey { .. }) {
                11
            } else {
                8
            };
            if span(*status, count).iter().any(|v| !fits(m, *v)) {
                limit(m, "Tuş durumunun tam bit aralığı model belleğini aşıyor");
                return;
            }
        }
        if !power {
            if let Some(mut r) = self.runs.remove(&pc)
                && r.active
            {
                if let ExternalIoAction::Print { .. } = r.action {
                    if r.long && !r.done {
                        r.stop_after = true;
                        if self.cadence != VisitCadence::OncePerScan || r.last_epoch != self.epoch {
                            r.last_epoch = self.epoch;
                            self.print_step(m, pc, &mut r);
                        }
                        self.runs.insert(pc, r);
                        self.claims(m);
                        return;
                    }
                    if r.long && r.done {
                        self.emit(m);
                    }
                }
                self.off(m, &r.action);
                r.active = false;
            }
            match a {
                ExternalIoAction::TenKey { status, .. } => set_bits(m, *status, 0, 11, 0),
                ExternalIoAction::HexKey { status, .. } => set_bits(m, *status, 0, 8, 0),
                _ => {}
            }
            self.claims(m);
            return;
        }
        if self.runs.len() >= 512 && !self.runs.contains_key(&pc) {
            limit(m, "Harici I/O site sınırı512");
            return;
        }
        if let Err(e) = self.can_claim(m, a, pc) {
            limit(m, e);
            return;
        }
        let buffered_print = matches!(a, ExternalIoAction::Print { .. })
            && self.runs.get(&pc).is_some_and(|r| r.active);
        if let Err(e) = if buffered_print {
            Ok(())
        } else {
            validate(m, a, self.long_packing)
        } {
            limit(m, e);
            return;
        }
        let mut r = self.runs.remove(&pc).unwrap_or_else(|| Run::new(a.clone()));
        if !r.active && r.done && matches!(a, ExternalIoAction::Print { .. }) {
            r = Run::new(a.clone());
        }
        if self.cadence == VisitCadence::OncePerScan && r.last_epoch == self.epoch {
            self.runs.insert(pc, r);
            return;
        }
        r.last_epoch = self.epoch;
        match a {
            ExternalIoAction::TenKey {
                wide,
                source,
                value,
                status,
            } => {
                let current = mask(m, *source, 10);
                let pressed = current & !r.previous;
                r.previous = current;
                if pressed != 0 {
                    let k = pressed.trailing_zeros() as u8;
                    append(m, value, k, *wide, false);
                    set_bits(m, *status, 0, 10, 1 << k);
                }
                set_bits(m, *status, 10, 1, u32::from(current != 0));
            }
            ExternalIoAction::HexKey {
                source,
                strobe,
                value,
                status,
                wide,
            } => {
                let row = r.phase / 2;
                if r.phase.is_multiple_of(2) {
                    physical_bits(m, *strobe, 4, 1 << row);
                } else {
                    let pins = if m.system_bit(1034)
                        || !m.physical_output(d(strobe.area, strobe.index + u16::from(row)))
                    {
                        0
                    } else if let Some(keys) = self.keyboards.get(&pc) {
                        keys.keys()
                            .filter(|&&key| key / 4 == row)
                            .fold(0u16, |v, &key| v | (1 << (key % 4)))
                    } else {
                        mask(m, *source, 4)
                    };
                    r.keys = (r.keys & !(15 << (row * 4))) | (pins << (row * 4));
                }
                r.phase = (r.phase + 1) % 8;
                if r.phase == 0 {
                    let fresh = r.keys & !r.previous;
                    if r.locked.is_some_and(|k| r.keys & (1 << k) == 0) {
                        r.locked = None;
                    }
                    if r.locked.is_none() && fresh != 0 {
                        let key = self
                            .keyboards
                            .get(&pc)
                            .and_then(|keys| {
                                keys.iter()
                                    .filter(|(k, _)| fresh & (1 << **k) != 0)
                                    .min_by_key(|(_, order)| **order)
                                    .map(|(&k, _)| k)
                            })
                            .unwrap_or(fresh.trailing_zeros() as u8);
                        r.locked = Some(key);
                        let hex = m.system_bit(1167);
                        if key < 10 || hex {
                            append(m, value, key, *wide, hex);
                        }
                        if key >= 10 {
                            set_bits(m, *status, 0, 6, 1 << (key - 10));
                        }
                        set_bits(m, *status, 6, 2, if key >= 10 { 1 } else { 2 });
                        self.emit(m);
                    }
                    r.previous = r.keys;
                }
            }
            ExternalIoAction::Dip { strobe, .. } => {
                if r.deadline.is_none() {
                    r.deadline = self.now_us.checked_add(100_000);
                    if r.deadline.is_none() {
                        limit(m, "DSW zaman çizgisi taştı");
                    }
                    physical_bits(m, *strobe, 4, 1);
                }
            }
            ExternalIoAction::Segment {
                source,
                output,
                mode,
            } => {
                let first = source.read(m);
                let second = if *mode >= 4 {
                    next(source).map(|v| v.read(m)).unwrap_or(0)
                } else {
                    0
                };
                if !(0..=9999).contains(&first) || !(0..=9999).contains(&second) {
                    limit(
                        m,
                        "SEGL sayısı0..9999 dışında; PLC hata kodu doğrulanmadığından sonuç tutuldu",
                    );
                } else {
                    self.segment_step(m, pc, &mut r, [first as u16, second as u16], *output, *mode);
                }
            }
            ExternalIoAction::Arrows {
                source,
                value,
                output,
                mode,
            } => {
                let current = mask(m, *source, 4);
                let pressed = current & !r.previous;
                r.previous = current;
                let mut number = value.read(m);
                if !(0..=9999).contains(&number) {
                    limit(m, "ARWS sayısı0..9999 dışında; sonuç tutuldu");
                } else {
                    if pressed != 0 {
                        let key = pressed.trailing_zeros();
                        match key {
                            2 => r.selected = (r.selected + 3) % 4,
                            3 => r.selected = (r.selected + 1) % 4,
                            _ => {
                                let factor = 10i32.pow(u32::from(r.selected));
                                let digit = (number / factor) % 10;
                                let new = if key == 1 {
                                    (digit + 1) % 10
                                } else {
                                    (digit + 9) % 10
                                };
                                number += (new - digit) * factor;
                                write(m, value, number);
                            }
                        }
                    }
                    self.display_commands.insert(pc, [number as u16, 0]);
                    if self.arrow_bus == ArrowBus::MultiplexDisplay {
                        self.segment_step(m, pc, &mut r, [number as u16, 0], *output, *mode);
                    } else {
                        let digit = (number / 10i32.pow(u32::from(r.selected))) % 10;
                        set_bcd(m, *output, 0, digit as u8, *mode);
                        set_latches(m, *output, Some(r.selected), *mode);
                        self.sample_latch(m, pc, *output, *mode, r.selected, 1);
                    }
                }
            }
            ExternalIoAction::Print { source, .. } => {
                if r.bytes.is_empty() && !r.done {
                    r.long = m.system_bit(1027);
                    r.bytes = print_bytes(m, *source, r.long, self.long_packing);
                    r.stop_after = false;
                }
                self.print_step(m, pc, &mut r);
            }
        }
        self.runs.insert(pc, r);
        self.claims(m);
    }
    fn off(&self, m: &mut Memory, a: &ExternalIoAction) {
        for output in a.pin_spans().1 {
            m.set_internal(output, false);
            m.set_physical_output(output, false);
        }
    }
    fn segment_step(
        &mut self,
        m: &mut Memory,
        pc: u16,
        r: &mut Run,
        values: [u16; 2],
        output: Device,
        mode: u8,
    ) {
        self.display_commands.insert(pc, values);
        let digit = r.phase / 3;
        let phase = r.phase % 3;
        if phase == 0 {
            set_latches(m, output, None, mode);
            set_bcd(
                m,
                output,
                0,
                ((values[0] / 10u16.pow(u32::from(digit))) % 10) as u8,
                mode,
            );
            if mode >= 4 {
                set_bcd(
                    m,
                    output,
                    8,
                    ((values[1] / 10u16.pow(u32::from(digit))) % 10) as u8,
                    mode,
                );
            }
        }
        if phase == 1 {
            set_latches(m, output, Some(digit), mode);
            self.sample_latch(m, pc, output, mode, digit, if mode >= 4 { 2 } else { 1 });
        }
        if phase == 2 {
            set_latches(m, output, None, mode);
        }
        r.phase = (r.phase + 1) % 12;
        if r.phase == 0 && matches!(r.action, ExternalIoAction::Segment { .. }) {
            self.emit(m);
        }
        self.notices.insert("SEGL: üç faz/hanede12ziyaret profili; fiziksel min10ms tarama şartı host gecikmesiyle taklit edilmez".into());
    }
    fn sample_latch(
        &mut self,
        m: &Memory,
        pc: u16,
        output: Device,
        mode: u8,
        digit: u8,
        groups: usize,
    ) {
        let active = mode & 1 == 0;
        if m.system_bit(1034)
            || m.physical_output(d(output.area, output.index + 4 + u16::from(digit))) != active
        {
            return;
        }
        let stored = self.display_latches.entry(pc).or_default();
        let factor = 10u16.pow(u32::from(digit));
        for (group, stored_value) in stored.iter_mut().enumerate().take(groups) {
            let mut nibble = (0..4).fold(0u16, |v, n| {
                v | (u16::from(m.physical_output(d(
                    output.area,
                    output.index + if group == 0 { 0 } else { 8 } + n,
                ))) << n)
            });
            if mode & 2 != 0 {
                nibble ^= 15;
            }
            if nibble <= 9 {
                *stored_value =
                    *stored_value - ((*stored_value / factor) % 10) * factor + nibble * factor;
            }
        }
    }
    fn print_step(&mut self, m: &mut Memory, pc: u16, r: &mut Run) {
        let ExternalIoAction::Print { output, .. } = r.action else {
            return;
        };
        if r.done {
            return;
        }
        if r.sent >= r.bytes.len() {
            self.finish_print(m, pc, r);
            return;
        }
        physical_bits(m, d(output.area, output.index + 9), 1, 1);
        match r.phase {
            0 => {
                physical_bits(m, output, 8, u16::from(r.bytes[r.sent]));
                physical_bits(m, d(output.area, output.index + 8), 1, 0);
            }
            1 => {
                physical_bits(m, d(output.area, output.index + 8), 1, 1);
                if !m.system_bit(1034) && m.physical_output(d(output.area, output.index + 8)) {
                    let byte = (0..8).fold(0u8, |v, n| {
                        v | (u8::from(m.physical_output(d(output.area, output.index + n))) << n)
                    });
                    r.observed_bytes.push(byte);
                }
            }
            _ => {
                physical_bits(m, d(output.area, output.index + 8), 1, 0);
                r.sent += 1;
            }
        }
        r.phase = (r.phase + 1) % 3;
        if r.sent == r.bytes.len() && r.phase == 0 {
            self.finish_print(m, pc, r);
        }
    }
    fn finish_print(&mut self, m: &mut Memory, pc: u16, r: &mut Run) {
        for (map, frame) in [
            (&mut self.printer_frames, r.observed_bytes.clone()),
            (&mut self.printer_commands, r.bytes.clone()),
        ] {
            let frames = map.entry(pc).or_default();
            if frames.len() == 256 {
                frames.pop_front();
            }
            frames.push_back(frame);
        }
        let ExternalIoAction::Print { output, .. } = r.action else {
            return;
        };
        if r.long && !r.stop_after && self.long_held == LongHeld::RepeatMessage {
            r.sent = 0;
            r.phase = 0;
            r.observed_bytes.clear();
            return;
        }
        physical_bits(
            m,
            d(output.area, output.index + 8),
            2,
            if r.long && !r.stop_after { 2 } else { 0 },
        );
        r.done = true;
        if !r.long || r.stop_after {
            self.emit(m);
        }
        if r.stop_after {
            r.active = false;
        }
    }
    pub fn advance(m: &mut Memory, at_us: u64, raw: &BTreeMap<Device, bool>) {
        let mut s = std::mem::take(&mut m.external_io);
        if at_us < s.now_us {
            limit(m, "Harici I/O sanal zamanı geriye gidiyor");
            m.external_io = s;
            return;
        }
        s.now_us = at_us;
        s.raw = raw.clone();
        while let Some((&key, _)) = s.events.first_key_value() {
            if key.0 > at_us {
                break;
            }
            let e = s.events.remove(&key).unwrap();
            let mut keys: Vec<_> = s
                .keyboards
                .get(&e.pc)
                .map(|v| v.keys().copied().collect())
                .unwrap_or_default();
            if e.down && !keys.contains(&e.key) {
                keys.push(e.key);
            }
            if !e.down {
                keys.retain(|k| *k != e.key);
            }
            if let Err(e) = s.set_keyboard(e.pc, &keys) {
                limit(m, e);
            }
        }
        let ids: Vec<_> = s.runs.keys().copied().collect();
        for pc in ids {
            let mut r = s.runs.remove(&pc).unwrap();
            let mut iterations = 0;
            while r.active && r.deadline.is_some_and(|t| t <= at_us) {
                iterations += 1;
                if iterations > 4096 {
                    limit(
                        m,
                        "DSW tek ilerlemede4096strobe sınırı; daha küçük zaman adımları gerekli",
                    );
                    r.deadline = at_us.checked_add(100_000);
                    break;
                }
                let ExternalIoAction::Dip {
                    source,
                    strobe,
                    value,
                    groups,
                } = r.action
                else {
                    break;
                };
                let digit = r.phase;
                for g in 0..groups {
                    let enabled = !m.system_bit(1034)
                        && m.physical_output(d(strobe.area, strobe.index + u16::from(digit)));
                    let nibble = if let Some(v) = s.dips.get(&pc) {
                        if !enabled {
                            r.digits[g as usize][digit as usize] = 255;
                            continue;
                        }
                        (v[g as usize] / 10u16.pow(u32::from(digit))) % 10
                    } else {
                        (0..4).fold(0u16, |v, n| {
                            v | (u16::from(
                                raw.get(&d(source.area, source.index + u16::from(g) * 4 + n))
                                    .copied()
                                    .unwrap_or(false),
                            ) << n)
                        })
                    };
                    if nibble > 9 {
                        limit(m, "DSW BCD giriş hanesi9'dan büyük; grup sonucu tutuldu");
                        r.digits[g as usize][digit as usize] = 255;
                    } else {
                        r.digits[g as usize][digit as usize] = nibble as u8;
                    }
                }
                r.phase = (r.phase + 1) % 4;
                physical_bits(m, strobe, 4, 1 << r.phase);
                if r.phase == 0 {
                    for g in 0..groups {
                        let digits = r.digits[g as usize];
                        if digits.iter().all(|v| *v < 10) {
                            let number = digits
                                .iter()
                                .enumerate()
                                .map(|(i, v)| u16::from(*v) * 10u16.pow(i as u32))
                                .sum::<u16>();
                            m.set_word(d(value.area, value.index + u16::from(g)), number as i16);
                        }
                    }
                    s.emit(m);
                }
                r.deadline = r.deadline.and_then(|t| t.checked_add(100_000));
                if r.deadline.is_none() {
                    limit(m, "DSW sanal son tarih taşması");
                    break;
                }
            }
            s.runs.insert(pc, r);
        }
        m.external_io = s;
    }
    pub fn input_edge(m: &mut Memory, input: Device, _old: bool, new: bool, at_us: u64) {
        if at_us < m.external_io.now_us {
            limit(m, "Harici I/O fiziksel olay zamanı geriye gidiyor");
            return;
        }
        if input.area == Area::X {
            m.external_io.raw.insert(input, new);
        }
    }
}
fn is_ours(owner: &ResourceOwner) -> bool {
    matches!(owner,ResourceOwner::External(s) if s.starts_with("EXTIO:"))
}
fn mask(m: &Memory, base: Device, n: u16) -> u16 {
    (0..n).fold(0, |v, i| {
        v | (u16::from(m.bit(d(base.area, base.index + i))) << i)
    })
}
fn set_bits(m: &mut Memory, base: Device, start: u16, n: u16, value: u32) {
    for i in 0..n {
        let target = d(base.area, base.index + start + i);
        let on = value & (1 << i) != 0;
        m.set_internal(target, on);
        if target.area == Area::Y {
            m.set_physical_output(target, on);
        }
    }
}
fn physical_bits(m: &mut Memory, base: Device, n: u16, value: u16) {
    for i in 0..n {
        let bit = d(base.area, base.index + i);
        let on = value & (1 << i) != 0;
        m.set_internal(bit, on);
        m.set_physical_output(bit, on);
    }
}
fn append(m: &mut Memory, target: &CalendarValue, key: u8, wide: bool, hex: bool) {
    let old = target.read(m) as u32;
    let value = if hex {
        if wide {
            old.wrapping_mul(16).wrapping_add(u32::from(key))
        } else {
            (old.wrapping_mul(16) + u32::from(key)) & 65535
        }
    } else {
        ((u64::from(old) * 10 + u64::from(key)) % if wide { 100_000_000 } else { 10_000 }) as u32
    };
    write(m, target, value as i32);
}
fn set_bcd(m: &mut Memory, base: Device, offset: u16, value: u8, mode: u8) {
    physical_bits(
        m,
        d(base.area, base.index + offset),
        4,
        u16::from(value) ^ if mode & 2 != 0 { 15 } else { 0 },
    );
}
fn set_latches(m: &mut Memory, base: Device, selected: Option<u8>, mode: u8) {
    let mut v = selected.map(|n| 1u16 << n).unwrap_or(0);
    if mode & 1 != 0 {
        v ^= 15;
    }
    physical_bits(m, d(base.area, base.index + 4), 4, v);
}
fn print_bytes(m: &Memory, source: Device, long: bool, packing: LongPacking) -> Vec<u8> {
    let words = if long {
        if packing == LongPacking::LowByteWords {
            16
        } else {
            8
        }
    } else {
        4
    };
    let mut bytes = Vec::new();
    for i in 0..words {
        let value = m.word(d(source.area, source.index + i)) as u16;
        bytes.push(value as u8);
        if !(long && packing == LongPacking::LowByteWords) {
            bytes.push((value >> 8) as u8);
        }
    }
    if long {
        bytes.truncate(bytes.iter().position(|v| *v == 0).unwrap_or(16));
    }
    bytes
}
fn validate(m: &Memory, a: &ExternalIoAction, packing: LongPacking) -> Result<(), String> {
    match a {
        ExternalIoAction::TenKey { value, status, .. }
        | ExternalIoAction::HexKey { value, status, .. } => {
            let n = if matches!(a, ExternalIoAction::TenKey { .. }) {
                11
            } else {
                8
            };
            if span(*status, n).iter().any(|v| !fits(m, *v)) {
                return Err("Tuş durumunun tam bit aralığı model belleğini aşıyor".into());
            }
            check(m, value)
        }
        ExternalIoAction::Arrows { value, .. } => check(m, value),
        ExternalIoAction::Dip { value, groups, .. } => {
            if span(*value, u16::from(*groups))
                .iter()
                .any(|v| !fits(m, *v) || v.area == Area::C && v.index >= 200)
            {
                Err("DSW sonuç aralığı model/doğal C kelime sınırını aşıyor".into())
            } else {
                Ok(())
            }
        }
        ExternalIoAction::Segment { source, mode, .. } => {
            check(m, source)?;
            if *mode >= 4 {
                check(m, &next(source)?)?;
            }
            Ok(())
        }
        ExternalIoAction::Print { source, .. } => {
            let n = if m.system_bit(1027) {
                if packing == LongPacking::LowByteWords {
                    16
                } else {
                    8
                }
            } else {
                4
            };
            if span(*source, n)
                .iter()
                .any(|v| !fits(m, *v) || v.area == Area::C && v.index >= 200)
            {
                Err("PR kaynak dizisi model/doğal C kelime sınırını aşıyor".into())
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "external_io_tests.rs"]
mod tests;
