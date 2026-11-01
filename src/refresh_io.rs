//! PDF API 50/51/52 process-image refresh and deterministic virtual input matrix.
//! Physical propagation and the installed simulator's shortcuts are separate
//! profiles.
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet};

fn at(area: Area, index: u16) -> Device {
    Device { area, index }
}
fn constant(s: &str) -> Result<u16, String> {
    if let Some(n) = s.strip_prefix('K') {
        n.parse().map_err(|_| "K/H sabiti gerekli".into())
    } else if let Some(n) = s.strip_prefix('H') {
        u16::from_str_radix(n, 16).map_err(|_| "K/H sabiti gerekli".into())
    } else {
        Err("K/H sabiti gerekli; register operandı yok".into())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefreshAction {
    Refresh {
        start: Device,
        count: u16,
    },
    Filter {
        milliseconds: u16,
    },
    Matrix {
        source: Device,
        strobe: Device,
        dest: Device,
        rows: u8,
    },
}
impl RefreshAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        match (name, args.len()) {
            ("REF", 2) => {
                let start: Device = args[0].parse()?;
                let count = constant(&args[1])?;
                // Y4 is explicitly drawn in PDF266, despite PDF265's general
                // decimal-spelling alignment sentence. Keep that documented case.
                if !matches!(start.area, Area::X | Area::Y)
                    || (!start.index.is_multiple_of(8)
                        && !(start.area == Area::Y && start.index == 4))
                    || !(4..=256).contains(&count)
                {
                    return Err("REF: X/Y onluk adres grubu (veya Y4), K/H4..256 gerekli; MPU profili çalışma anında denetlenir".into());
                }
                Ok(Self::Refresh { start, count })
            }
            ("REFF", 1) => {
                let milliseconds = constant(&args[0])?;
                if !(2..=20).contains(&milliseconds) {
                    return Err("REFF K/H2..20 gerekli".into());
                }
                Ok(Self::Filter { milliseconds })
            }
            ("MTR", 4) => {
                let source: Device = args[0].parse()?;
                let strobe: Device = args[1].parse()?;
                let dest: Device = args[2].parse()?;
                let rows = constant(&args[3])?;
                if source.area != Area::X
                    || !source.index.is_multiple_of(8)
                    || strobe.area != Area::Y
                    || !strobe.index.is_multiple_of(8)
                    || !matches!(dest.area, Area::Y | Area::M | Area::S)
                    || !dest
                        .index
                        .is_multiple_of(if dest.area == Area::Y { 8 } else { 10 })
                    || !(2..=8).contains(&rows)
                {
                    return Err("MTR: hizalı X, Y, Y/M/S ve K/H2..8 gerekli".into());
                }
                let action = Self::Matrix {
                    source,
                    strobe,
                    dest,
                    rows: rows as u8,
                };
                action.matrix_cells()?;
                Ok(action)
            }
            _ => Err(format!(
                "{name}: REF 2, REFF 1, MTR 4 operand alır; P dış sarmalayıcıyla uygulanır"
            )),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Refresh { .. } => "REF",
            Self::Filter { .. } => "REFF",
            Self::Matrix { .. } => "MTR",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Refresh { .. } => 0x7400,
            Self::Filter { .. } => 0x7600,
            Self::Matrix { .. } => 0x7800,
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Refresh { start, .. } => *start,
            Self::Filter { .. } => at(Area::D, 1020),
            Self::Matrix { dest, .. } => *dest,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Refresh { start, count } => format!("REF {start} K{count}"),
            Self::Filter { milliseconds } => format!("REFF K{milliseconds}"),
            Self::Matrix {
                source,
                strobe,
                dest,
                rows,
            } => format!("MTR {source} {strobe} {dest} K{rows}"),
        }
    }
    fn matrix_cells(&self) -> Result<Vec<Device>, String> {
        let Self::Matrix {
            source,
            strobe,
            dest,
            rows,
        } = self
        else {
            return Ok(Vec::new());
        };
        if source.index + 7 > 255 || strobe.index + u16::from(*rows) > 256 {
            return Err("MTR kaynak/strobe sınırı aşılıyor".into());
        }
        let stride = if dest.area == Area::Y { 8 } else { 10 };
        let max = match dest.area {
            Area::Y => 255,
            Area::M => 4095,
            Area::S => 1023,
            _ => return Err("MTR hedef alanı".into()),
        };
        let mut cells = Vec::new();
        for r in 0..u16::from(*rows) {
            for b in 0..8 {
                let index = dest
                    .index
                    .checked_add(r * stride + b)
                    .ok_or("MTR hedef taşması")?;
                if index > max {
                    return Err("MTR hedef sınırı aşılıyor".into());
                }
                let d = at(dest.area, index);
                if !d.writable_bit() {
                    return Err("MTR salt okunur sistem biti hedefleniyor".into());
                }
                cells.push(d);
            }
        }
        Ok(cells)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Refresh { start, count } => {
                for index in start.index..start.index.saturating_add(*count).min(256) {
                    out.insert(at(start.area, index));
                }
                for i in 1672..=1677 {
                    out.insert(at(Area::M, i));
                }
            }
            Self::Filter { .. } => {
                out.extend([at(Area::D, 1020), at(Area::D, 1021)]);
            }
            Self::Matrix {
                source,
                strobe,
                rows,
                ..
            } => {
                out.extend((0..8).map(|n| at(Area::X, source.index + n)));
                out.extend((0..u16::from(*rows)).map(|n| at(Area::Y, strobe.index + n)));
                if let Ok(cells) = self.matrix_cells() {
                    out.extend(cells)
                }
                out.insert(at(Area::M, 1029));
            }
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Self::Refresh { start, count } = self
            && let Ok((area, range)) = m.refresh_io.profile.refresh_range(*start, *count)
        {
            out.extend(range.map(|i| at(area, i)));
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, instance: usize) {
        let now = m.timing_io.now_us;
        let raw = m.timing_io.physical_inputs.clone();
        let owned = m.timing_io.output_owners.keys().copied().collect();
        let mut state = std::mem::take(&mut m.refresh_io);
        if let Self::Matrix { .. } = self {
            if !power {
                state.cancel_matrix(m, instance, &owned);
            } else {
                state.start_matrix(m, self, instance, now, &owned);
            }
        } else if power {
            match self {
                Self::Filter { milliseconds } => {
                    let installed = state.filter_profile == FilterProfile::InstalledRegisters;
                    if !installed {
                        state.filter_profile = FilterProfile::DocumentedSoftware;
                    }
                    m.set_word(at(Area::D, 1020), *milliseconds as i16);
                    if installed || state.profile.second_filter_bank {
                        m.set_word(at(Area::D, 1021), *milliseconds as i16);
                    }
                }
                Self::Refresh { start, count } => {
                    let pulse = (1672..=1677).any(|i| m.system_bit(i));
                    if pulse {
                        state.refresh_pulse_counts(m);
                    } else if let Ok((area, range)) = state.profile.refresh_range(*start, *count) {
                        state.advance_inner(m, now, &raw, &owned);
                        for index in range {
                            let d = at(area, index);
                            if area == Area::X {
                                let v = state.input_level(d, &raw);
                                m.set_input_image(d, v);
                            } else if !owned.contains(&d) {
                                m.set_physical_output(d, m.bit(d));
                            }
                        }
                        state.refresh_analog(m);
                    } else {
                        m.execution_limitations
                            .insert("REF operand is outside configured MPU refresh policy".into());
                    }
                }
                _ => {}
            }
        }
        m.refresh_io = state;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FilterProfile {
    #[default]
    ExplicitInputImage,
    DocumentedSoftware,
    /// Hash-bound installed ES2/SS2 REFF handler writes both D1020/D1021,
    /// without evidence of a physical input-filter backend in those runtimes.
    InstalledRegisters,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MpuFamily {
    Virtual,
    Es2,
    Ex2,
    Ss2,
    Sa2,
    Se,
    Se2,
    Sx2,
    Ec5,
}
/// Counts describe the actual MPU, not extension modules. Firmware gates are
/// explicit capabilities: callers must not infer them from a family name.
#[derive(Clone, Debug)]
pub struct RefreshProfile {
    pub family: MpuFamily,
    pub input_points: u16,
    pub output_points: u16,
    pub second_filter_bank: bool,
    pub pulse_refresh: bool,
    pub analog_refresh: bool,
}
impl Default for RefreshProfile {
    fn default() -> Self {
        Self {
            family: MpuFamily::Virtual,
            input_points: 256,
            output_points: 256,
            second_filter_bank: false,
            pulse_refresh: false,
            analog_refresh: false,
        }
    }
}
impl RefreshProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.input_points == 0
            || self.input_points > 256
            || self.output_points == 0
            || self.output_points > 256
        {
            Err("MPU X/Y kapasitesi 1..256 olmalı".into())
        } else {
            Ok(())
        }
    }
    pub fn refresh_range(
        &self,
        start: Device,
        count: u16,
    ) -> Result<(Area, std::ops::Range<u16>), String> {
        self.validate()?;
        let total = if start.area == Area::X {
            self.input_points
        } else if start.area == Area::Y {
            self.output_points
        } else {
            return Err("REF X/Y gerekli".into());
        };
        let min = if matches!(
            self.family,
            MpuFamily::Es2 | MpuFamily::Ex2 | MpuFamily::Virtual
        ) {
            4
        } else {
            8
        };
        if count < min
            || count > total
            || (matches!(self.family, MpuFamily::Es2 | MpuFamily::Ex2) && !count.is_multiple_of(4))
            || start.index >= total
        {
            return Err("REF MPU sayı/adres sınırı".into());
        }
        let range = match (start.area, start.index) {
            (Area::X, 0) if count <= 8 => 0..8.min(total),
            (Area::Y, 0) if count <= 8 => 0..count.min(total),
            (_, 0) => 0..total,
            (Area::X, 8) => 8..total,
            (Area::Y, 4 | 8) => 4..total,
            _ => return Err("REF yalnız MPU alt/üst bankası için tanımlı".into()),
        };
        Ok((start.area, range))
    }
}
#[derive(Clone, Debug)]
struct FilterCell {
    raw: bool,
    accepted: bool,
    changed_us: u64,
    bypass: bool,
}
#[derive(Clone, Debug)]
struct MatrixRun {
    id: usize,
    action: RefreshAction,
    row: u8,
    deadline: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatrixBackend {
    #[default]
    ExplicitRows,
    RawMultiplexedInputs,
}
#[derive(Clone, Debug)]
pub struct RefreshState {
    pub profile: RefreshProfile,
    pub filter_profile: FilterProfile,
    pub matrix_backend: MatrixBackend,
    pub notices: BTreeSet<String>,
    pub analog_inputs: [i16; 4],
    pub analog_outputs: [i16; 2],
    pub pulse_positions: BTreeMap<u8, i32>,
    filters: BTreeMap<Device, FilterCell>,
    filter_ms: [u16; 2],
    matrix_rows: BTreeMap<usize, Vec<u8>>,
    run: Option<MatrixRun>,
    completion_aged: bool,
    completion_emitted: bool,
    completion_revision: u64,
    last_processed_us: u64,
}
impl Default for RefreshState {
    fn default() -> Self {
        Self {
            profile: RefreshProfile::default(),
            filter_profile: FilterProfile::default(),
            matrix_backend: MatrixBackend::default(),
            notices: BTreeSet::new(),
            analog_inputs: [0; 4],
            analog_outputs: [0; 2],
            pulse_positions: BTreeMap::new(),
            filters: BTreeMap::new(),
            filter_ms: [10, 10],
            matrix_rows: BTreeMap::new(),
            run: None,
            completion_aged: false,
            completion_emitted: false,
            completion_revision: 0,
            last_processed_us: 0,
        }
    }
}
impl RefreshState {
    /// Execution abort freezes the current output image. Watchdog's caller
    /// separately forces all Y OFF. Keep matrix wiring, sampled values, input
    /// filters and their physical time; no asynchronous row may reassert Y.
    pub(crate) fn abort_output_engine(m: &mut Memory) {
        m.refresh_io.run = None;
    }
    pub fn configure(&mut self, profile: RefreshProfile) -> Result<(), String> {
        profile.validate()?;
        if self.run.is_some() {
            return Err("MTR çalışırken MPU profili değiştirilemez".into());
        }
        self.profile = profile;
        Ok(())
    }
    pub fn set_matrix_rows(&mut self, instance: usize, rows: Vec<u8>) -> Result<(), String> {
        if !(2..=8).contains(&rows.len()) {
            return Err("Sanal matris 2..8 satır gerektirir".into());
        }
        if self.run.as_ref().is_some_and(|r|r.id==instance&&matches!(&r.action,RefreshAction::Matrix{rows:n,..} if usize::from(*n)!=rows.len())){return Err("Çalışan matrisin satır sayısı değiştirilemez".into())}
        self.matrix_rows.insert(instance, rows);
        Ok(())
    }
    pub fn matrix_owner(&self) -> Option<usize> {
        self.run.as_ref().map(|r| r.id)
    }
    pub(crate) fn matrix_pins(&self) -> (Vec<Device>, Vec<Device>) {
        let Some(MatrixRun {
            action:
                RefreshAction::Matrix {
                    source,
                    strobe,
                    dest,
                    rows,
                },
            ..
        }) = &self.run
        else {
            return (Vec::new(), Vec::new());
        };
        let inputs = (0..8).map(|i| at(Area::X, source.index + i)).collect();
        let mut outputs: Vec<_> = (0..u16::from(*rows))
            .map(|i| at(Area::Y, strobe.index + i))
            .collect();
        if dest.area == Area::Y {
            outputs.extend((0..u16::from(*rows) * 8).map(|i| at(Area::Y, dest.index + i)));
        }
        (inputs, outputs)
    }
    pub fn next_deadline_us(&self) -> Option<u64> {
        let matrix = self.run.as_ref().map(|r| r.deadline);
        let filter = self
            .filters
            .iter()
            .filter(|(_, c)| c.raw != c.accepted)
            .map(|(d, c)| c.changed_us.saturating_add(self.delay_us(*d, c.bypass)))
            .min();
        matrix
            .into_iter()
            .chain(filter)
            .filter(|&t| t > self.last_processed_us)
            .min()
    }
    fn delay_us(&self, d: Device, bypass: bool) -> u64 {
        if bypass || self.filter_profile != FilterProfile::DocumentedSoftware {
            return 0;
        }
        let bank = if d.index < 8 {
            Some(0)
        } else if d.index < 16 && self.profile.second_filter_bank {
            Some(1)
        } else {
            None
        };
        bank.map_or(0, |n| u64::from(self.filter_ms[n]) * 1000)
    }
    /// Raw physical edge only; ordinary process image remains latched. IRQ/HSC
    /// consumers receive the original edge through their independent hooks.
    pub fn raw_input_edge(&mut self, input: Device, level: bool, at_us: u64, bypass: bool) {
        if input.area != Area::X {
            return;
        }
        let delay = self.delay_us(input, bypass);
        let c = self.filters.entry(input).or_insert(FilterCell {
            raw: !level,
            accepted: !level,
            changed_us: at_us,
            bypass,
        });
        if c.raw != level {
            c.raw = level;
            c.changed_us = at_us;
        }
        c.bypass = bypass;
        if delay == 0 {
            c.accepted = level;
        }
    }
    pub fn seed_input(&mut self, input: Device, level: bool, now_us: u64) {
        self.filters.insert(
            input,
            FilterCell {
                raw: level,
                accepted: level,
                changed_us: now_us,
                bypass: false,
            },
        );
    }
    /// Explicit UI/protocol image override, not a physical transition/filter test.
    pub fn seed_input_image(&mut self, input: Device, level: bool, at_us: u64) {
        self.seed_input(input, level, at_us);
    }
    /// Cold power-on only; END commits subsequent direct D writes. This does
    /// not manufacture a physical level or an interrupt edge.
    pub fn power_on(m: &mut Memory) {
        m.set_word(at(Area::D, 1020), 10);
        if m.refresh_io.profile.second_filter_bank {
            m.set_word(at(Area::D, 1021), 10);
        }
        m.refresh_io.filter_ms = [10, 10];
    }
    fn input_level(&self, d: Device, raw: &BTreeMap<Device, bool>) -> bool {
        if self.filter_profile != FilterProfile::DocumentedSoftware {
            raw.get(&d).copied().unwrap_or(false)
        } else {
            self.filters
                .get(&d)
                .map_or_else(|| raw.get(&d).copied().unwrap_or(false), |c| c.accepted)
        }
    }
    pub fn advance(
        m: &mut Memory,
        now_us: u64,
        raw: &BTreeMap<Device, bool>,
        owned: &BTreeSet<Device>,
    ) {
        let mut s = std::mem::take(&mut m.refresh_io);
        s.advance_inner(m, now_us, raw, owned);
        m.refresh_io = s;
    }
    fn advance_inner(
        &mut self,
        m: &mut Memory,
        now_us: u64,
        raw: &BTreeMap<Device, bool>,
        owned: &BTreeSet<Device>,
    ) {
        self.last_processed_us = self.last_processed_us.max(now_us);
        let delays: Vec<_> = self
            .filters
            .iter()
            .map(|(d, c)| (*d, self.delay_us(*d, c.bypass)))
            .collect();
        for (d, delay) in delays {
            let c = self.filters.get_mut(&d).unwrap();
            if now_us.saturating_sub(c.changed_us) >= delay {
                c.accepted = c.raw;
            }
        }
        while self.run.as_ref().is_some_and(|r| r.deadline <= now_us) {
            let mut run = self.run.take().unwrap();
            let RefreshAction::Matrix {
                source,
                strobe,
                dest,
                rows,
            } = run.action
            else {
                unreachable!()
            };
            if (0..u16::from(rows)).any(|i| owned.contains(&at(Area::Y, strobe.index + i))) {
                m.execution_limitations
                    .insert("MTR strobe is owned by a high-speed/external output backend".into());
                for i in 0..u16::from(rows) {
                    let d = at(Area::Y, strobe.index + i);
                    if !owned.contains(&d) {
                        m.set_bit(d, false);
                        m.set_physical_output(d, false);
                    }
                }
                break;
            }
            let value = match self.matrix_backend {
                MatrixBackend::ExplicitRows => self.matrix_rows[&run.id][usize::from(run.row)],
                MatrixBackend::RawMultiplexedInputs => {
                    let mut v = 0;
                    for b in 0..8 {
                        if self.input_level(at(Area::X, source.index + b), raw) {
                            v |= 1 << b;
                        }
                    }
                    v
                }
            };
            let stride = if dest.area == Area::Y { 8 } else { 10 };
            for b in 0..8 {
                m.set_bit(
                    at(dest.area, dest.index + u16::from(run.row) * stride + b),
                    value & (1 << b) != 0,
                );
            }
            run.row += 1;
            if run.row == rows {
                run.row = 0;
                m.set_system(1029, true);
                self.completion_revision = m.completion_flag_revision();
                self.completion_emitted = true;
                self.completion_aged = false;
            }
            self.strobes(m, strobe, rows, Some(run.row));
            run.deadline = run.deadline.saturating_add(25_000);
            if run.deadline == u64::MAX {
                m.execution_limitations
                    .insert("MTR virtual clock overflow".into());
                break;
            }
            self.run = Some(run);
        }
    }
    pub fn begin_scan(
        m: &mut Memory,
        now_us: u64,
        raw: &BTreeMap<Device, bool>,
        bypass: &BTreeSet<Device>,
    ) {
        let mut s = std::mem::take(&mut m.refresh_io);
        if s.completion_emitted && s.completion_aged {
            if m.completion_flag_revision() == s.completion_revision {
                m.set_system(1029, false);
            }
            s.completion_emitted = false;
        }
        s.completion_aged = s.completion_emitted;
        for (&d, &v) in raw {
            if !s.filters.contains_key(&d) {
                s.seed_input(d, v, now_us);
            }
            if let Some(c) = s.filters.get_mut(&d) {
                c.bypass = bypass.contains(&d);
            }
        }
        let owned = BTreeSet::new(); // Only filter advancement here; MTR must be
        // advanced by the time hook with the actual ownership snapshot.
        let run = s.run.take();
        s.advance_inner(m, now_us, raw, &owned);
        s.run = run;
        for index in 0..s.profile.input_points {
            let d = at(Area::X, index);
            m.set_input_image(d, s.input_level(d, raw));
        }
        m.refresh_io = s;
    }
    pub fn end_scan(m: &mut Memory) {
        let mut s = std::mem::take(&mut m.refresh_io);
        if s.filter_profile == FilterProfile::DocumentedSoftware {
            for bank in 0..if s.profile.second_filter_bank { 2 } else { 1 } {
                let v = m.word(at(Area::D, 1020 + bank as u16));
                if (0..=20).contains(&v) {
                    s.filter_ms[bank] = v as u16;
                } else {
                    m.execution_limitations.insert(format!(
                        "D{} input filter outside documented 0..20 ms",
                        1020 + bank
                    ));
                }
            }
            // A shorter newly committed duration may already have elapsed.
            // Resolve it at END instead of offering a past scheduler deadline.
            let now = s.last_processed_us;
            let ready: Vec<_> = s
                .filters
                .iter()
                .filter_map(|(d, c)| {
                    (now.saturating_sub(c.changed_us) >= s.delay_us(*d, c.bypass)).then_some(*d)
                })
                .collect();
            for d in ready {
                let c = s.filters.get_mut(&d).unwrap();
                c.accepted = c.raw;
            }
        }
        m.refresh_io = s;
    }
    pub fn active_filter_ms(&self, bank: usize) -> Option<u16> {
        self.filter_ms.get(bank).copied()
    }
    fn strobes(&self, m: &mut Memory, start: Device, rows: u8, row: Option<u8>) {
        for i in 0..rows {
            let d = at(Area::Y, start.index + u16::from(i));
            let high = Some(i) == row;
            m.set_bit(d, high);
            m.set_physical_output(d, high);
        }
    }
    fn start_matrix(
        &mut self,
        m: &mut Memory,
        action: &RefreshAction,
        id: usize,
        now: u64,
        owned: &BTreeSet<Device>,
    ) {
        if self.run.as_ref().is_some_and(|r| r.id == id) {
            return;
        }
        if self.run.is_some() {
            self.notices
                .insert("MTR global owner retained; later simultaneous site waits".into());
            return;
        }
        let RefreshAction::Matrix {
            source,
            strobe,
            dest,
            rows,
        } = action
        else {
            return;
        };
        let cells = action.matrix_cells();
        if !cells.as_ref().is_ok_and(|cells| {
            cells
                .iter()
                .all(|d| d.area != Area::M || d.index <= m.relay_limit.unwrap_or(4095))
        }) || source.index + 8 > self.profile.input_points
            || strobe.index + u16::from(*rows) > self.profile.output_points
        {
            m.execution_limitations
                .insert("MTR entire matrix is outside configured MPU/device spans".into());
            return;
        }
        if (0..u16::from(*rows)).any(|i| owned.contains(&at(Area::Y, strobe.index + i))) {
            m.execution_limitations
                .insert("MTR strobe conflicts with owned output".into());
            return;
        }
        if (0..8).any(|i| {
            m.timing_io
                .input_owners
                .contains_key(&at(Area::X, source.index + i))
        }) || (dest.area == Area::Y
            && (0..u16::from(*rows) * 8).any(|i| owned.contains(&at(Area::Y, dest.index + i))))
        {
            m.execution_limitations
                .insert("MTR input or Y result overlaps an owned peripheral pin".into());
            return;
        }
        if dest.area == Area::Y
            && (0..u16::from(*rows) * 8).any(|i| {
                (strobe.index..strobe.index + u16::from(*rows)).contains(&(dest.index + i))
            })
        {
            m.execution_limitations
                .insert("MTR Y destination overlaps strobe; physical alias policy unproven".into());
            return;
        }
        if self.matrix_backend == MatrixBackend::ExplicitRows
            && !self
                .matrix_rows
                .get(&id)
                .is_some_and(|v| v.len() == usize::from(*rows))
        {
            m.execution_limitations.insert("MTR requires configured virtual matrix rows or explicit RawMultiplexedInputs backend".into());
            return;
        }
        self.notices.insert(
            "MTR virtual row slot=25000us; PDF says approximately25ms, no hardware parity claim"
                .into(),
        );
        self.strobes(m, *strobe, *rows, Some(0));
        self.run = Some(MatrixRun {
            id,
            action: action.clone(),
            row: 0,
            deadline: now.saturating_add(25_000),
        });
    }
    fn cancel_matrix(&mut self, m: &mut Memory, id: usize, owned: &BTreeSet<Device>) {
        if self.run.as_ref().is_some_and(|r| r.id == id) {
            let run = self.run.take().unwrap();
            if let RefreshAction::Matrix { strobe, rows, .. } = run.action {
                for i in 0..u16::from(rows) {
                    let d = at(Area::Y, strobe.index + i);
                    if !owned.contains(&d) {
                        m.set_bit(d, false);
                        m.set_physical_output(d, false);
                    }
                }
            }
            self.notices.insert("MTR local false policy: release owner/strobes, retain sampled matrix; rearm starts row0".into());
        }
    }
    fn refresh_pulse_counts(&mut self, m: &mut Memory) {
        if !self.profile.pulse_refresh {
            m.execution_limitations.insert(
                "REF pulse-position refresh requires configured firmware capability".into(),
            );
            return;
        }
        for (flag, y, d) in [
            (1672, 0, 1030),
            (1673, 1, 1032),
            (1674, 2, 1336),
            (1675, 3, 1338),
            (1676, 4, 1375),
            (1677, 6, 1377),
        ] {
            if m.system_bit(flag) {
                if y >= 4
                    && !matches!(
                        self.profile.family,
                        MpuFamily::Ec5 | MpuFamily::Se2 | MpuFamily::Virtual
                    )
                {
                    m.execution_limitations
                        .insert("REF Y4/Y6 pulse refresh requires EC5/SE2 model capability".into());
                    continue;
                }
                if let Some(&position) = self.pulse_positions.get(&y) {
                    m.set_double_word(at(Area::D, d), position);
                } else {
                    m.execution_limitations
                        .insert(format!("REF Y{y} pulse position backend is not configured"));
                }
            }
        }
    }
    fn refresh_analog(&mut self, m: &mut Memory) {
        if self.profile.analog_refresh
            && matches!(self.profile.family, MpuFamily::Ex2 | MpuFamily::Sx2)
        {
            if m.system_bit(1180) {
                for (i, &value) in self.analog_inputs.iter().enumerate() {
                    m.set_word(at(Area::D, 1110 + i as u16), value);
                }
                m.set_system(1180, false);
            }
            if m.system_bit(1181) {
                self.analog_outputs = [m.word(at(Area::D, 1116)), m.word(at(Area::D, 1117))];
                m.set_system(1181, false);
            }
        }
    }
}

#[cfg(test)]
#[path = "refresh_io_tests.rs"]
mod tests;
