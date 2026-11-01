//! Read-only observations from the owned ES2/SS2 simulator.
//! Values are never inferred from the independent local simulator.
use crate::ui_theme;
use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Border, Element, Fill, Font, Theme};
use opendvp::es2_bridge::Comparison;
use opendvp::plc::{Area, Device};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    Disconnected,
    Waiting,
    Polling,
    Ready,
    Running,
}
impl Connection {
    fn status(self) -> &'static str {
        match self {
            Self::Disconnected => "Bağlantı kapalı",
            Self::Waiting => "Bağlı · program yükleme veya ilk okuma bekleniyor",
            Self::Polling => "Okuma sürüyor · gösterilen değerler önceki okumadan",
            Self::Ready => "Duraklatıldı · otomatik okuma yok",
            Self::Running => "Çalışıyor · son okuma",
        }
    }
    fn current(self) -> bool {
        matches!(self, Self::Ready | Self::Running)
    }
}
/// Live connection context is supplied by the application; pending inputs are
/// requests, never native observations.
pub struct Context<'a> {
    pub connection: Connection,
    pub pending_inputs: &'a BTreeMap<Device, bool>,
    pub error: Option<&'a str>,
    pub mismatches: Option<usize>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    On,
    Off,
    Unknown,
}
#[derive(Debug, Clone)]
pub enum Event {
    Address(String),
    Filter(Filter),
    ToggleBank,
    PreviousBank,
    NextBank,
    Select(Device),
    Find(Device),
    OpenComparison,
}
#[derive(Debug, Clone, Default)]
struct Reading {
    value: Option<bool>,
    transitions: u64,
    observed_at: Option<Instant>,
}
#[derive(Debug, Clone)]
pub struct Monitor {
    readings: BTreeMap<Device, Reading>,
    address: String,
    filter: Filter,
    bank: Option<u8>,
    selected: Option<Device>,
    observed_at: Option<Instant>,
    observed_scan: Option<u64>,
}
impl Default for Monitor {
    fn default() -> Self {
        Self {
            readings: BTreeMap::new(),
            address: String::new(),
            filter: Filter::All,
            bank: None,
            selected: None,
            observed_at: None,
            observed_scan: None,
        }
    }
}
impl Monitor {
    /// Ingest a native observation, including the initial read at scan zero.
    /// Missing or invalid bits are unknown;
    /// they break continuity for transition counts instead of becoming false.
    pub fn observe(&mut self, comparison: &Comparison) {
        let mut incoming: BTreeMap<Device, Option<bool>> = BTreeMap::new();
        for item in &comparison.rows {
            if !matches!(item.device.area, Area::X | Area::Y) || item.kind != "bit" {
                continue;
            }
            let value = match item.original {
                0 => Some(false),
                1 => Some(true),
                _ => None,
            };
            // Ambiguous duplicate observations are not trusted as a single value.
            if incoming.insert(item.device, value).is_some() {
                incoming.insert(item.device, None);
            }
        }
        let now = Instant::now();
        for (&device, reading) in &mut self.readings {
            if !incoming.contains_key(&device) {
                reading.value = None;
                reading.observed_at = None;
            }
        }
        for (device, value) in incoming {
            let reading = self.readings.entry(device).or_default();
            if let (Some(before), Some(after)) = (reading.value, value) {
                if before != after {
                    reading.transitions = reading.transitions.saturating_add(1);
                }
            }
            reading.value = value;
            reading.observed_at = value.map(|_| now);
        }
        self.observed_at = Some(now);
        self.observed_scan = Some(comparison.scan);
    }
    /// Forget readings and their transition histories for a new project or reset.
    pub fn clear(&mut self) {
        self.readings.clear();
        self.address.clear();
        self.filter = Filter::All;
        self.bank = None;
        self.selected = None;
        self.observed_at = None;
        self.observed_scan = None;
    }
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Address(value) => self.address = value,
            Event::Filter(value) => self.filter = value,
            Event::ToggleBank => self.bank = if self.bank.is_some() { None } else { Some(0) },
            Event::PreviousBank => {
                if let Some(bank) = self.bank {
                    self.bank = Some(bank.saturating_sub(1));
                }
            }
            Event::NextBank => {
                if let Some(bank) = self.bank {
                    self.bank = Some(bank.saturating_add(1).min(15));
                }
            }
            Event::Select(device) => self.selected = Some(device),
            Event::Find(_) | Event::OpenComparison => {}
        }
    }
    fn devices(&self, used: &[Device]) -> Vec<Device> {
        let set: BTreeSet<_> = if let Some(bank) = self.bank {
            [Area::X, Area::Y]
                .into_iter()
                .flat_map(|area| {
                    (0..16).map(move |index| Device {
                        area,
                        index: u16::from(bank) * 16 + index,
                    })
                })
                .collect()
        } else {
            used.iter()
                .copied()
                .filter(|d| matches!(d.area, Area::X | Area::Y))
                .chain(self.readings.keys().copied())
                .collect()
        };
        set.into_iter().collect()
    }
    fn matches(&self, device: Device) -> bool {
        let query = self.address.trim().to_ascii_uppercase();
        if !device.to_string().contains(&query) {
            return false;
        }
        let value = self.readings.get(&device).and_then(|r| r.value);
        match self.filter {
            Filter::All => true,
            Filter::On => value == Some(true),
            Filter::Off => value == Some(false),
            Filter::Unknown => value.is_none(),
        }
    }
    /// `width` is the actual content width, not the full window width.
    pub fn view<'a>(
        &'a self,
        context: Context<'_>,
        used: &[Device],
        width: f32,
    ) -> Element<'a, Event> {
        let connection = context.connection;
        let devices = self.devices(used);
        let known = devices
            .iter()
            .filter(|d| self.readings.get(d).is_some_and(|r| r.value.is_some()))
            .count();
        let on = devices
            .iter()
            .filter(|d| self.readings.get(d).is_some_and(|r| r.value == Some(true)))
            .count();
        let off = known - on;
        let unread = devices.len() - known;
        let age = self
            .observed_at
            .map_or("Henüz simülatör okuması yok".into(), |t| {
                let source = match self.observed_scan {
                    Some(0) => "Program yüklemesindeki ilk okuma".into(),
                    Some(scan) => format!("Çevrim {scan} okuması"),
                    None => "Son okuma".into(),
                };
                format!("{source} · {} sn önce", t.elapsed().as_secs())
            });
        let mut content = column![
            row![
                text("I/O izleme").size(21),
                Space::new().width(Fill),
                text("ES2/SS2 SİMÜLATÖRÜ").size(11).style(muted)
            ]
            .align_y(iced::Center),
            text(format!("{} · {}", connection.status(), age))
                .size(12)
                .width(Fill)
                .style(muted),
            text(
                "Yalnızca simülatörden okunan X/Y bitleri · gerçek PLC bağlantısı desteklenmiyor."
            )
            .size(12)
            .width(Fill)
            .style(muted),
            row![
                text_input("Adres ara: X0, Y17…", &self.address)
                    .on_input(Event::Address)
                    .padding(8)
                    .size(13)
                    .width(220),
                button(
                    text(if self.bank.is_some() {
                        "Program adresleri"
                    } else {
                        "16'lı banka"
                    })
                    .size(12)
                )
                .on_press(Event::ToggleBank)
                .padding([8, 10])
                .style(button::secondary),
                button(text("Bağlantıyı aç").size(12))
                    .on_press(Event::OpenComparison)
                    .padding([8, 10])
                    .style(button::secondary)
            ]
            .spacing(8)
            .align_y(iced::Center),
            row![
                filter_button("Tümü", Filter::All, self.filter),
                filter_button("ON", Filter::On, self.filter),
                filter_button("OFF", Filter::Off, self.filter),
                filter_button("Okunmadı", Filter::Unknown, self.filter)
            ]
            .spacing(6),
        ]
        .spacing(9);
        if let Some(error) = context.error {
            content = content.push(
                text(format!("Bağlantı / karşılaştırma: {error}"))
                    .size(12)
                    .width(Fill)
                    .style(|theme: &Theme| iced::widget::text::Style {
                        color: Some(ui_theme::colors(theme).danger),
                    }),
            );
        }
        if let Some(mismatches) = context.mismatches {
            content = content.push(
                text(format!(
                    "Karşılaştırma: tüm izlenen adreslerde {mismatches} fark"
                ))
                .size(12)
                .width(Fill)
                .style(muted),
            );
        }
        if let Some(bank) = self.bank {
            let start = u16::from(bank) * 16;
            content = content.push(
                row![
                    button(text("Önceki").size(12))
                        .on_press_maybe((bank > 0).then_some(Event::PreviousBank))
                        .padding([6, 10])
                        .style(button::secondary),
                    text(format!(
                        "Banka {} / 16 · X{:o}–X{:o}, Y{:o}–Y{:o}",
                        bank + 1,
                        start,
                        start + 15,
                        start,
                        start + 15
                    ))
                    .size(12),
                    button(text("Sonraki").size(12))
                        .on_press_maybe((bank < 15).then_some(Event::NextBank))
                        .padding([6, 10])
                        .style(button::secondary),
                ]
                .spacing(8)
                .align_y(iced::Center),
            );
        }
        content = content.push(
            text(format!(
                "Listedeki adresler: {known} okunmuş · {on} ON · {off} OFF · {unread} okunmadı"
            ))
            .size(12)
            .width(Fill)
            .style(muted),
        );
        let visible: Vec<_> = devices.into_iter().filter(|d| self.matches(*d)).collect();
        if visible.is_empty() {
            content = content.push(container(text("Gösterilecek adres yok. Programda X/Y kullanın veya banka görünümünü açın.").size(14)).padding(18).width(Fill));
        } else {
            let inputs = self.section("GİRİŞLER · X", &visible, Area::X, connection);
            let outputs = self.section("ÇIKIŞLAR · Y", &visible, Area::Y, connection);
            content = if width >= 900.0 {
                content.push(
                    row![
                        container(inputs).width(Fill),
                        container(outputs).width(Fill)
                    ]
                    .spacing(12),
                )
            } else {
                content.push(inputs).push(outputs)
            };
        }
        if let Some(device) = self.selected {
            let reading = self.readings.get(&device);
            let value = reading.and_then(|r| r.value);
            let age = reading
                .and_then(|r| r.observed_at)
                .map_or("Okuma yok".into(), |t| {
                    format!("{} sn önce okundu", t.elapsed().as_secs())
                });
            let changes = reading.map_or(0, |r| r.transitions);
            let suffix = if value.is_some() && !connection.current() {
                " · eski okuma"
            } else {
                ""
            };
            let pending = context
                .pending_inputs
                .get(&device)
                .map_or(String::new(), |value| {
                    format!(
                        "İstenen {} · sonraki çevrimi bekliyor",
                        if *value { "ON" } else { "OFF" }
                    )
                });
            content = content.push(
                container(
                    column![
                        row![
                            text(format!("{device} ayrıntısı"))
                                .font(Font::MONOSPACE)
                                .size(15),
                            Space::new().width(Fill),
                            button(text("Kullanımları bul").size(12))
                                .on_press(Event::Find(device))
                                .padding([6, 9])
                                .style(button::secondary)
                        ]
                        .spacing(8)
                        .align_y(iced::Center),
                        text(format!(
                            "Native: {}{} · {} · {} değişim",
                            value_label(value),
                            suffix,
                            age,
                            changes
                        ))
                        .size(12)
                        .width(Fill)
                        .style(muted),
                        text(pending).size(12).width(Fill).style(muted),
                        text("Değişim sayısı yalnız ardışık ve geçerli okumalardan hesaplanır.")
                            .size(11)
                            .width(Fill)
                            .style(muted),
                    ]
                    .spacing(7),
                )
                .padding(12)
                .width(Fill)
                .style(card),
            );
        }
        scrollable(content).height(Fill).into()
    }
    fn section(
        &self,
        title: &'static str,
        visible: &[Device],
        area: Area,
        connection: Connection,
    ) -> Element<'_, Event> {
        let devices: Vec<_> = visible.iter().copied().filter(|d| d.area == area).collect();
        let mut list = column![text(title).size(12).style(muted)].spacing(7);
        if devices.is_empty() {
            list = list.push(text("Eşleşen adres yok").size(12).style(muted));
        }
        for chunk in devices.chunks(4) {
            let mut cells = row![].spacing(6);
            for device in chunk {
                let device = *device;
                let reading = self.readings.get(&device);
                let value = reading.and_then(|r| r.value);
                let stale = value.is_some() && !connection.current();
                let label = if stale {
                    format!("{} · eski", value_label(value))
                } else {
                    value_label(value).into()
                };
                let selected = self.selected == Some(device);
                cells = cells.push(
                    button(
                        column![
                            text(device.to_string()).font(Font::MONOSPACE).size(13),
                            text(label).size(11)
                        ]
                        .spacing(4),
                    )
                    .on_press(Event::Select(device))
                    .width(Fill)
                    .padding([8, 8])
                    .style(move |theme, status| cell(theme, status, selected)),
                );
            }
            // Fill empty slots so cards retain their width on a partial last row.
            for _ in chunk.len()..4 {
                cells = cells.push(Space::new().width(Fill));
            }
            list = list.push(cells);
        }
        list.into()
    }
}
fn value_label(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "ON",
        Some(false) => "OFF",
        None => "Okunmadı",
    }
}
fn muted(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(ui_theme::colors(theme).muted),
    }
}
fn card(theme: &Theme) -> container::Style {
    let c = ui_theme::colors(theme);
    container::Style {
        background: Some(c.surface.into()),
        border: Border {
            color: c.line,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
fn cell(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let c = ui_theme::colors(theme);
    let mut style = button::secondary(theme, status);
    style.background = Some(
        if selected {
            c.selected
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            c.raised
        } else {
            c.surface
        }
        .into(),
    );
    style.text_color = c.ink;
    style.border = Border {
        color: if selected { c.accent } else { c.line },
        width: 1.0,
        radius: 4.0.into(),
    };
    style
}
fn filter_button(
    label: &'static str,
    choice: Filter,
    selected: Filter,
) -> iced::widget::Button<'static, Event> {
    button(text(label).size(12))
        .on_press(Event::Filter(choice))
        .padding([6, 10])
        .style(move |theme, status| cell(theme, status, choice == selected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendvp::es2_bridge::Reading as NativeReading;
    use opendvp::plc::Simulator;
    fn row(address: &str, local: u32, original: u32, kind: &'static str) -> NativeReading {
        NativeReading {
            device: address.parse().unwrap(),
            kind,
            local,
            original,
        }
    }
    fn comparison(rows: Vec<NativeReading>) -> Comparison {
        Comparison {
            local: Simulator::default(),
            scan: 1,
            mismatches: 0,
            rows,
        }
    }
    #[test]
    fn only_real_native_xy_bits_are_used_and_unknown_never_means_off() {
        let mut monitor = Monitor::default();
        monitor.observe(&comparison(vec![
            row("X0", 0, 1, "bit"),
            row("Y0", 1, 0, "bit"),
            row("X1", 0, 2, "bit"),
            row("Y1", 1, 1, "word"),
            row("M0", 0, 1, "bit"),
        ]));
        assert_eq!(monitor.readings[&"X0".parse().unwrap()].value, Some(true));
        assert_eq!(monitor.readings[&"Y0".parse().unwrap()].value, Some(false));
        assert_eq!(monitor.readings[&"X1".parse().unwrap()].value, None);
        assert!(!monitor.readings.contains_key(&"Y1".parse().unwrap()));
        assert!(!monitor.readings.contains_key(&"M0".parse().unwrap()));
        monitor.update(Event::Filter(Filter::Unknown));
        assert!(monitor.matches("X1".parse().unwrap()));
        assert!(!monitor.matches("Y0".parse().unwrap()));
    }
    #[test]
    fn transitions_count_only_consecutive_known_readings_and_clear_resets() {
        let mut monitor = Monitor::default();
        let device = "X0".parse().unwrap();
        monitor.observe(&comparison(vec![row("X0", 0, 1, "bit")]));
        assert_eq!(monitor.readings[&device].transitions, 0);
        monitor.observe(&comparison(vec![row("X0", 1, 0, "bit")]));
        assert_eq!(monitor.readings[&device].transitions, 1);
        monitor.observe(&comparison(vec![]));
        assert_eq!(monitor.readings[&device].value, None);
        monitor.observe(&comparison(vec![row("X0", 0, 1, "bit")]));
        assert_eq!(monitor.readings[&device].transitions, 1);
        monitor.update(Event::Address("X".into()));
        monitor.update(Event::Select(device));
        monitor.clear();
        assert!(monitor.readings.is_empty());
        assert!(monitor.address.is_empty());
        assert_eq!(monitor.selected, None);
    }
    #[test]
    fn connection_state_marks_previous_observations_as_stale() {
        assert!(!Connection::Disconnected.current());
        assert!(!Connection::Waiting.current());
        assert!(!Connection::Polling.current());
        assert!(Connection::Ready.current());
        assert!(Connection::Running.current());
        let mut monitor = Monitor::default();
        monitor.observe(&comparison(vec![row("Y0", 0, 1, "bit")]));
        assert_eq!(monitor.readings[&"Y0".parse().unwrap()].value, Some(true));
        // A change of connection status does not overwrite cached observations.
        let pending = BTreeMap::new();
        let _ = monitor.view(
            Context {
                connection: Connection::Disconnected,
                pending_inputs: &pending,
                error: None,
                mismatches: None,
            },
            &[],
            600.0,
        );
        assert_eq!(monitor.readings[&"Y0".parse().unwrap()].value, Some(true));
    }
    #[test]
    fn initial_snapshot_and_pending_request_remain_distinct() {
        let mut monitor = Monitor::default();
        let input = "X0".parse().unwrap();
        let mut initial = comparison(vec![row("X0", 1, 0, "bit")]);
        initial.scan = 0;
        monitor.observe(&initial);
        monitor.update(Event::Select(input));
        let pending = [(input, true)].into_iter().collect();
        let _ = monitor.view(
            Context {
                connection: Connection::Ready,
                pending_inputs: &pending,
                error: None,
                mismatches: Some(1),
            },
            &[input],
            1000.0,
        );
        assert_eq!(monitor.observed_scan, Some(0));
        assert_eq!(monitor.readings[&input].value, Some(false));
        assert_eq!(monitor.readings[&input].transitions, 0);
    }
    #[test]
    fn bank_addresses_are_octal_and_stay_in_range() {
        let mut monitor = Monitor::default();
        monitor.update(Event::ToggleBank);
        let devices = monitor.devices(&[]);
        assert_eq!(devices.len(), 32);
        assert!(devices.iter().any(|d| d.to_string() == "X17"));
        for _ in 0..20 {
            monitor.update(Event::NextBank);
        }
        let devices = monitor.devices(&[]);
        assert!(devices.iter().any(|d| d.to_string() == "Y377"));
        assert!(devices.iter().all(|d| d.index <= 255));
    }
}
