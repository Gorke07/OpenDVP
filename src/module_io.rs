//! Explicit typed virtual module bus; installed simulator handlers are stubs.
use crate::calendar_operand::CalendarValue;
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleIoAction {
    pub read: bool,
    pub wide: bool,
    pub module: CalendarValue,
    pub register: CalendarValue,
    pub value: CalendarValue,
    pub count: CalendarValue,
}
impl ModuleIoAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if args.len() != 4 || !matches!(name, "FROM" | "DFROM" | "TO" | "DTO") {
            return Err(
                "FROM/DFROM/TO/DTO dört operand gerektirir; P dış sarmalayıcı kullanır".into(),
            );
        }
        let wide = name.starts_with('D');
        let read = matches!(name, "FROM" | "DFROM");
        let parse = |raw: &str, dest: bool| -> Result<CalendarValue, String> {
            if !(raw.starts_with('D') || !dest && raw.starts_with(['K', 'H'])) {
                return Err("FROM/TO matrisi yalnız K/H/D; FROM hedefi yalnız D kabul eder".into());
            }
            let v = CalendarValue::parse(raw, wide)?;
            if v.base().is_some_and(|v| v.area != Area::D) {
                return Err("FROM/TO register operandı çıplak D olmalı".into());
            }
            Ok(v)
        };
        Ok(Self {
            read,
            wide,
            module: parse(&args[0], false)?,
            register: parse(&args[1], false)?,
            value: parse(&args[2], read)?,
            count: parse(&args[3], false)?,
        })
    }
    pub fn name(&self) -> &'static str {
        match (self.read, self.wide) {
            (true, false) => "FROM",
            (true, true) => "DFROM",
            (false, false) => "TO",
            (false, true) => "DTO",
        }
    }
    pub fn header(&self) -> u16 {
        match (self.read, self.wide) {
            (true, false) => 0xAC00,
            (true, true) => 0xAD00,
            (false, false) => 0xAE00,
            (false, true) => 0xAF00,
        }
    }
    pub fn words(&self) -> u16 {
        if self.wide { 17 } else { 9 }
    }
    pub fn wide(&self) -> bool {
        self.wide
    }
    pub fn operands(&self) -> Vec<CalendarValue> {
        vec![
            self.module.clone(),
            self.register.clone(),
            self.value.clone(),
            self.count.clone(),
        ]
    }
    pub fn dest(&self) -> Device {
        self.value.base().unwrap_or(d(0))
    }
    pub fn label(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.name(),
            self.module,
            self.register,
            self.value,
            self.count
        )
    }
    pub fn display_value(&self, m: &Memory) -> String {
        let slot = self.module.read(m);
        let cr = self.register.read(m);
        if let (Ok(slot), Ok(cr)) = (u8::try_from(slot), u16::try_from(cr)) {
            match m.module_io.read_cr(slot, cr) {
                Ok(v) => format!("CR#{cr}: {v} (H{v:04X})"),
                Err(e) => e,
            }
        } else {
            "Modül/CR adresi geçersiz".into()
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for v in self.operands() {
            v.devices(out);
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        let n = self.count.read(m);
        if (1..=256).contains(&n)
            && let Some(base) = self.value.base()
        {
            for i in 0..n as u16 * if self.wide { 2 } else { 1 } {
                if let Some(index) = base.index.checked_add(i)
                    && index <= m.data_register_limit.unwrap_or(9999)
                {
                    out.insert(d(index));
                }
            }
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        if !power {
            return;
        }
        let at = m.timing_io.now_us;
        let mut bus = std::mem::take(&mut m.module_io);
        if let Err(e) = bus.transfer(m, self, pc, at) {
            m.execution_limitations
                .insert(format!("{} STEP{pc}: {e}", self.name()));
        }
        m.module_io = bus;
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCpu {
    Es2,
    Ss2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCpuProfile {
    pub model: ModuleCpu,
    pub major: u16,
    pub minor: u8,
}
impl Default for ModuleCpuProfile {
    fn default() -> Self {
        Self {
            model: ModuleCpu::Es2,
            major: 3,
            minor: 46,
        }
    }
}
impl ModuleCpuProfile {
    pub fn new(model: ModuleCpu, major: u16, minor: u8) -> Result<Self, String> {
        if minor > 99 {
            return Err("CPU firmware alt sürümü 0..99 olmalı".into());
        }
        Ok(Self {
            model,
            major,
            minor,
        })
    }
    fn modern(self) -> bool {
        match self.model {
            ModuleCpu::Es2 => (self.major, self.minor) >= (3, 0),
            ModuleCpu::Ss2 => (self.major, self.minor) >= (2, 80),
        }
    }
    fn max_words(self) -> u16 {
        if self.modern() { 6 } else { 4 }
    }
    fn max_cr(self) -> u16 {
        if self.model == ModuleCpu::Es2 {
            255
        } else {
            48
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleKind {
    Dvp04AdE2,
    Dvp04AdS2,
}
impl ModuleKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Dvp04AdE2 => "DVP04AD-E2",
            Self::Dvp04AdS2 => "DVP04AD-S2",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleFrame {
    pub present: [i16; 4],
    pub average: [i16; 4],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum S2ResetPolicy {
    #[default]
    LiteralAllSettings,
    ChannelOnlyCandidate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum E2LockPolicy {
    #[default]
    HoldUndocumentedMask,
    ChannelBitsCandidate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum S2ChannelLayout {
    #[default]
    KnownCh1Only,
    TripletCandidate,
}
#[derive(Debug, Clone)]
pub struct VirtualModule {
    kind: ModuleKind,
    firmware: u16,
    registers: BTreeMap<u16, u16>,
    retained: BTreeMap<u16, u16>,
    frame: Option<SampleFrame>,
    sample_valid: [bool; 4],
    history: [VecDeque<i16>; 4],
    reset_at_us: Option<u64>,
    /// Only the currently attempted write may contribute configuration faults
    /// when an entire candidate transaction is rolled back.
    current_write_faults: u16,
    pub notices: BTreeSet<String>,
    pub s2_reset_policy: S2ResetPolicy,
    pub e2_lock_policy: E2LockPolicy,
    pub s2_channel_layout: S2ChannelLayout,
}
impl VirtualModule {
    pub fn ad_e2(firmware_hex: u16) -> Result<Self, String> {
        Self::new(ModuleKind::Dvp04AdE2, firmware_hex)
    }
    pub fn ad_s2(firmware_hex: u16) -> Result<Self, String> {
        Self::new(ModuleKind::Dvp04AdS2, firmware_hex)
    }
    fn new(kind: ModuleKind, firmware: u16) -> Result<Self, String> {
        if firmware == 0 {
            return Err("Modül firmware kimliği açık ve sıfırdan farklı olmalı".into());
        }
        let registers = defaults(kind, firmware);
        let retained = retain_values(kind, &registers);
        Ok(Self {
            kind,
            firmware,
            registers,
            retained,
            frame: None,
            sample_valid: [false; 4],
            history: std::array::from_fn(|_| VecDeque::new()),
            reset_at_us: None,
            current_write_faults: 0,
            notices: BTreeSet::new(),
            s2_reset_policy: S2ResetPolicy::default(),
            e2_lock_policy: E2LockPolicy::default(),
            s2_channel_layout: S2ChannelLayout::default(),
        })
    }
    pub fn kind(&self) -> ModuleKind {
        self.kind
    }
    pub fn firmware(&self) -> u16 {
        self.firmware
    }
    pub fn samples(&self) -> Option<[i16; 4]> {
        self.frame.map(|v| v.present)
    }
    pub fn sample_frame(&self) -> Option<SampleFrame> {
        self.frame
    }
    pub fn registers(&self) -> BTreeMap<u16, Result<u16, String>> {
        self.registers
            .keys()
            .map(|&cr| (cr, self.read_cr(cr)))
            .collect()
    }
    fn mode(&self, ch: usize) -> u16 {
        if self.kind == ModuleKind::Dvp04AdE2 {
            self.registers[&(2 + ch as u16)]
        } else {
            (self.registers[&1] >> (ch * 3)) & 7
        }
    }
    fn disabled(&self, ch: usize) -> bool {
        self.mode(ch)
            == if self.kind == ModuleKind::Dvp04AdE2 {
                u16::MAX
            } else {
                7
            }
    }
    fn sample_value(&self, ch: usize, average: bool) -> Result<u16, String> {
        if self.disabled(ch) {
            return Ok(if self.kind == ModuleKind::Dvp04AdE2 {
                32767
            } else {
                0
            });
        }
        if !self.sample_valid[ch] {
            return Err(
                "Kanal için geçerli açık dönüşüm-sonrası örnek/ortalama yanıtı henüz yok".into(),
            );
        }
        let frame = self.frame.ok_or("Analog örnek çerçevesi eksik")?;
        Ok(if average {
            frame.average[ch]
        } else {
            frame.present[ch]
        } as u16)
    }
    pub fn read_cr(&self, cr: u16) -> Result<u16, String> {
        if !self.registers.contains_key(&cr) {
            return Err(format!(
                "{} CR#{cr} ayrılmış veya bu profilde belgelenmedi",
                self.kind.name()
            ));
        }
        match (self.kind, cr) {
            (ModuleKind::Dvp04AdE2, 12..=15) => self.sample_value((cr - 12) as usize, true),
            (ModuleKind::Dvp04AdE2, 20..=23) => self.sample_value((cr - 20) as usize, false),
            (ModuleKind::Dvp04AdS2, 6..=9) => self.sample_value((cr - 6) as usize, true),
            (ModuleKind::Dvp04AdS2, 12..=15) => self.sample_value((cr - 12) as usize, false),
            _ => Ok(self.registers[&cr]),
        }
    }
    pub fn set_sample_frame(&mut self, frame: SampleFrame) {
        self.frame = Some(frame);
        self.sample_valid = [true; 4];
        self.notices.insert("Açık dönüşüm-sonrası örnek+ortalama yanıtı; fiziksel ADC/dönüşüm saati eşdeğerliği değildir".into());
        self.update_limits();
    }
    /// Caller asserts a completed stable-code window, with equal present/mean.
    pub fn set_samples(&mut self, present: [i16; 4]) {
        self.set_sample_frame(SampleFrame {
            present,
            average: present,
        });
    }
    pub fn push_conversion_event(&mut self, present: [i16; 4]) -> Result<(), String> {
        let mut average = [0i16; 4];
        let mut complete = true;
        for (ch, &v) in present.iter().enumerate() {
            let first = if self.kind == ModuleKind::Dvp04AdE2 {
                8
            } else {
                2
            };
            let n = usize::from(self.registers[&(first + ch as u16)]);
            let h = &mut self.history[ch];
            h.push_back(v);
            while h.len() > n {
                h.pop_front();
            }
            if h.len() < n {
                complete = false;
            }
            average[ch] = (h.iter().map(|v| i64::from(*v)).sum::<i64>() / h.len() as i64) as i16;
        }
        if !complete {
            return Err(
                "Ortalama için tam açık örnek penceresi dolmadı; son geçerli yanıt korunuyor"
                    .into(),
            );
        }
        self.set_sample_frame(SampleFrame { present, average });
        self.notices.insert("Açık örnek penceresinde sıfıra doğru tamsayı ortalaması; firmware yuvarlaması kanıtlanmadı".into());
        Ok(())
    }
    fn update_limits(&mut self) {
        if self.kind != ModuleKind::Dvp04AdE2 {
            return;
        }
        let Some(frame) = self.frame else {
            return;
        };
        let mut bits = 0;
        for (ch, &v) in frame.present.iter().enumerate() {
            if self.registers[&100] & (1 << ch) == 0 || self.disabled(ch) || !self.sample_valid[ch]
            {
                continue;
            }
            if v < (self.registers[&(108 + ch as u16)] as i16) {
                bits |= 1 << ch;
            }
            if v > (self.registers[&(102 + ch as u16)] as i16) {
                bits |= 1 << (8 + ch);
            }
        }
        self.registers.insert(101, bits);
        let error = self.registers[&43];
        self.registers
            .insert(43, (error & !4) | if bits != 0 { 4 } else { 0 });
    }
    fn fault(&mut self, bit: u16) {
        self.current_write_faults |= bit;
        let cr = if self.kind == ModuleKind::Dvp04AdE2 {
            43
        } else {
            30
        };
        *self.registers.entry(cr).or_default() |= bit;
    }
    fn write_one(&mut self, cr: u16, value: u16, at: u64) -> Result<(), String> {
        if cr == 0 && value == 0x4352 {
            let min = if self.kind == ModuleKind::Dvp04AdE2 {
                0x0110
            } else {
                0x0416
            };
            if self.firmware < min {
                return Err("CR0 reset bu firmware için belgelenmedi".into());
            }
            self.reset_at_us = Some(at);
            return Ok(());
        }
        if !writable(self.kind, cr) {
            return Err(format!(
                "{} CR#{cr} salt okunur veya ayrılmış",
                self.kind.name()
            ));
        }
        match self.kind {
            ModuleKind::Dvp04AdE2 => self.write_e2(cr, value)?,
            ModuleKind::Dvp04AdS2 => self.write_s2(cr, value)?,
        }
        self.update_limits();
        Ok(())
    }
    fn write_e2(&mut self, cr: u16, value: u16) -> Result<(), String> {
        if !matches!(cr, 40 | 41 | 101) && self.registers[&40] != 0 {
            if self.e2_lock_policy == E2LockPolicy::HoldUndocumentedMask {
                return Err("E2 CR40 kilit maskesi tam açıklanmadı; değişiklik tutuldu".into());
            }
            let ch = match cr {
                2..=5 => Some(cr - 2),
                8..=11 => Some(cr - 8),
                28..=31 => Some(cr - 28),
                34..=37 => Some(cr - 34),
                102..=105 => Some(cr - 102),
                108..=111 => Some(cr - 108),
                _ => None,
            };
            if ch.is_some_and(|ch| self.registers[&40] & (1 << ch) != 0) {
                self.fault(0x1000);
                return Err("E2 kanal ayarı kilitli".into());
            }
        }
        match cr {
            2..=5 => {
                if !(value <= 6 || value == u16::MAX) {
                    self.fault(0x0200);
                    return Err("E2 giriş modu0..6 veya HFFFF olmalı".into());
                }
                if self.registers[&cr] != value {
                    let ch = cr - 2;
                    self.registers.insert(28 + ch, 0);
                    self.registers.insert(34 + ch, 16000);
                    self.history[ch as usize].clear();
                    self.sample_valid[ch as usize] = false;
                }
            }
            8..=11 => {
                if !(1..=100).contains(&value) {
                    self.fault(0x0400);
                    return Err("E2 ortalama sayısı1..100 olmalı".into());
                }
            }
            28..=31 | 34..=37 => {
                let ch = if cr < 34 { cr - 28 } else { cr - 34 };
                if self.mode(ch as usize) == 6 {
                    return Err("E2 mod6 offset/gain ayarını yasaklar".into());
                }
            }
            41 => {
                if value == 0x5678 {
                    self.retained = retain_values(self.kind, &self.registers);
                }
                self.registers.insert(cr, value);
                return Ok(());
            }
            100 => {
                if value & !15 != 0 {
                    return Err("E2 limit etkinleştirme kanal0..3 bitleriyle sınırlı".into());
                }
            }
            102..=105 => {
                if (value as i16) < (self.registers[&(cr + 6)] as i16) {
                    self.fault(0x0800);
                    return Err("E2 üst sınır alt sınırdan küçük".into());
                }
            }
            108..=111 if (value as i16) > (self.registers[&(cr - 6)] as i16) => {
                self.fault(0x0800);
                return Err("E2 alt sınır üst sınırdan büyük".into());
            }
            _ => {}
        }
        self.registers.insert(cr, value);
        Ok(())
    }
    fn reset_factory(&mut self) {
        self.registers = defaults(self.kind, self.firmware);
        self.retained = retain_values(self.kind, &self.registers);
        self.frame = None;
        self.sample_valid = [false; 4];
        for h in &mut self.history {
            h.clear();
        }
    }
    fn write_s2(&mut self, cr: u16, value: u16) -> Result<(), String> {
        match cr {
            1 => {
                if value & 0xf000 != 0 {
                    return Err("S2 giriş modu üst dört bit ayrılmış".into());
                }
                if self.firmware < 0x0500
                    && (0..4).any(|ch| matches!((value >> (3 * ch)) & 7, 4..=6))
                {
                    self.fault(4);
                    return Err("S2 mod4..6 firmware V5.00 gerektirir".into());
                }
            }
            2..=5 => {
                if !(1..=20).contains(&value) {
                    self.fault(64);
                    return Err("S2 ortalama sayısı1..20 olmalı".into());
                }
            }
            18..=21 | 24..=27 => {
                let ch = if cr < 24 { cr - 18 } else { cr - 24 };
                if self.registers[&33] & (1 << (ch * 3)) != 0 {
                    return Err("S2 offset/gain CR33 tarafından kilitli".into());
                }
                let voltage = matches!(self.mode(ch as usize), 0 | 1 | 5 | 6);
                let mut offset = self.registers[&(18 + ch)] as i16;
                let mut gain = self.registers[&(24 + ch)] as i16;
                if cr < 24 {
                    offset = value as i16;
                } else {
                    gain = value as i16;
                }
                let maxgain = if voltage { 16000 } else { 10400 };
                let maxgap = if voltage { 12000 } else { 6400 };
                let gap = i32::from(gain) - i32::from(offset);
                if !(-4000..=4000).contains(&offset)
                    || !(-3200..=maxgain).contains(&gain)
                    || !(800..=maxgap).contains(&gap)
                {
                    self.fault(8);
                    return Err("S2 offset/gain/fark sınırı geçersiz".into());
                }
            }
            31 => {
                if !(1..=254).contains(&value) {
                    return Err("S2 RS485 istasyonu1..254 olmalı".into());
                }
            }
            32 => validate_format(value, self.firmware)?,
            33 => {
                if value & 0xf000 != 0 {
                    return Err("S2 CR33 üst dört bit ayrılmış".into());
                }
                if self.s2_channel_layout == S2ChannelLayout::KnownCh1Only && value & !7 != 0 {
                    return Err(
                        "S2 CR33 diğer kanal tripletleri için açık aday profil gerekli".into(),
                    );
                }
                let reset = value & 0x0924;
                if reset != 0 {
                    match self.s2_reset_policy {
                        S2ResetPolicy::LiteralAllSettings => {
                            self.reset_factory();
                            self.notices.insert("S2 CR33 literal tüm-ayarlar reset profili; kanal kapsamı firmware kanıtına açık".into());
                            return Ok(());
                        }
                        S2ResetPolicy::ChannelOnlyCandidate => {
                            for ch in 0..4 {
                                if reset & (4 << (ch * 3)) != 0 {
                                    self.registers.insert(18 + ch, 0);
                                    self.registers.insert(24 + ch, 4000);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        self.registers.insert(cr, value);
        if retentive(self.kind, cr) {
            let ch = match cr {
                18..=21 => Some(cr - 18),
                24..=27 => Some(cr - 24),
                _ => None,
            };
            if ch.is_none_or(|ch| self.registers[&33] & (2 << (ch * 3)) == 0) {
                self.retained.insert(cr, value);
            }
        }
        Ok(())
    }
    fn write_batch(&mut self, start: u16, values: &[u16], at: u64) -> Result<(), String> {
        let mut next = self.clone();
        for (i, &v) in values.iter().enumerate() {
            next.current_write_faults = 0;
            if let Err(e) = next.write_one(start + i as u16, v, at) {
                // A prior candidate limit/config/reset operation may have
                // changed or cleared next's status. It is not committed.
                // Preserve original status and add only this failed write's
                // explicitly identified configuration fault bits.
                self.fault(next.current_write_faults);
                self.current_write_faults = 0;
                return Err(e);
            }
        }
        next.current_write_faults = 0;
        *self = next;
        Ok(())
    }
    pub fn power_cycle(&mut self, at: u64) -> Result<(), String> {
        if self.reset_at_us.is_some_and(|v| at < v) {
            return Err("Güç çevrimi reset isteğinden önce olamaz".into());
        }
        let reset = self
            .reset_at_us
            .is_some_and(|v| self.kind == ModuleKind::Dvp04AdS2 || at - v >= 1_000_000);
        if reset {
            self.reset_factory();
        } else {
            let mut next = defaults(self.kind, self.firmware);
            for (&cr, &v) in &self.retained {
                next.insert(cr, v);
            }
            self.registers = next;
            self.frame = None;
            self.sample_valid = [false; 4];
            for h in &mut self.history {
                h.clear();
            }
        }
        self.reset_at_us = None;
        Ok(())
    }
}
fn defaults(kind: ModuleKind, fw: u16) -> BTreeMap<u16, u16> {
    let mut r = BTreeMap::new();
    match kind {
        ModuleKind::Dvp04AdE2 => {
            r.extend([
                (0, 0x80),
                (1, fw),
                (40, 0),
                (41, 0),
                (43, 0),
                (100, 0),
                (101, 0),
            ]);
            for ch in 0..4 {
                for (cr, v) in [
                    (2, 0),
                    (8, 10),
                    (12, 0),
                    (20, 0),
                    (28, 0),
                    (34, 16000),
                    (102, 32000),
                    (108, (-32000i16) as u16),
                ] {
                    r.insert(cr + ch, v);
                }
            }
        }
        ModuleKind::Dvp04AdS2 => {
            r.extend([
                (0, 0x90),
                (1, 0),
                (30, 0),
                (31, 1),
                (32, 2),
                (33, 0),
                (34, fw),
            ]);
            for ch in 0..4 {
                for (cr, v) in [(2, 10), (6, 0), (12, 0), (18, 0), (24, 4000)] {
                    r.insert(cr + ch, v);
                }
            }
        }
    }
    r
}
fn retentive(kind: ModuleKind, cr: u16) -> bool {
    match kind {
        ModuleKind::Dvp04AdE2 => {
            matches!(cr,0..=5|8..=11|28..=31|34..=37|40|100|102..=105|108..=111)
        }
        ModuleKind::Dvp04AdS2 => matches!(cr,0..=5|18..=21|24..=27|31..=34),
    }
}
fn retain_values(kind: ModuleKind, r: &BTreeMap<u16, u16>) -> BTreeMap<u16, u16> {
    r.iter()
        .filter(|(k, _)| retentive(kind, **k))
        .map(|(&k, &v)| (k, v))
        .collect()
}
fn writable(kind: ModuleKind, cr: u16) -> bool {
    match kind {
        ModuleKind::Dvp04AdE2 => {
            matches!(cr,2..=5|8..=11|28..=31|34..=37|40|41|100..=105|108..=111)
        }
        ModuleKind::Dvp04AdS2 => matches!(cr,1..=5|18..=21|24..=27|31..=33),
    }
}
fn validate_format(value: u16, fw: u16) -> Result<(), String> {
    let proto = value >> 12;
    let format = (value >> 8) & 15;
    let baud = value & 255;
    if !matches!(proto, 0 | 8 | 12) || !matches!(baud, 1 | 2 | 4 | 8 | 16 | 32) {
        return Err("S2 protokol/baud alanı geçersiz".into());
    }
    if fw < 0x0415 && format != 0 {
        return Err("S2 veri biçimi alanı firmware V4.15 gerektirir".into());
    }
    let valid = if proto == 0 {
        matches!(format, 0 | 1 | 3..=11)
    } else {
        matches!(format, 0 | 1 | 3 | 5 | 7 | 9 | 11)
    };
    if !valid {
        return Err("S2 veri biçimi seçilen ASCII/RTU modunda geçersiz".into());
    }
    Ok(())
}
#[derive(Debug, Clone, Default)]
pub struct VirtualModuleBus {
    cpu: ModuleCpuProfile,
    modules: BTreeMap<u8, VirtualModule>,
    pub notices: BTreeSet<String>,
    pub last_transfer: Option<ModuleTransfer>,
    cpu_clock_offset_us: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleTransfer {
    pub pc: u16,
    pub slot: u8,
    pub first_cr: u16,
    pub words: u16,
    pub read: bool,
    pub at_us: u64,
}
impl VirtualModuleBus {
    /// Called once before the CPU clock is reset to zero. External modules
    /// retain their own elapsed time and pending reset qualification.
    pub fn restart_cpu_clock(&mut self, elapsed_cpu_us: u64) -> Result<(), String> {
        self.cpu_clock_offset_us = self.module_time_us(elapsed_cpu_us)?;
        Ok(())
    }
    pub fn module_time_us(&self, cpu_us: u64) -> Result<u64, String> {
        self.cpu_clock_offset_us
            .checked_add(cpu_us)
            .ok_or_else(|| "Harici modül sanal saati taştı".into())
    }
    pub fn reset_instruction_state(&mut self) {
        self.last_transfer = None;
    }
    pub fn cpu(&self) -> &ModuleCpuProfile {
        &self.cpu
    }
    pub fn set_cpu(&mut self, cpu: ModuleCpuProfile) -> Result<(), String> {
        if self
            .modules
            .values()
            .any(|m| !compatible(cpu.model, m.kind))
        {
            return Err("Yapılandırılmış modül CPU sağ veri yoluyla uyumsuz".into());
        }
        self.cpu = cpu;
        Ok(())
    }
    pub fn modules(&self) -> impl Iterator<Item = (&u8, &VirtualModule)> {
        self.modules.iter()
    }
    pub fn read_slot(&self, slot: u8) -> Option<&VirtualModule> {
        self.modules.get(&slot)
    }
    pub fn configure(&mut self, slot: u8, module: VirtualModule) -> Result<(), String> {
        if slot > 7 {
            return Err("Sağ modül yuvası0..7 olmalı".into());
        }
        if self.modules.contains_key(&slot) {
            return Err("Dolu modül önce açıkça kaldırılmalı".into());
        }
        if !compatible(self.cpu.model, module.kind) {
            return Err("E2 sağ modül ES2, S2 sağ modül SS2 gerektirir".into());
        }
        self.modules.insert(slot, module);
        Ok(())
    }
    pub fn remove(&mut self, slot: u8) -> Option<VirtualModule> {
        self.modules.remove(&slot)
    }
    pub fn set_samples(&mut self, slot: u8, values: [i16; 4]) -> Result<(), String> {
        self.modules
            .get_mut(&slot)
            .ok_or("Örnek için modül yapılandırılmadı")?
            .set_samples(values);
        Ok(())
    }
    pub fn set_sample_frame(&mut self, slot: u8, frame: SampleFrame) -> Result<(), String> {
        self.modules
            .get_mut(&slot)
            .ok_or("Örnek için modül yapılandırılmadı")?
            .set_sample_frame(frame);
        Ok(())
    }
    pub fn read_cr(&self, slot: u8, cr: u16) -> Result<u16, String> {
        self.modules
            .get(&slot)
            .ok_or("Modül bağlı değil; başarılı okuma üretilemez")?
            .read_cr(cr)
    }
    pub fn power_cycle(&mut self, slot: u8, at: u64) -> Result<(), String> {
        let at = self.module_time_us(at)?;
        self.modules
            .get_mut(&slot)
            .ok_or("Güç çevrimi için modül yapılandırılmadı")?
            .power_cycle(at)
    }
    fn transfer(
        &mut self,
        m: &mut Memory,
        a: &ModuleIoAction,
        pc: u16,
        at: u64,
    ) -> Result<(), String> {
        let at = self.module_time_us(at)?;
        let control = |v: &CalendarValue| -> Result<i32, String> {
            let mut devices = BTreeSet::new();
            v.devices(&mut devices);
            if devices
                .iter()
                .any(|v| v.area != Area::D || v.index > m.data_register_limit.unwrap_or(9999))
            {
                return Err(
                    "FROM/TO kontrol operandının tam D aralığı model sınırını aşıyor".into(),
                );
            }
            Ok(v.read(m))
        };
        let slot = control(&a.module)?;
        let cr = control(&a.register)?;
        let n = control(&a.count)?;
        if !(0..=7).contains(&slot) {
            return Err(
                "Sol modül/ağ portu bu ES2/SS2 sağ modül profilinde yapılandırılmadı".into(),
            );
        }
        let words = n
            .checked_mul(if a.wide { 2 } else { 1 })
            .filter(|_| n > 0)
            .ok_or("FROM/TO veri sayısı pozitif ve sınırlı olmalı")?;
        if words > i32::from(self.cpu.max_words())
            || cr < 0
            || cr
                .checked_add(words - 1)
                .is_none_or(|end| end > i32::from(self.cpu.max_cr()))
        {
            return Err("FROM/TO CR/veri sayısı CPU firmware sınırını aşıyor".into());
        }
        let slot = slot as u8;
        let cr = cr as u16;
        let words = words as u16;
        let module = self
            .modules
            .get(&slot)
            .ok_or("Modül yapılandırılmadı; başarılı aktarım üretilemez")?;
        let base = a.value.base();
        if let Some(base) = base
            && (base.area != Area::D
                || base
                    .index
                    .checked_add(words - 1)
                    .is_none_or(|end| end > m.data_register_limit.unwrap_or(9999)))
        {
            return Err("FROM/TO tam veri dizisi D belleği sınırını aşıyor".into());
        }
        if a.read {
            let base = base.ok_or("FROM hedefi çıplak D olmalı")?;
            let values = (0..words)
                .map(|i| module.read_cr(cr + i))
                .collect::<Result<Vec<_>, _>>()?;
            for (i, value) in values.into_iter().enumerate() {
                m.set_word(d(base.index + i as u16), value as i16);
            }
        } else {
            let values = if let Some(base) = base {
                (0..words)
                    .map(|i| m.word(d(base.index + i)) as u16)
                    .collect::<Vec<_>>()
            } else {
                let value = a.value.read(m) as u32;
                (0..words)
                    .map(|i| {
                        if a.wide && i % 2 == 1 {
                            (value >> 16) as u16
                        } else {
                            value as u16
                        }
                    })
                    .collect()
            };
            self.modules
                .get_mut(&slot)
                .unwrap()
                .write_batch(cr, &values, at)?;
        }
        self.last_transfer = Some(ModuleTransfer {
            pc,
            slot,
            first_cr: cr,
            words,
            read: a.read,
            at_us: at,
        });
        Ok(())
    }
}
fn compatible(cpu: ModuleCpu, kind: ModuleKind) -> bool {
    matches!(
        (cpu, kind),
        (ModuleCpu::Es2, ModuleKind::Dvp04AdE2) | (ModuleCpu::Ss2, ModuleKind::Dvp04AdS2)
    )
}
#[cfg(test)]
#[path = "module_io_tests.rs"]
mod tests;
