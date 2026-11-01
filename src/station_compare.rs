//! API315/316 documented local conveyor queues. Installed simulator slots are
//! not functional oracles. Capture/filter/alias policies are named explicitly;
//! this module does not emulate physical input interrupts or encoder hardware.
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StationCapacity {
    #[default]
    Documented32,
    /// XCMP's newer-firmware table. Applying this to paired YOUT is a local
    /// compatibility policy; YOUT's own prose still specifies 32.
    Extended64,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CaptureBackend {
    /// One sample per visited instruction, filter=0 only; no hardware claim.
    #[default]
    SampledVirtual,
    /// Caller supplies edges AFTER external qualification/filter completion.
    QualifiedExternal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualifiedEdge {
    pub level: bool,
    pub counter: i32,
}
#[derive(Debug, Clone, Default)]
pub struct StationState {
    pub capacity: StationCapacity,
    pub backend: CaptureBackend,
    pub notices: BTreeSet<String>,
    levels: BTreeMap<usize, bool>,
    pending: BTreeMap<usize, VecDeque<QualifiedEdge>>,
}
impl StationState {
    pub fn supply_qualified_edge(&mut self, instance: usize, edge: QualifiedEdge) {
        self.pending.entry(instance).or_default().push_back(edge);
    }
    pub fn reset_instruction_state(&mut self) {
        self.levels.clear();
        self.pending.clear();
    }
    fn maximum(&self) -> u16 {
        match self.capacity {
            StationCapacity::Documented32 => 32,
            StationCapacity::Extended64 => 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StationAction {
    Capture {
        input: Device,
        counter: Device,
        settings: Device,
        offsets: Device,
        queue: Device,
    },
    Output {
        counter: Device,
        settings: Device,
        queue: Device,
        dest: Device,
    },
}
fn at(base: Device, offset: u16) -> Device {
    Device {
        area: base.area,
        index: base.index + offset,
    }
}
fn data(text: &str) -> Result<Device, String> {
    let d: Device = text.parse()?;
    if d.area != Area::D {
        return Err("İstasyon tablo operandı D olmalı".into());
    }
    Ok(d)
}
fn counter(text: &str) -> Result<Device, String> {
    let d: Device = text.parse()?;
    if d.area != Area::C || d.index < 200 {
        return Err("XCMP/YOUT sayacı 32-bit C200..C255 olmalı".into());
    }
    Ok(d)
}
fn span(m: &Memory, base: Device, count: usize) -> bool {
    count > 0
        && usize::from(base.index)
            .checked_add(count - 1)
            .is_some_and(|last| last <= usize::from(m.data_register_limit.unwrap_or(9999)))
}
fn shape(m: &Memory, settings: Device, maximum: u16) -> (u16, u16) {
    (
        m.word(settings).clamp(1, maximum as i16) as u16,
        m.word(at(settings, 1)).clamp(1, maximum as i16) as u16,
    )
}
fn queue_words(n: u16, objects: u16) -> usize {
    usize::from(2 * n) + 4 * usize::from(n) * usize::from(objects)
}
fn event(queue: Device, n: u16, slot: u16, station: u16) -> Device {
    at(queue, 2 * n + 2 * ((slot - 1) * n + station))
}
fn next(slot: u16, slots: u16) -> u16 {
    if slot >= slots { 1 } else { slot + 1 }
}

impl StationAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        match name {
            "XCMP" if args.len() == 5 => {
                let input: Device = args[0].parse()?;
                if input.area != Area::X || input.index > 15 {
                    return Err("XCMP girişi X0..X17 (sekizlik) olmalı".into());
                }
                Ok(Self::Capture {
                    input,
                    counter: counter(&args[1])?,
                    settings: data(&args[2])?,
                    offsets: data(&args[3])?,
                    queue: data(&args[4])?,
                })
            }
            "YOUT" if args.len() == 4 => {
                let dest: Device = args[3].parse()?;
                if !matches!(dest.area, Area::Y | Area::M) || !dest.writable_bit() {
                    return Err("YOUT hedefi yazılabilir Y/M biti olmalı".into());
                }
                Ok(Self::Output {
                    counter: counter(&args[0])?,
                    settings: data(&args[1])?,
                    queue: data(&args[2])?,
                    dest,
                })
            }
            _ => Err(
                "XCMP beş, YOUT dört operand ister; D/P biçimleri bu PDF profilinde tanımlı değil"
                    .into(),
            ),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Capture { .. } => "XCMP",
            Self::Output { .. } => "YOUT",
        }
    }
    /// Genuine catalog header only; installed handlers are stubs and native
    /// upload must reject these actions rather than execute this header.
    pub fn header(&self) -> u16 {
        match self {
            Self::Capture { .. } => 0x9602,
            Self::Output { .. } => 0x9802,
        }
    }
    pub fn layout_words(&self) -> u16 {
        match self {
            Self::Capture { .. } => 11,
            Self::Output { .. } => 9,
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Capture { queue, .. } => *queue,
            Self::Output { dest, .. } => *dest,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Capture {
                input,
                counter,
                settings,
                offsets,
                queue,
            } => format!("XCMP {input} {counter} {settings} {offsets} {queue}"),
            Self::Output {
                counter,
                settings,
                queue,
                dest,
            } => format!("YOUT {counter} {settings} {queue} {dest}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Capture {
                input,
                counter,
                settings,
                offsets,
                queue,
            } => out.extend([*input, *counter, *settings, *offsets, *queue]),
            Self::Output {
                counter,
                settings,
                queue,
                dest,
            } => out.extend([*counter, *settings, *queue, *dest]),
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        let (settings, queue) = match self {
            Self::Capture {
                settings, queue, ..
            }
            | Self::Output {
                settings, queue, ..
            } => (*settings, *queue),
        };
        if !span(m, settings, 2) {
            return;
        }
        let (n, objects) = shape(m, settings, m.station_compare.maximum());
        for i in 0..queue_words(n, objects) {
            if span(m, queue, i + 1) {
                out.insert(at(queue, i as u16));
            }
        }
        match self {
            Self::Capture { offsets, .. } => {
                for i in 0..7 {
                    if span(m, settings, i + 1) {
                        out.insert(at(settings, i as u16));
                    }
                }
                for i in 0..usize::from(n) * 6 {
                    if span(m, *offsets, i + 1) {
                        out.insert(at(*offsets, i as u16));
                    }
                }
            }
            Self::Output { dest, .. } => {
                for i in 0..n {
                    if let Some(index) = dest.index.checked_add(i)
                        && index <= if dest.area == Area::Y { 255 } else { 4095 }
                    {
                        out.insert(Device {
                            area: dest.area,
                            index,
                        });
                    }
                }
            }
        }
    }
    /// Whole-instruction validation. No PLC E-code is invented for contradictory
    /// PDF partial-storage prose. Callers receive an explicit local limitation.
    pub fn validate(&self, m: &Memory) -> Result<(), String> {
        let (settings, queue) = match self {
            Self::Capture {
                settings, queue, ..
            }
            | Self::Output {
                settings, queue, ..
            } => (*settings, *queue),
        };
        if !span(
            m,
            settings,
            if matches!(self, Self::Capture { .. }) {
                7
            } else {
                2
            },
        ) {
            return Err("XCMP/YOUT ayar tablosu model D sınırını aşıyor".into());
        }
        let (n, objects) = shape(m, settings, m.station_compare.maximum());
        if !span(m, queue, queue_words(n, objects)) {
            return Err("XCMP/YOUT kuyruğu model D sınırını aşıyor; kısmi yazma uygulanmaz".into());
        }
        match self {
            Self::Capture { offsets, .. } if !span(m, *offsets, usize::from(n) * 6) => {
                return Err("XCMP karşılaştırma tablosu model D sınırını aşıyor".into());
            }
            Self::Output { dest, .. } => {
                for i in 0..n {
                    let index = dest
                        .index
                        .checked_add(i)
                        .ok_or("YOUT bit aralığı taşıyor")?;
                    let d = Device {
                        area: dest.area,
                        index,
                    };
                    if index > if dest.area == Area::Y { 255 } else { 4095 } || !d.writable_bit() {
                        return Err("YOUT ardışık çıkış aralığı yazılamaz".into());
                    }
                }
            }
            _ => {}
        }
        for i in 0..2 * n {
            if !(0..=2 * objects as i16).contains(&m.word(at(queue, i))) {
                return Err(
                    "XCMP/YOUT kuyruk indisi 0..2m dışında; bozuk halkaya işlem yapılmadı".into(),
                );
            }
        }
        Ok(())
    }
    pub fn execute(&self, m: &mut Memory, power: bool, instance: usize) {
        if !power {
            if matches!(self, Self::Capture { .. }) {
                m.station_compare.levels.remove(&instance);
                m.station_compare.pending.remove(&instance);
            }
            return;
        }
        if let Err(reason) = self.validate(m) {
            m.execution_limitations.insert(reason);
            return;
        }
        let mut state = std::mem::take(&mut m.station_compare);
        state.notices.insert("XCMP/YOUT: belgelenmiş yerel kuyruk; fiziksel kesme/enkoder ve kurulu simulator eşdeğerliği doğrulanmadı".into());
        self.execute_with(m, &mut state, instance);
        m.station_compare = state;
    }
    fn execute_with(&self, m: &mut Memory, state: &mut StationState, instance: usize) {
        match self {
            Self::Capture {
                input,
                counter,
                settings,
                offsets,
                queue,
            } => {
                let (n, objects) = shape(m, *settings, state.maximum());
                let level = m.bit(*input);
                let previous = state.levels.entry(instance).or_insert(level);
                let sampled = (*previous != level).then_some(QualifiedEdge {
                    level,
                    counter: m.double_word(*counter),
                });
                *previous = level;
                let filter = m.word(at(*settings, 2)).max(0);
                let edges: Vec<_> = match state.backend {
                    CaptureBackend::SampledVirtual if filter == 0 => sampled.into_iter().collect(),
                    CaptureBackend::SampledVirtual => {
                        m.execution_limitations.insert("XCMP filtre birimi fiziksel arka uç olmadan tanımlı değil; QualifiedExternal seçin".into());
                        return;
                    }
                    CaptureBackend::QualifiedExternal => state
                        .pending
                        .remove(&instance)
                        .unwrap_or_default()
                        .into_iter()
                        .collect(),
                };
                // Snapshot parameters before queue writes, including aliased D
                // settings/offsets. Counter arithmetic is wrapping signed i32.
                let references: Vec<_> = (0..n)
                    .map(|j| {
                        (
                            m.double_word(at(*offsets, 2 * j)),
                            m.double_word(at(*offsets, 2 * n + 2 * j)),
                            m.double_word(at(*offsets, 4 * n + 2 * j)),
                        )
                    })
                    .collect();
                let mut heads: Vec<_> = (0..n).map(|j| m.word(at(*queue, j)) as u16).collect();
                let tails: Vec<_> = (0..n).map(|j| m.word(at(*queue, n + j)) as u16).collect();
                let mut thresholds = Vec::new();
                let mut captures = Vec::new();
                for edge in edges {
                    captures.push((at(*settings, if edge.level { 3 } else { 5 }), edge.counter));
                    for (j, (reference, enter, leave)) in references.iter().copied().enumerate() {
                        if reference == 0 {
                            continue;
                        }
                        let j = j as u16;
                        let head = heads[usize::from(j)];
                        let tail = tails[usize::from(j)];
                        let new_head = next(head, 2 * objects);
                        if new_head == tail {
                            continue;
                        }
                        let error = if new_head.is_multiple_of(2) {
                            leave
                        } else {
                            enter
                        };
                        thresholds.push((
                            event(*queue, n, new_head, j),
                            edge.counter.wrapping_add(reference).wrapping_add(error),
                        ));
                        heads[usize::from(j)] = new_head;
                    }
                }
                // Defined local alias order: threshold payload, head indices,
                // then last-edge metadata. All reads precede any commit.
                for (d, value) in thresholds {
                    m.set_double_word(d, value);
                }
                for (j, value) in heads.into_iter().enumerate() {
                    m.set_word(at(*queue, j as u16), value as i16);
                }
                for (d, value) in captures {
                    m.set_double_word(d, value);
                }
            }
            Self::Output {
                counter,
                settings,
                queue,
                dest,
            } => {
                let (n, objects) = shape(m, *settings, state.maximum());
                let count = m.double_word(*counter);
                // Snapshot all queue values before output writes (M/Y cannot
                // alias D). A station catches up at most one full ring/visit.
                for j in 0..n {
                    let head = m.word(at(*queue, j)) as u16;
                    if head == 0 {
                        continue;
                    }
                    let mut tail = (m.word(at(*queue, n + j)) as u16).max(1);
                    let output = at(*dest, j);
                    for _ in 0..2 * objects {
                        let pair = if tail.is_multiple_of(2) {
                            tail - 1
                        } else {
                            tail
                        };
                        let entering = m.double_word(event(*queue, n, pair, j));
                        let leaving = m.double_word(event(*queue, n, pair + 1, j));
                        if entering == 0 && leaving == 0 {
                            m.set_bit(output, false);
                            if tail == head {
                                break;
                            }
                            tail = next(tail, 2 * objects);
                            if tail == head {
                                break;
                            }
                            tail = next(tail, 2 * objects);
                            continue;
                        }
                        let threshold = if tail.is_multiple_of(2) {
                            leaving
                        } else {
                            entering
                        };
                        if count < threshold {
                            break;
                        }
                        m.set_bit(output, !tail.is_multiple_of(2));
                        if tail == head {
                            break;
                        }
                        tail = next(tail, 2 * objects);
                    }
                    m.set_word(at(*queue, n + j), tail as i16);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(index: u16) -> Device {
        Device {
            area: Area::D,
            index,
        }
    }
    fn capture() -> StationAction {
        StationAction::parse(
            "XCMP",
            &["X4", "C251", "D0", "D500", "D2000"].map(String::from),
        )
        .unwrap()
    }
    fn output() -> StationAction {
        StationAction::parse("YOUT", &["C251", "D0", "D2000", "Y0"].map(String::from)).unwrap()
    }
    fn fixture() -> Memory {
        let mut m = Memory::default();
        m.set_word(d(0), 3);
        m.set_word(d(1), 4);
        for (i, v) in [2000, 3000, 4000, 100, 120, 130, 50, -20, 20]
            .into_iter()
            .enumerate()
        {
            m.set_double_word(d(500 + 2 * i as u16), v);
        }
        m
    }
    #[test]
    fn pdf_three_station_thresholds_and_outputs() {
        let mut m = fixture();
        m.station_compare.backend = CaptureBackend::QualifiedExternal;
        for (level, counter) in [(true, 3000), (false, 3500), (true, 4500), (false, 5000)] {
            m.station_compare
                .supply_qualified_edge(7, QualifiedEdge { level, counter });
        }
        capture().execute(&mut m, true, 7);
        for (i, v) in [
            5100, 6120, 7130, 5550, 6480, 7520, 6600, 7620, 8630, 7050, 7980, 9020,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(m.double_word(d(2006 + 2 * i as u16)), v);
        }
        let c: Device = "C251".parse().unwrap();
        for (count, expected) in [
            (5200, [true, false, false]),
            (6200, [false, true, false]),
            (6800, [true, false, false]),
            (7300, [false, false, true]),
            (7700, [false, true, false]),
            (8000, [false, false, false]),
            (8700, [false, false, true]),
        ] {
            m.set_double_word(c, count);
            output().execute(&mut m, true, 8);
            for (j, value) in expected.into_iter().enumerate() {
                assert_eq!(
                    m.bit(Device {
                        area: Area::Y,
                        index: j as u16
                    }),
                    value,
                    "counter={count}"
                );
            }
        }
        assert_eq!(m.double_word(d(3)), 4500);
        assert_eq!(m.double_word(d(5)), 5000);
    }
    #[test]
    fn initial_on_means_first_falling_is_enter_and_false_rearms() {
        let mut m = fixture();
        let x: Device = "X4".parse().unwrap();
        let c: Device = "C251".parse().unwrap();
        m.set_input(x, true);
        capture().execute(&mut m, true, 1);
        assert_eq!(m.word(d(2000)), 0);
        m.set_input(x, false);
        m.set_double_word(c, 123);
        capture().execute(&mut m, true, 1);
        assert_eq!(m.double_word(d(2006)), 2223);
        capture().execute(&mut m, false, 1);
        m.set_input(x, true);
        capture().execute(&mut m, true, 1);
        assert_eq!(m.word(d(2000)), 1);
    }
    #[test]
    fn filter_requires_qualified_backend_and_bounds_are_atomic() {
        let mut m = fixture();
        m.set_word(d(2), 5);
        capture().execute(&mut m, true, 1);
        assert!(!m.execution_limitations.is_empty());
        assert_eq!(m.word(d(2000)), 0);
        let mut m = fixture();
        m.data_register_limit = Some(2020);
        m.set_word(d(2000), 77);
        capture().execute(&mut m, true, 1);
        assert_eq!(m.word(d(2000)), 77);
        assert!(!m.system_bit(1067));
    }
    #[test]
    fn full_ring_drops_event_without_overwriting_and_zero_reference_skips() {
        let mut m = fixture();
        m.station_compare.backend = CaptureBackend::QualifiedExternal;
        m.set_word(d(2000), 8);
        m.set_word(d(2003), 1);
        m.set_double_word(d(502), 0);
        m.station_compare.supply_qualified_edge(
            1,
            QualifiedEdge {
                level: true,
                counter: 10,
            },
        );
        capture().execute(&mut m, true, 1);
        assert_eq!(m.word(d(2000)), 8);
        assert_eq!(m.word(d(2001)), 0);
        assert_eq!(m.word(d(2002)), 1);
    }
    #[test]
    fn validation_preserves_counter_and_output_matrices() {
        for (name, args) in [
            ("XCMP", vec!["X20", "C251", "D0", "D10", "D100"]),
            ("XCMP", vec!["X4", "C199", "D0", "D10", "D100"]),
            ("YOUT", vec!["X0", "D0", "D100", "Y0"]),
            ("YOUT", vec!["C251", "D0", "D100", "S0"]),
        ] {
            assert!(
                StationAction::parse(
                    name,
                    &args.into_iter().map(String::from).collect::<Vec<_>>()
                )
                .is_err()
            );
        }
    }
    #[test]
    fn queue_wrap_consumes_old_terminal_then_new_entry() {
        let mut m = Memory::default();
        m.set_word(d(0), 1);
        m.set_word(d(1), 2);
        m.set_word(d(2000), 1);
        m.set_word(d(2001), 4);
        m.set_double_word(d(2002), 100); // new entering slot1
        m.set_double_word(d(2008), 50); // previous leaving slot4
        m.set_double_word("C251".parse().unwrap(), 110);
        output().execute(&mut m, true, 9);
        assert!(m.bit("Y0".parse().unwrap()));
        assert_eq!(m.word(d(2001)), 1);
    }
    #[test]
    fn aliased_metadata_cannot_change_indices_mid_transaction() {
        let mut m = Memory::default();
        m.station_compare.backend = CaptureBackend::QualifiedExternal;
        m.set_word(d(0), 1);
        m.set_word(d(1), 2);
        m.set_double_word(d(500), 100);
        let a = StationAction::parse(
            "XCMP",
            &["X4", "C251", "D0", "D500", "D3"].map(String::from),
        )
        .unwrap();
        m.station_compare.supply_qualified_edge(
            1,
            QualifiedEdge {
                level: true,
                counter: i32::MAX,
            },
        );
        a.execute(&mut m, true, 1);
        // Metadata deliberately wins the final alias; payload was calculated
        // from the original head0, not the metadata's invalid index32767.
        assert_eq!(m.double_word(d(5)), i32::MAX.wrapping_add(100));
        assert_eq!(m.double_word(d(3)), i32::MAX);
    }
}
