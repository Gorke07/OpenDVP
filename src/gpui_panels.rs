#[path = "gpui_config_views.rs"]
mod config_views;
use super::*;

const LIVE_LAMP: u32 = 0x22c55e;
// RFD requires an STA thread on Windows. GPUI pool threads already use MTA.
pub(super) async fn dialog<T: Send + 'static>(
    job: impl FnOnce() -> Option<Result<T, String>> + Send + 'static,
) -> Option<Result<T, String>> {
    let (tx, rx) = futures::channel::oneshot::channel();
    if let Err(error) = std::thread::Builder::new()
        .name("opendvp-file-dialog".into())
        .spawn(move || {
            let _ = tx.send(job());
        })
    {
        return Some(Err(error.to_string()));
    }
    rx.await.unwrap_or_else(|_| {
        Some(Err(
            tr("Dosya iletişim kutusu beklenmedik şekilde kapandı.").into()
        ))
    })
}
use crate::{
    connection_profiles::{Event, Field, Kind, Manager},
    plc_connection::{self, Client, Sample},
};
use opendvp::{
    file_memory::{FileMemoryProfile, FileModel, FileStore, FirmwareVersion},
    module_io::{ModuleCpu, ModuleCpuProfile, VirtualModule},
    power_monitor::{PowerBackend, PowerState, SupplyInput},
};

pub(super) struct Panels {
    pub(super) recipes: super::recipes_view::RecipeUi,
    config: config_views::ConfigUi,
    pub(super) generated: bool,
    pub(super) comparison_rows: Vec<(String, String, u32, u32)>,
    pub profiles: Manager,
    fields: Vec<(Field, &'static str, Entity<InputState>)>,
    pub hardware: Option<Client>,
    pub hardware_polling: bool,
    pub(super) live: super::live::Live,
    pub hardware_next: Instant,
    hardware_received: Option<Instant>,
    hardware_config: Option<plc_connection::Config>,
    pub download_armed: bool,
    pub control_armed: bool,
    pub(super) download_password: Entity<InputState>,
    pub(super) download_plc_id: Entity<InputState>,
    pub memory_path: Option<PathBuf>,
    pub memory_recovery: bool,
    memory_identity: Entity<InputState>,
    memory_index: Entity<InputState>,
    memory_capacity: u16,
    memory_wide: bool,
    pub slot: u8,
    samples: [Entity<InputState>; 4],
    contact: Entity<InputState>,
    output: Entity<InputState>,
    watch: Entity<InputState>,
    word: Entity<InputState>,
    pub(super) watch_hex: bool,
    pub(super) trend_device: Entity<InputState>,
    pub(super) comment_device: Entity<InputState>,
    pub(super) comment_text: Entity<InputState>,
    pub(super) comment_filter: Entity<InputState>,
    pub(super) network_comment: Entity<TextareaState>,
    /// Name and pointer number of the selected program unit.
    pub(super) unit_name: Entity<InputState>,
    pub(super) unit_number: Entity<InputState>,
    /// Symbol table form and the allocation range inputs.
    pub(super) sym_name: Entity<InputState>,
    pub(super) sym_device: Entity<InputState>,
    pub(super) sym_comment: Entity<InputState>,
    pub(super) sym_init: Entity<InputState>,
    pub(super) sym_type: usize,
    /// Write the symbols' initial values after the program download.
    pub(super) write_initial: bool,
    /// Show symbol names instead of addresses (ladder, watch table, cross reference).
    pub(super) show_symbols: bool,
    /// The symbol form's "retained" switch and the retain range inputs.
    pub(super) sym_retain: bool,
    pub(super) retain_d_lo: Entity<InputState>,
    pub(super) retain_d_hi: Entity<InputState>,
    pub(super) retain_m_lo: Entity<InputState>,
    pub(super) retain_m_hi: Entity<InputState>,
    pub(super) alloc_d_lo: Entity<InputState>,
    pub(super) alloc_d_hi: Entity<InputState>,
    pub(super) alloc_m_lo: Entity<InputState>,
    pub(super) alloc_m_hi: Entity<InputState>,
    /// New-project screen: chosen model id, project name and language.
    pub(super) new_model: &'static str,
    pub(super) new_name: Entity<InputState>,
    pub(super) new_st: bool,
    /// Result of the last read-only connection test.
    pub(super) last_probe: Option<Result<opendvp::plc_download::Probe, String>>,
    /// Instruction library: selected command (palette item) or guide topic, and search.
    pub(super) library_item: Option<&'static crate::palette::Item>,
    pub(super) library_topic: Option<usize>,
    pub(super) library_query: Entity<InputState>,
    pub(super) st_lib_query: Entity<InputState>,
    pub(super) st_lib_selected: Option<String>,
    /// Device configuration: selected rack slot (`None` = CPU) and catalog search.
    pub(super) hw_selected: Option<usize>,
    pub(super) hw_query: Entity<InputState>,
    pub(super) hw_cr: Entity<InputState>,
    pub(super) hw_data: Entity<InputState>,
    pub(super) hw_count: Entity<InputState>,
    pub(super) hw_read: bool,
    /// Last module scan read from the PLC.
    pub(super) hw_scan: Option<Result<opendvp::plc_modules::ModuleScan, String>>,
    /// Cross-reference: search and selected device.
    pub(super) xref_query: Entity<InputState>,
    pub(super) xref_device: Option<Device>,
    /// Watch & force table: selected row, value to write, new-row address, number format.
    pub(super) watch_selected: Option<Device>,
    pub(super) watch_value: Entity<InputState>,
    pub(super) watch_new: Entity<InputState>,
    pub(super) watch_format: BTreeMap<Device, u8>,
    pub trend: VecDeque<(f32, u64)>,
}
impl Panels {
    fn placeholder_input(
        window: &mut Window,
        cx: &mut Context<Workbench>,
        placeholder: &'static str,
    ) -> Entity<InputState> {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify()).detach();
        input
    }
    fn input(
        window: &mut Window,
        cx: &mut Context<Workbench>,
        value: impl Into<SharedString>,
    ) -> Entity<InputState> {
        let value = value.into();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
        let subscription = cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify());
        subscription.detach();
        input
    }
    pub fn new(window: &mut Window, cx: &mut Context<Workbench>) -> Self {
        let mut profiles = Manager::default();
        if let Some(path) = Self::profile_path()
            && path.exists()
            && let Err(e) = profiles.load_from(&path)
        {
            profiles.feedback = Some(e);
        }
        let p = &profiles.profiles[profiles.selected];
        let fields = [
            (Field::Name, tr("Profil adı"), p.name.clone()),
            (Field::Port, tr("COM portu"), p.port.clone()),
            (Field::Baud, "Baud", p.baud.clone()),
            (Field::DataBits, tr("Veri biti"), p.data_bits.clone()),
            (
                Field::Parity,
                tr("Parite (None / Even / Odd)"),
                p.parity.clone(),
            ),
            (Field::StopBits, tr("Stop biti"), p.stop_bits.clone()),
            (
                Field::Protocol,
                tr("Protokol (Ascii / Rtu / Tcp)"),
                p.protocol.clone(),
            ),
            (Field::Host, tr("IP adresi"), p.host.clone()),
            (Field::TcpPort, tr("TCP portu"), p.tcp_port.clone()),
            (Field::Station, tr("İstasyon"), p.station.clone()),
            (Field::Timeout, tr("Zaman aşımı · ms"), p.timeout.clone()),
            (Field::Poll, tr("Okuma aralığı · ms"), p.poll.clone()),
            (Field::XStart, tr("İlk X (ondalık indeks)"), p.x_start.clone()),
            (Field::XCount, tr("X sayısı"), p.x_count.clone()),
            (Field::YStart, tr("İlk Y (ondalık indeks)"), p.y_start.clone()),
            (Field::YCount, tr("Y sayısı"), p.y_count.clone()),
        ]
        .into_iter()
        .map(|(field, label, value)| (field, label, Self::input(window, cx, value)))
        .collect();
        let trend_device = Self::input(window, cx, "D0");
        cx.subscribe(&trend_device, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.extra.trend.clear();
                cx.notify();
            }
        })
        .detach();
        let config = config_views::ConfigUi::new(window, cx);
        Self {
            recipes: super::recipes_view::RecipeUi::new(window, cx),
            config,
            generated: false,
            comparison_rows: Vec::new(),
            profiles,
            fields,
            hardware: None,
            hardware_polling: false,
            live: Default::default(),
            hardware_next: Instant::now(),
            hardware_received: None,
            hardware_config: None,
            download_armed: false,
            control_armed: false,
            download_password: cx.new(|cx| InputState::new(window, cx).masked(true)),
            download_plc_id: Self::input(window, cx, ""),
            memory_path: None,
            memory_recovery: false,
            memory_identity: Self::input(window, cx, "DVP-ES2-LOCAL"),
            memory_index: Self::input(window, cx, "0"),
            memory_capacity: 8000,
            memory_wide: false,
            slot: 0,
            samples: std::array::from_fn(|_| Self::input(window, cx, "0")),
            contact: Self::input(window, cx, "X0"),
            output: Self::input(window, cx, "Y0"),
            watch: Self::input(window, cx, "D0"),
            word: Self::input(window, cx, "0"),
            watch_hex: false,
            trend_device,
            comment_device: Self::input(window, cx, "X0"),
            comment_text: cx
                .new(|cx| InputState::new(window, cx).placeholder(tr("Örn. Başlat butonu"))),
            comment_filter: {
                let input = cx
                    .new(|cx| InputState::new(window, cx).placeholder(tr("Adres veya açıklama ara…")));
                cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify())
                    .detach();
                input
            },
            network_comment: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(2, 6)
                    .placeholder(tr("İlk satır ağ başlığı, sonraki satırlar açıklama"))
            }),
            new_model: opendvp::devices::find(&opendvp::settings::get().new_model)
                .map_or(opendvp::devices::DEFAULT, |m| m.id),
            unit_name: Self::input(window, cx, ""),
            unit_number: Self::input(window, cx, ""),
            sym_name: Self::placeholder_input(window, cx, tr("Ad")),
            sym_device: Self::placeholder_input(window, cx, tr("otomatik")),
            sym_comment: Self::placeholder_input(window, cx, tr("Açıklama")),
            sym_init: Self::placeholder_input(window, cx, tr("Başlangıç")),
            sym_type: 0,
            write_initial: true,
            show_symbols: true,
            sym_retain: false,
            retain_d_lo: Self::input(window, cx, ""),
            retain_d_hi: Self::input(window, cx, ""),
            retain_m_lo: Self::input(window, cx, ""),
            retain_m_hi: Self::input(window, cx, ""),
            alloc_d_lo: Self::input(window, cx, ""),
            alloc_d_hi: Self::input(window, cx, ""),
            alloc_m_lo: Self::input(window, cx, ""),
            alloc_m_hi: Self::input(window, cx, ""),
            new_name: Self::input(window, cx, tr("Yeni proje")),
            new_st: opendvp::settings::get().new_st,
            last_probe: None,
            library_item: crate::palette::ITEMS.first(),
            library_topic: None,
            hw_selected: None,
            hw_cr: Self::input(window, cx, "0"),
            hw_data: Self::input(window, cx, "D100"),
            hw_count: Self::input(window, cx, "1"),
            hw_read: true,
            hw_scan: None,
            xref_device: None,
            watch_selected: None,
            watch_value: Self::input(window, cx, "1"),
            watch_new: Self::input(window, cx, "D0"),
            watch_format: BTreeMap::new(),
            xref_query: {
                let input = cx
                    .new(|cx| InputState::new(window, cx).placeholder(tr("Adres veya açıklama ara…")));
                cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify())
                    .detach();
                input
            },
            hw_query: {
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Modül ara…")));
                cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify())
                    .detach();
                input
            },
            st_lib_query: {
                let input = cx.new(|cx| {
                    InputState::new(window, cx).placeholder(tr("Blok, açıklama veya parametre ara…"))
                });
                cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify())
                    .detach();
                input
            },
            st_lib_selected: None,
            library_query: {
                let input = cx.new(|cx| {
                    InputState::new(window, cx).placeholder(tr("Komut, açıklama veya konu ara…"))
                });
                cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify())
                    .detach();
                input
            },
            trend: VecDeque::new(),
        }
    }
    fn profile_path() -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("OpenDVP/connections.json"))
    }
}
impl Workbench {
    pub(super) fn unsaved(&self) -> bool {
        self.dirty
            || self
                .simulator
                .memory
                .file_memory
                .store()
                .is_some_and(|s| s.dirty())
    }
    pub(super) fn sync_network_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_sync = false;
        if let Some(network) = self
            .view_program()
            .and_then(|p| p.networks.get(self.selected))
            && let Some(device) = network.actions.iter().find_map(|a| {
                if let opendvp::plc::Action::Out(d) = a {
                    Some(*d)
                } else {
                    None
                }
            })
        {
            self.extra.output.update(cx, |input, cx| {
                input.set_value(device.to_string(), window, cx)
            });
        }
        let comment = self
            .view_program()
            .and_then(|p| p.networks.get(self.selected))
            .map(|n| n.comment.clone())
            .unwrap_or_default();
        self.extra
            .network_comment
            .update(cx, |input, cx| input.set_value(comment, window, cx));
    }
    pub(super) fn local_editable(&self) -> bool {
        !self.busy && !self.running && self.bridge.is_none() && self.extra.hardware.is_none()
    }
    pub(super) fn topology_editable(&self) -> bool {
        self.local_editable() && !self.local_running
    }
    pub(super) fn hardware_stale(&self) -> bool {
        self.extra.hardware_received.is_none_or(|t| {
            t.elapsed()
                > Duration::from_millis(
                    self.extra
                        .hardware_config
                        .as_ref()
                        .map_or(1500, |c| c.poll_ms.saturating_mul(3).max(1500)),
                )
        })
    }
    pub(super) fn disconnect_hardware(&mut self) {
        if let Some(hardware) = self.extra.hardware.take() {
            hardware.disconnect();
        }
        self.extra.hardware_polling = false;
        self.extra.hardware_received = None;
        self.values.clear();
    }
    pub(super) fn adopt_hardware(&mut self, sample: Sample) {
        self.values = sample
            .values
            .iter()
            .map(|(d, v)| ((*d, "bit".into()), u32::from(*v)))
            .collect();
        self.extra.hardware_received = Some(Instant::now());
        self.extra.hardware_next = Instant::now()
            + Duration::from_millis(
                self.extra
                    .hardware_config
                    .as_ref()
                    .map_or(500, |c| c.poll_ms),
            );
        self.poll_ms = Some(u128::from(sample.latency_ms));
        self.history.push_back((
            sample.sequence,
            sample
                .values
                .iter()
                .map(|(d, v)| (d.to_string(), u32::from(*v)))
                .collect(),
        ));
        if self.history.len() > 120 {
            self.history.pop_front();
        }
    }
    pub(super) fn capture_trend(&mut self, cx: &App) {
        if let Ok(device) = self.extra.trend_device.read(cx).value().parse::<Device>() {
            let value = if self.bridge.is_none() && self.extra.hardware.is_none() {
                Some(
                    if device.bit() && !matches!(device.area, Area::T | Area::C) {
                        u8::from(self.simulator.memory.bit(device)) as f32
                    } else {
                        f32::from(self.simulator.memory.word(device))
                    },
                )
            } else if self.hardware_stale() && self.extra.hardware.is_some() {
                None
            } else {
                let kind = if matches!(device.area, Area::D | Area::T | Area::C) {
                    "word"
                } else {
                    "bit"
                };
                self.values.get(&(device, kind.into())).map(|v| {
                    if kind == "word" {
                        (*v as u16 as i16) as f32
                    } else {
                        *v as f32
                    }
                })
            };
            if let Some(value) = value {
                let seq = self.extra.trend.back().map_or(0, |(_, s)| s + 1);
                self.extra.trend.push_back((value, seq));
                if self.extra.trend.len() > 240 {
                    self.extra.trend.pop_front();
                }
            }
        }
    }
    pub(super) fn sync_profile(&mut self, cx: &mut Context<Self>) {
        if self.extra.profiles.active_kind() == Kind::Simulator {
            self.extra
                .profiles
                .set_simulator_path(self.simulator_path.read(cx).value().to_string());
        }
        for (field, _, input) in &self.extra.fields {
            self.extra.profiles.update(
                Event::Field(*field, input.read(cx).value().to_string()),
                true,
            );
        }
    }
    fn select_profile(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        if !self.topology_editable() {
            return;
        }
        self.sync_profile(cx);
        self.extra.profiles.update(event, true);
        let p = self.extra.profiles.profiles[self.extra.profiles.selected].clone();
        if p.kind == Kind::Simulator {
            self.simulator_path.update(cx, |input, cx| {
                input.set_value(p.simulator_path.clone(), window, cx)
            });
        }
        for (field, _, input) in &self.extra.fields {
            let mut copy = p.clone();
            let value = copy.field_mut(*field).clone();
            input.update(cx, |state, cx| state.set_value(value, window, cx));
        }
        cx.notify();
    }
    fn connect_physical(&mut self, cx: &mut Context<Self>) {
        if !self.topology_editable() {
            return;
        }
        self.sync_profile(cx);
        match self.extra.profiles.active_config() {
            Ok(config) => {
                self.extra.hardware_config = Some(config.clone());
                self.snapshot = None;
                self.log(tr("PLC’ye bağlanılıyor · yalnız okuma…"));
                self.dispatch(
                    async move { Done::Hardware(plc_connection::connect(config).await) },
                    cx,
                );
            }
            Err(e) => {
                self.log(e);
                cx.notify();
            }
        }
    }
    fn download_physical(&mut self, run_after: bool, cx: &mut Context<Self>) {
        self.extra.download_armed = false;
        if self.plc_write_blocked(cx) {
            return;
        }
        if !self.topology_editable() {
            return;
        }
        let Some(program) = self.program.as_ref() else {
            self.log(tr("Program derlenmeden PLC’ye yüklenemez."));
            return cx.notify();
        };
        let words = match opendvp::es2_encoder::encode(program) {
            Ok(encoded) => encoded.words,
            Err(e) => {
                self.log(tfmt!("Program PLC biçimine çevrilemedi: {e}", "The program could not be converted to PLC format: {e}"));
                return cx.notify();
            }
        };
        let optional = |input: &Entity<InputState>, cx: &Context<Self>| {
            let value = input.read(cx).value().trim().to_string();
            (!value.is_empty()).then_some(value)
        };
        // Initial values of the symbols go in after the program, while the PLC is still in STOP.
        let initial = if self.extra.write_initial {
            match self
                .project
                .initial_values()
                .and_then(|v| v.iter().map(opendvp::project::InitialValue::to_modbus).collect())
            {
                Ok(initial) => initial,
                Err(e) => {
                    self.log(e);
                    return cx.notify();
                }
            }
        } else {
            Vec::new()
        };
        let password = optional(&self.extra.download_password, cx);
        let plc_id = optional(&self.extra.download_plc_id, cx);
        if let Some(Err(e)) = password
            .as_deref()
            .map(opendvp::plc_download::validate_password)
        {
            self.log(e);
            return cx.notify();
        }
        self.sync_profile(cx);
        match self.extra.profiles.active_config() {
            Ok(config) => {
                self.log(tfmt!(
                    "PLC’ye program yükleniyor · {} adım · STOP…", "Downloading the program to the PLC · {} steps · STOP…",
                    words.len()
                ));
                self.dispatch(
                    async move {
                        Done::Download(
                            plc_connection::download_program(
                                config, words, run_after, password, plc_id, initial,
                            )
                            .await,
                        )
                    },
                    cx,
                );
            }
            Err(e) => {
                self.log(e);
                cx.notify();
            }
        }
    }
    /// Read-only connection test with the active profile.
    pub(super) fn probe_physical(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.extra.hardware.is_some() {
            return;
        }
        self.sync_profile(cx);
        match self.extra.profiles.active_config() {
            Ok(config) => {
                self.log(tr("Bağlantı test ediliyor (yalnız okuma)…"));
                self.dispatch(
                    async move { Done::Probe(plc_connection::probe_plc(config).await) },
                    cx,
                );
            }
            Err(e) => {
                // Profile errors (no COM port, bad IP) belong next to the test button.
                self.extra.last_probe = Some(Err(e.clone()));
                self.log(e);
                cx.notify();
            }
        }
    }
    /// Read-only PLC status (RUN, error, scan time, clock) with the active profile.
    pub(super) fn status_physical(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.extra.hardware.is_some() {
            return;
        }
        self.sync_profile(cx);
        match self.extra.profiles.active_config() {
            Ok(config) => {
                self.log(tr("PLC durumu okunuyor (yalnız okuma)…"));
                self.dispatch(
                    async move {
                        Done::Control(plc_connection::plc_status(config).await.map(|s| {
                            let rtc = s.rtc.map_or(tr("saat yok").to_string(), |r| {
                                format!(
                                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                                    r.year, r.month, r.day, r.hour, r.minute, r.second
                                )
                            });
                            tfmt!(
                                "PLC {} · hata {} (D1004={}, adım {}) · tarama {:.1}/{:.1}/{:.1} ms (şimdi/min/maks) · {rtc}", "PLC {} · error {} (D1004={}, step {}) · scan {:.1}/{:.1}/{:.1} ms (now/min/max) · {rtc}",
                                if s.running { "RUN" } else { "STOP" },
                                if s.error { tr("VAR") } else { "yok" },
                                s.error_code,
                                s.error_step,
                                f64::from(s.scan_current) / 10.0,
                                f64::from(s.scan_min) / 10.0,
                                f64::from(s.scan_max) / 10.0,
                            )
                        }))
                    },
                    cx,
                );
            }
            Err(e) => {
                self.log(e);
                cx.notify();
            }
        }
    }
    /// Switch the real PLC to RUN/STOP (dedicated connection; user confirmed in the UI).
    pub(super) fn run_stop_physical(&mut self, run: bool, cx: &mut Context<Self>) {
        self.extra.control_armed = false;
        if self.plc_write_blocked(cx) {
            return;
        }
        if self.busy || self.extra.hardware.is_some() {
            return;
        }
        self.sync_profile(cx);
        match self.extra.profiles.active_config() {
            Ok(config) => {
                self.log(if run { tr("PLC RUN yapılıyor…") } else { tr("PLC STOP yapılıyor…") });
                self.dispatch(
                    async move {
                        Done::Control(plc_connection::plc_set_run(config, run).await.map(|()| {
                            if run { tr("PLC RUN komutu gönderildi.") } else { tr("PLC STOP komutu gönderildi.") }
                                .to_string()
                        }))
                    },
                    cx,
                );
            }
            Err(e) => {
                self.log(e);
                cx.notify();
            }
        }
    }
    /// Read the PLC's extension modules with the active profile (read only).
    pub(super) fn scan_hardware(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.extra.hardware.is_some() {
            return;
        }
        self.sync_profile(cx);
        match self.extra.profiles.active_config() {
            Ok(config) => {
                self.log(tr("PLC'deki modüller okunuyor (yalnız okuma)…"));
                self.dispatch(
                    async move { Done::ModuleScan(plc_connection::scan_modules(config).await) },
                    cx,
                );
            }
            Err(e) => {
                self.extra.hw_scan = Some(Err(e.clone()));
                self.log(e);
                cx.notify();
            }
        }
    }
    /// Card with the last connection test result, compared with the project model.
    pub(super) fn probe_card(&self) -> AnyElement {
        let Some(result) = &self.extra.last_probe else {
            return div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(tr("Bağlantıyı test et: PLC'ye yazmadan model, kapasite ve durum okunur."))
                .into_any_element();
        };
        let probe = match result {
            Ok(probe) => probe,
            Err(e) => {
                return div()
                    .p_3()
                    .rounded(px(6.))
                    .bg(rgb(0xfdecea))
                    .text_color(rgb(0xb42318))
                    .child(tfmt!("Bağlantı testi başarısız: {e}", "Connection test failed: {e}"))
                    .into_any_element();
            }
        };
        let row = |label: &'static str, value: String| {
            div()
                .flex()
                .py_1()
                .child(div().w(px(160.)).text_color(rgb(MUTED)).child(label))
                .child(div().flex_1().child(value))
        };
        let flag = |v: Option<bool>, yes: &str, no: &str| match v {
            Some(true) => yes.to_string(),
            Some(false) => no.to_string(),
            None => tr("bilinmiyor (sorgu yanıtlanmadı)").to_string(),
        };
        let found = probe.identity.model;
        let mismatch = found.is_some_and(|m| !m.eq_ignore_ascii_case(&self.project.device));
        let mut card = div()
            .p_3()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(if mismatch { 0xf59e0b } else { 0x9fd3d8 }))
            .bg(rgb(if mismatch { 0xfff7e6 } else { 0xeef8f9 }))
            .flex()
            .flex_col()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .mb_1()
                    .child(tr("Bağlantı testi")),
            )
            .child(row(
                "Model",
                match (found, probe.identity.info) {
                    (Some(m), Some(info)) => tfmt!("DVP-{m} (FC67 kodu {:#04x})", "DVP-{m} (FC67 code {:#04x})", info[3]),
                    (None, Some(info)) => match opendvp::plc_download::simulator_name(&info) {
                        Some(sim) => tfmt!("{sim} simülatörü (FC67 kodu {:#04x})", "{sim} simulator (FC67 code {:#04x})", info[3]),
                        None => tfmt!("tanınmadı (FC67 kodu {:#04x})", "not recognized (FC67 code {:#04x})", info[3]),
                    },
                    _ => tr("FC67 yanıtlanmadı").into(),
                },
            ))
            .child(row(
                tr("Program kapasitesi"),
                tfmt!("{} adım", "{} steps", probe.identity.capacity_steps),
            ))
            .child(row("CPU", flag(probe.running, "RUN", "STOP")))
            .child(row(tr("Parola"), flag(probe.password_set, tr("tanımlı"), "yok")))
            .child(row(tr("PLC kimliği"), flag(probe.plc_id_set, tr("tanımlı"), "yok")));
        if mismatch {
            card = card.child(div().mt_1().text_color(rgb(0xb45309)).child(tfmt!(
                "Proje modeli {}, bağlı PLC {}. Program yüklemeden önce proje modelini kontrol edin.", "Project model {}, connected PLC {}. Check the project model before downloading the program.",
                self.project.device,
                found.unwrap_or("?")
            )));
        }
        card.into_any_element()
    }
    fn refresh_ports(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            self.dispatch(
                async { Done::Ports(plc_connection::available_ports().await) },
                cx,
            );
        }
    }
    pub(super) fn commit_source(
        &mut self,
        source: String,
        language: SourceLanguage,
        cx: &mut Context<Self>,
    ) {
        self.undo
            .push((self.project.program_language(self.active_program), self.active_source().to_owned()));
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
        let index = self.active_program;
        self.project.set_program_source(index, source);
        // Only Main's language is the project's; a unit keeps its own.
        if index == 0 {
            self.project.language = language;
        }
        self.editor_sync = true;
        self.dirty = true;
        self.uploaded = false;
        self.read_preview = None;
        if let Some(bridge) = self.bridge.take() {
            bridge.disconnect();
        }
        self.snapshot = None;
        self.values.clear();
        self.compile();
        cx.notify();
    }
    pub(super) fn graph_edit(&mut self, kind: u8, cx: &mut Context<Self>) {
        if !self.topology_editable() {
            return;
        }
        let op = match kind {
            0..=2 => match self.extra.contact.read(cx).value().parse::<Device>() {
                Ok(d) => match kind {
                    0 => crate::gpui_model::Edit::SeriesNo(d),
                    1 => crate::gpui_model::Edit::SeriesNc(d),
                    _ => crate::gpui_model::Edit::Parallel(d),
                },
                Err(e) => {
                    self.log(e);
                    cx.notify();
                    return;
                }
            },
            3 => match self.extra.output.read(cx).value().parse::<Device>() {
                Ok(d) => crate::gpui_model::Edit::Output(d),
                Err(e) => {
                    self.log(e);
                    cx.notify();
                    return;
                }
            },
            4 => crate::gpui_model::Edit::Add,
            _ => crate::gpui_model::Edit::Remove,
        };
        let compiler = self.model_compiler();
        let edited = crate::gpui_model::scoped(compiler, || {
            crate::gpui_model::edit(
                self.active_source(),
                self.project.program_language(self.active_program),
                self.selected,
                op,
            )
        });
        match edited {
            Ok(source) => {
                self.commit_source(source, SourceLanguage::Il, cx);
                if kind == 4 {
                    self.selected = self
                        .view_program()
                        .map_or(0, |p| p.networks.len().saturating_sub(1));
                } else {
                    self.selected = self.selected.min(
                        self.view_program()
                            .map_or(0, |p| p.networks.len().saturating_sub(1)),
                    );
                }
                self.selected_sync = true;
                self.ladder_scroll.scroll_to_item(self.selected);
                self.log(tr("Ladder değişikliği derlendi. Geri al kullanılabilir."));
            }
            Err(e) => {
                self.log(e);
                cx.notify();
            }
        }
    }
    pub(super) fn source_undo(&mut self, redo: bool, cx: &mut Context<Self>) {
        if !self.topology_editable() {
            return;
        }
        let next = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some((language, source)) = next {
            let old = (self.project.program_language(self.active_program), self.active_source().to_owned());
            if redo {
                self.undo.push(old);
            } else {
                self.redo.push(old);
            }
            let index = self.active_program;
            self.project.set_program_source(index, source);
            if index == 0 {
                self.project.language = language;
            }
            self.dirty = true;
            self.editor_sync = true;
            self.compile();
            self.selected_sync = true;
            self.ladder_scroll.scroll_to_item(self.selected);
            cx.notify();
        }
    }
    fn memory_profile(&self) -> Result<FileMemoryProfile, String> {
        let p = FileMemoryProfile::new(
            FileModel::Es2,
            FirmwareVersion::new(
                3,
                if self.extra.memory_capacity == 5000 {
                    44
                } else {
                    46
                },
            ),
            false,
        )?;
        Ok(if self.extra.memory_wide {
            p.with_installed_wide_extension()
        } else {
            p
        })
    }
    fn can_replace_memory(&mut self) -> bool {
        if self
            .simulator
            .memory
            .file_memory
            .store()
            .is_some_and(|s| s.dirty() && (s.generation() != 0 || s.write_transactions() != 0))
        {
            self.log(tr("Dosya belleğinde kaydedilmemiş veriler var. Önce belleği kaydedin."));
            false
        } else {
            true
        }
    }
    fn memory_new(&mut self, cx: &mut Context<Self>) {
        if !self.topology_editable() || !self.can_replace_memory() {
            cx.notify();
            return;
        }
        match self.memory_profile().and_then(|p| {
            FileStore::new(self.extra.memory_identity.read(cx).value().to_string(), p)
        }) {
            Ok(store) => {
                self.simulator.memory.file_memory.configure(store);
                self.extra.memory_path = None;
                self.extra.memory_recovery = false;
                self.log(tr("Silinmiş dosya belleği hazır · FFFF / -1"));
            }
            Err(e) => self.log(e),
        }
        cx.notify();
    }
    fn memory_open(&mut self, cx: &mut Context<Self>) {
        if !self.topology_editable() || !self.can_replace_memory() {
            cx.notify();
            return;
        }
        let profile = match self.memory_profile() {
            Ok(p) => p,
            Err(e) => {
                self.log(e);
                return;
            }
        };
        let identity = self.extra.memory_identity.read(cx).value().to_string();
        let task = dialog(move || {
            rfd::FileDialog::new()
                .add_filter(tr("PLC dosya belleği"), &["odmem"])
                .pick_file()
                .map(|path| {
                    FileStore::load_with_recovery(&path, &identity, &profile)
                        .map(|load| (load, path))
                        .map_err(|e| e.to_string())
                })
        });
        self.dispatch(async move { Done::MemoryLoaded(task.await) }, cx);
    }
    pub(super) fn memory_save(&mut self, as_new: bool, cx: &mut Context<Self>) {
        if !self.local_editable() {
            return;
        }
        self.local_running = false;
        let Some(mut store) = self.simulator.memory.file_memory.store().cloned() else {
            self.log(tr("Önce dosya belleği oluşturun veya yükleyin."));
            cx.notify();
            return;
        };
        let path = if as_new {
            None
        } else {
            self.extra.memory_path.clone()
        };
        let task = dialog(move || {
            path.or_else(|| {
                rfd::FileDialog::new()
                    .add_filter(tr("PLC dosya belleği"), &["odmem"])
                    .set_file_name("plc-memory.odmem")
                    .save_file()
            })
            .map(|path| {
                store
                    .save(&path)
                    .map(|_| (store, path))
                    .map_err(|e| e.to_string())
            })
        });
        self.dispatch(async move { Done::MemorySaved(task.await) }, cx);
    }
    fn memory_recover(&mut self, cx: &mut Context<Self>) {
        if !self.topology_editable() {
            return;
        }
        if let (Some(path), Some(store)) = (
            &self.extra.memory_path,
            self.simulator.memory.file_memory.store(),
        ) {
            match FileStore::recover_previous(path, store.identity(), store.profile()) {
                Ok(_) => {
                    self.extra.memory_recovery = false;
                    self.log(tr("Önceki doğrulanmış disk görüntüsü onarıldı; RAM’deki dosya verileri korundu."));
                }
                Err(e) => self.log(e.to_string()),
            }
        }
        cx.notify();
    }
    fn module_action(&mut self, kind: u8, cx: &mut Context<Self>) {
        if !self.local_editable() || (kind != 3 && self.local_running) {
            return;
        }
        let slot = self.extra.slot;
        let now = self.simulator.memory.timing_io.now_us;
        let result = match kind {
            0 | 1 => {
                let model = if kind == 0 {
                    ModuleCpu::Es2
                } else {
                    ModuleCpu::Ss2
                };
                ModuleCpuProfile::new(
                    model,
                    if kind == 0 { 3 } else { 2 },
                    if kind == 0 { 46 } else { 80 },
                )
                .and_then(|p| self.simulator.memory.module_io.set_cpu(p))
            }
            2 => {
                let bus = &mut self.simulator.memory.module_io;
                if bus.read_slot(slot).is_some() {
                    Err(tr("Seçili slot dolu.").into())
                } else {
                    let module = if bus.cpu().model == ModuleCpu::Es2 {
                        VirtualModule::ad_e2(0x0110)
                    } else {
                        VirtualModule::ad_s2(0x0500)
                    };
                    module.and_then(|m| bus.configure(slot, m))
                }
            }
            3 => {
                let samples = self
                    .extra
                    .samples
                    .iter()
                    .map(|input| {
                        input.read(cx).value().trim().parse::<i16>().map_err(|_| {
                            tr("Dört kanal için -32768…32767 aralığında ham kod girin.").to_string()
                        })
                    })
                    .collect::<Result<Vec<_>, _>>();
                samples.and_then(|v| {
                    self.simulator
                        .memory
                        .module_io
                        .set_samples(slot, [v[0], v[1], v[2], v[3]])
                })
            }
            4 => self.simulator.memory.module_io.power_cycle(slot, now),
            _ => self
                .simulator
                .memory
                .module_io
                .remove(slot)
                .map(|_| ())
                .ok_or(tr("Slot boş.").into()),
        };
        self.log(
            result
                .map(|_| tr("Sanal modül ayarları uygulandı.").to_string())
                .unwrap_or_else(|e| e),
        );
        cx.notify();
    }
    fn form(label: &str, input: &Entity<InputState>, disabled: bool) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .min_w(px(165.))
            .w_full()
            .child(Self::heading(label.to_owned()))
            .child(Input::new(input).disabled(disabled))
    }
    pub(super) fn inspector_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let editable = self.topology_editable()
            && self.project.program_language(self.active_program) == SourceLanguage::Il
            && self
                .view_program()
                .is_some_and(|p| p.graphically_editable());
        let mut body = div()
            .id("inspector-scroll")
            .w_full()
            .flex_shrink_0()
            .overflow_y_scroll()
            .border_l_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .p_4()
            .flex()
            .flex_col()
            .gap_3();
        if self.tab == Tab::Ladder && !self.ui.watch_mode {
            body = body.child(self.inspector_tabs(cx));
        }
        if self.tab == Tab::Ladder && !self.ui.watch_mode {
            body = body
                .child(Self::heading(tfmt!(
                    "AĞ {:02} · DÜZENLE", "NETWORK {:02} · EDIT",
                    self.selected + 1
                )))
                .child(Self::form(tr("Kontak adresi"), &self.extra.contact, !editable))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            self.button("series-no", tr("Seri NO"), !editable)
                                .on_click(cx.listener(|this, _, _, cx| this.graph_edit(0, cx))),
                        )
                        .child(
                            self.button("series-nc", tr("Seri NC"), !editable)
                                .on_click(cx.listener(|this, _, _, cx| this.graph_edit(1, cx))),
                        ),
                )
                .child(
                    self.button("parallel", tr("Paralel dal ekle"), !editable)
                        .on_click(cx.listener(|this, _, _, cx| this.graph_edit(2, cx))),
                )
                .child(Self::form(tr("Çıkış adresi"), &self.extra.output, !editable))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            self.button("output-apply", tr("OUT uygula"), !editable)
                                .on_click(cx.listener(|this, _, _, cx| this.graph_edit(3, cx))),
                        )
                        .child(
                            self.button("network-remove", tr("Ağı kaldır"), !editable)
                                .on_click(cx.listener(|this, _, _, cx| this.graph_edit(5, cx))),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            self.button("graph-undo", tr("Geri al"), !editable || self.undo.is_empty())
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.source_undo(false, cx)),
                                ),
                        )
                        .child(
                            self.button("graph-redo", tr("Yinele"), !editable || self.redo.is_empty())
                                .on_click(cx.listener(|this, _, _, cx| this.source_undo(true, cx))),
                        ),
                )
                .child(
                    self.button("network-add", tr("+ Yeni ağ"), !editable)
                        .on_click(cx.listener(|this, _, _, cx| this.graph_edit(4, cx))),
                )
                .child(Self::heading(
                    if self.project.program_language(self.active_program) == SourceLanguage::St {
                        tr("ST kaynağı grafik olarak salt okunur.")
                    } else {
                        tr("Grafik değişiklikleri derlenir; RAM sıfırlanır.")
                    },
                ));
        }
        if self.tab == Tab::Ladder && !self.ui.watch_mode {
            return body.child(div().mt_4().pt_4().border_t_1().border_color(rgb(LINE)).text_xs().text_color(rgb(MUTED)).child(tr("Kontak eklemek veya bobini değiştirmek için ağ numarasını seçin. Cihaz değerleri İzleme sekmesindedir."))).into_any_element();
        }
        body = body
            .child(
                div()
                    .pt_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(Self::heading(tr("CİHAZ İZLEME"))),
            )
            .child(Self::form(tr("İzleme adresi"), &self.extra.watch, false))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        self.button("watch-add", tr("Ekle"), false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                match this.extra.watch.read(cx).value().parse::<Device>() {
                                    Ok(d) => {
                                        let name = d.to_string();
                                        if !this.project.watch.contains(&name) {
                                            this.project.watch.push(name);
                                            this.dirty = true;
                                        }
                                    }
                                    Err(e) => this.log(e),
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        self.button(
                            "watch-hex",
                            if self.extra.watch_hex { "HEX" } else { "DEC" },
                            false,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.extra.watch_hex = !this.extra.watch_hex;
                            cx.notify();
                        })),
                    ),
            );
        body = body.child(self.register_table());
        body = body
            .child(Self::form(
                tr("Yerel WORD değeri"),
                &self.extra.word,
                !self.local_editable(),
            ))
            .child(
                self.button(
                    "write-local-word",
                    tr("Adrese yaz (yerel)"),
                    !self.local_editable(),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    let result = (|| {
                        let d = this.extra.watch.read(cx).value().parse::<Device>()?;
                        if !matches!(d.area, Area::D | Area::T | Area::C) {
                            return Err(tr("WORD adresi D/T/C olmalı.").to_string());
                        }
                        let v = this
                            .extra
                            .word
                            .read(cx)
                            .value()
                            .parse::<i16>()
                            .map_err(|_| tr("16 bit işaretli değer girin.").to_string())?;
                        this.simulator.memory.set_word(d, v);
                        Ok(())
                    })();
                    if let Err(e) = result {
                        this.log(e);
                    }
                    cx.notify();
                })),
            );
        body.into_any_element()
    }

    /// Watch table in the style of PLC IDEs: address, comment, type, monitor value, modify.
    pub(super) fn io_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut devices = std::collections::BTreeSet::new();
        for index in 0..16 {
            devices.insert(Device {
                area: Area::X,
                index,
            });
            devices.insert(Device {
                area: Area::Y,
                index,
            });
        }
        if let Some(p) = &self.program {
            devices.extend(
                p.devices
                    .iter()
                    .filter(|d| matches!(d.area, Area::X | Area::Y))
                    .copied(),
            );
        }
        // Points of the configured extension modules, labelled with their module.
        let mut module_of = std::collections::BTreeMap::new();
        for (prefix, index, module) in opendvp::hardware::extension_devices(&self.project.hardware)
        {
            let device = Device {
                area: if prefix == 'X' { Area::X } else { Area::Y },
                index,
            };
            devices.insert(device);
            module_of.insert(device, module);
        }
        let inputs_locked = self.extra.hardware.is_some()
            || (self.bridge.is_some() && !self.running && (!self.uploaded || self.online_used));
        let cell = |w: f32| div().w(px(w)).flex_shrink_0().px_3();
        let header = div()
            .flex()
            .items_center()
            .h(px(30.))
            .bg(rgb(0xf3f5f8))
            .border_b_1()
            .border_color(rgb(LINE))
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(MUTED))
            .child(cell(90.).child(tr("Adres")))
            .child(div().flex_1().min_w_0().px_3().child(tr("Yorum")))
            .child(cell(70.).child(tr("Tip")))
            .child(cell(150.).child(tr("İzleme değeri")))
            .child(cell(130.).child(tr("Değiştir")));
        let mut table = div()
            .id("io-table")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .m_3()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .child(header);
        for area in [Area::X, Area::Y] {
            table = table.child(
                div()
                    .px_3()
                    .py_1()
                    .bg(rgb(0xfafbfc))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(ACCENT))
                    .child(if area == Area::X {
                        tr("Dijital girişler (X)")
                    } else {
                        tr("Dijital çıkışlar (Y)")
                    }),
            );
            for d in devices.iter().filter(|d| d.area == area) {
                let d = *d;
                let state = self.observed_bit(d);
                let (lamp, text, color) = match state {
                    Some(true) => (LIVE_LAMP, "TRUE", GREEN),
                    Some(false) => (0xc8d0da, "FALSE", MUTED),
                    None => (PANEL, "—", MUTED),
                };
                let used = self
                    .program
                    .as_ref()
                    .is_some_and(|p| p.devices.contains(&d));
                let mut row = div()
                    .flex()
                    .items_center()
                    .h(px(32.))
                    .border_b_1()
                    .border_color(rgb(0xedf0f4))
                    .when(state == Some(true), |r| r.bg(rgb(0xf0faf3)))
                    .child(
                        cell(90.)
                            .font_family("Consolas")
                            .font_weight(if used {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(rgb(if used { TEXT } else { MUTED }))
                            .child(d.to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_3()
                            .overflow_hidden()
                            .text_color(rgb(MUTED))
                            .child(match (self.project.comment(d), module_of.get(&d)) {
                                (Some(comment), _) => comment.to_owned(),
                                (None, Some(module)) => tfmt!("{module} (genişleme)", "{module} (expansion)"),
                                (None, None) => String::new(),
                            }),
                    )
                    .child(cell(70.).text_xs().text_color(rgb(MUTED)).child("BOOL"))
                    .child(
                        cell(150.)
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size(px(12.))
                                    .rounded_full()
                                    .bg(rgb(lamp))
                                    .border_1()
                                    .border_color(rgb(if state == Some(true) {
                                        GREEN
                                    } else {
                                        0xaab4c0
                                    })),
                            )
                            .child(
                                div()
                                    .font_family("Consolas")
                                    .text_xs()
                                    .text_color(rgb(color))
                                    .child(text),
                            ),
                    );
                row = row.child(cell(130.).child(if area == Area::X {
                    Button::new(SharedString::from(format!("io-{d}")))
                        .label(if state == Some(true) {
                            tr("FALSE yap")
                        } else {
                            tr("TRUE yap")
                        })
                        .xsmall()
                        .outline()
                        .disabled(inputs_locked)
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_input(d, cx)))
                        .into_any_element()
                } else {
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(tr("program sürer"))
                        .into_any_element()
                }));
                table = table.child(row);
            }
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .pt_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(tr("İzleme tablosu · Giriş / çıkış")),
                            )
                            .child(Self::heading(
                                tr("Fiziksel X/Y salt okunur. Simülatör girişleri yalnız sahip olunan oturumda değiştirilebilir."),
                            )),
                    )
                    .child(
                        self.button(
                            "physical-poll",
                            if self.extra.hardware_polling {
                                tr("Fiziksel okumayı duraklat")
                            } else {
                                tr("Fiziksel okumayı sürdür")
                            },
                            self.extra.hardware.is_none(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.extra.hardware_polling = !this.extra.hardware_polling;
                            cx.notify();
                        })),
                    ),
            )
            .child(table)
            .into_any_element()
    }
}

/// File dialog starting in the default project folder of the settings (when it exists).
pub(super) fn start_dir(dialog: rfd::FileDialog) -> rfd::FileDialog {
    match opendvp::settings::get().default_dir {
        Some(dir) if dir.is_dir() => dialog.set_directory(dir),
        _ => dialog,
    }
}

impl Workbench {
    /// Read-only mode of the settings: no action may write to a real PLC.
    pub(super) fn plc_write_blocked(&mut self, cx: &mut Context<Self>) -> bool {
        if !opendvp::settings::get().plc_read_only {
            return false;
        }
        self.log(tr("PLC salt okunur modda: yazma işlemleri kapalı (Ayarlar)."));
        cx.notify();
        true
    }
}
