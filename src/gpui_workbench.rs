//! Native GPUI shell. PLC operations remain on the existing owned worker.
use crate::device_search::{self, DeviceType};
use gpui::{prelude::*, *};
use gpui_component::{
    Disableable, Sizable,
    button::{Button, ButtonVariants},
    input::{Editor, EditorState, Input, InputEvent, InputState, Textarea, TextareaState},
};
use opendvp::{
    i18n::tr,
    tfmt,
    es2_bridge::{self, Bridge, Comparison, OnlineSnapshot},
    plc::{Area, Device, Program, Simulator},
    project::{Project, SourceLanguage, UnitKind},
};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    path::PathBuf,
    time::{Duration, Instant},
};
#[path = "gpui_comments.rs"]
mod comments;
#[path = "gpui_diff.rs"]
mod diff;
#[path = "gpui_ladder.rs"]
mod drawing;
#[path = "gpui_hardware.rs"]
mod hardware;
#[path = "gpui_ladder_editor.rs"]
mod interactive;
#[path = "gpui_library.rs"]
mod library;
#[path = "gpui_new_project.rs"]
mod new_project;
#[path = "gpui_panels.rs"]
mod panels;
#[path = "gpui_messages.rs"]
mod messages;
#[path = "gpui_programs.rs"]
mod programs;
#[path = "gpui_symbols.rs"]
mod symbols;
#[path = "gpui_shell.rs"]
mod shell;
#[path = "gpui_settings.rs"]
mod settings_page;
#[path = "gpui_watch.rs"]
mod watch;
#[path = "gpui_widgets.rs"]
mod widgets;
#[path = "gpui_xref.rs"]
mod xref_view;
#[path = "gpui_theme.rs"]
pub mod colors;
use colors::rgb;

// Engineering-tool palette: light work area, dark chrome, petrol accent; orange marks
// an online session and green/amber the CPU mode, as in common PLC IDEs.
const BG: u32 = 0xeceff3;
const PANEL: u32 = 0xfffffe; // off-white so dark mode can tell panels from white text
const LINE: u32 = 0xd6dce4;
const TEXT: u32 = 0x1d2733;
const MUTED: u32 = 0x5f6b7a;
const GREEN: u32 = 0x15803d;
const ACCENT: u32 = 0x00838f;
const AMBER: u32 = 0xc2410c;
const ERROR: u32 = 0xb42318;
const CHROME: u32 = 0x18222d;
const CHROME_TEXT: u32 = 0xe8eef4;
const CHROME_MUTED: u32 = 0x9fb0c2;
const ONLINE: u32 = 0xf08c1c;
const ONLINE_SOFT: u32 = 0xfff3e2;
actions!(
    workbench,
    [
        SaveProject,
        OpenProject,
        CompileProgram,
        StopProgram,
        ShowSearch,
        CloseTab,
        NextTab,
        PrevTab
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-s", SaveProject, None),
        KeyBinding::new("ctrl-o", OpenProject, None),
        KeyBinding::new("f7", CompileProgram, None),
        KeyBinding::new("shift-f5", StopProgram, None),
        KeyBinding::new("ctrl-shift-f", ShowSearch, None),
        KeyBinding::new("ctrl-w", CloseTab, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PrevTab, None),
    ]);
}
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    NewProject,
    Diff,
    Watch,
    CrossRef,
    Hardware,
    Ladder,
    Source,
    Comments,
    Symbols,
    Search,
    Connection,
    Trend,
    Help,
    Log,
    Modules,
    FileMemory,
    Io,
    Comparison,
    Settings,
}
impl Tab {
    /// Every page, in the order the "+" menu offers the closed ones.
    const ALL: [Tab; 19] = [
        Tab::Ladder,
        Tab::Source,
        Tab::Io,
        Tab::Trend,
        Tab::Watch,
        Tab::CrossRef,
        Tab::Hardware,
        Tab::Connection,
        Tab::Modules,
        Tab::FileMemory,
        Tab::Diff,
        Tab::Comparison,
        Tab::Comments,
        Tab::Symbols,
        Tab::Search,
        Tab::Help,
        Tab::Log,
        Tab::Settings,
        Tab::NewProject,
    ];
    /// Document tab title; also the page heading.
    fn title(self) -> &'static str {
        match self {
            Tab::Ladder => "Ladder",
            Tab::NewProject => tr("Yeni proje"),
            Tab::Hardware => tr("Cihaz yapılandırması"),
            Tab::CrossRef => tr("Çapraz referans"),
            Tab::Watch => tr("İzleme tablosu"),
            Tab::Diff => tr("Online / offline karşılaştırma"),
            Tab::Source => tr("Kaynak"),
            Tab::Comments => tr("Cihaz açıklamaları"),
            Tab::Symbols => tr("Semboller"),
            Tab::Search => tr("Programda ara"),
            Tab::Connection => tr("Bağlantılar"),
            Tab::Modules => tr("Genişleme modülleri"),
            Tab::FileMemory => tr("Dosya belleği"),
            Tab::Io => tr("Giriş / çıkış"),
            Tab::Trend => "Trend",
            Tab::Comparison => tr("Karşılaştırma"),
            Tab::Help => tr("Komut kütüphanesi"),
            Tab::Log => tr("Tanılama"),
            Tab::Settings => tr("Ayarlar"),
        }
    }
    /// Label on the tab strip: the two program views are the project's "Main" document.
    fn tab_label(self) -> String {
        match self {
            Tab::Ladder => tr("Main · Ladder").into(),
            Tab::Source => tr("Main · Kaynak").into(),
            other => other.title().into(),
        }
    }
    fn icon(self) -> gpui_component::IconName {
        match self {
            Tab::Ladder => gpui_component::IconName::LayoutDashboard,
            Tab::Source => gpui_component::IconName::File,
            Tab::NewProject => gpui_component::IconName::Plus,
            Tab::Hardware => gpui_component::IconName::Frame,
            Tab::CrossRef => gpui_component::IconName::Replace,
            Tab::Watch => gpui_component::IconName::Eye,
            Tab::Diff | Tab::Comparison => gpui_component::IconName::ChevronsUpDown,
            Tab::Comments => gpui_component::IconName::Inbox,
            Tab::Symbols => gpui_component::IconName::Asterisk,
            Tab::Search => gpui_component::IconName::Search,
            Tab::Connection => gpui_component::IconName::Globe,
            Tab::Modules => gpui_component::IconName::GalleryVerticalEnd,
            Tab::FileMemory => gpui_component::IconName::Folder,
            Tab::Io => gpui_component::IconName::Map,
            Tab::Trend => gpui_component::IconName::ChartPie,
            Tab::Help => gpui_component::IconName::BookOpen,
            Tab::Log => gpui_component::IconName::SquareTerminal,
            Tab::Settings => gpui_component::IconName::Settings,
        }
    }
    /// The program views stay open; every other page can be closed.
    fn closable(self) -> bool {
        !matches!(self, Tab::Ladder | Tab::Source)
    }
}
enum Done {
    Connect(Result<Bridge, String>),
    Upload(Result<Comparison, String>),
    Start(Result<OnlineSnapshot, String>),
    Poll(Result<OnlineSnapshot, String>),
    Stop(Result<OnlineSnapshot, String>),
    Step(Result<Comparison, String>),
    Read(Result<(String, usize), String>),
    File(Option<Result<(Project, PathBuf), String>>),
    Saved(Option<Result<PathBuf, String>>),
    Exported(Option<Result<(), String>>),
    /// A program package was written (path) or read from a file (its text).
    PackSaved(Option<Result<String, String>>),
    PackRead(Option<Result<String, String>>),
    Ports(Result<Vec<crate::plc_connection::PortInfo>, String>),
    Hardware(Result<(crate::plc_connection::Client, crate::plc_connection::Sample), String>),
    HardwarePoll(Result<crate::plc_connection::Sample, String>),
    Download(Result<opendvp::plc_download::Report, String>),
    Probe(Result<opendvp::plc_download::Probe, String>),
    Force(Result<String, String>),
    Control(Result<String, String>),
    ModuleScan(Result<opendvp::plc_modules::ModuleScan, String>),
    Report(Option<Result<PathBuf, String>>),
    MemoryLoaded(Option<Result<(opendvp::file_memory::FileLoad, PathBuf), String>>),
    MemorySaved(Option<Result<(opendvp::file_memory::FileStore, PathBuf), String>>),
}
pub struct Workbench {
    focus: FocusHandle,
    project: Project,
    path: Option<PathBuf>,
    dirty: bool,
    /// Last recovery copy / start of the current unsaved stretch (see `autosave`).
    autosave_at: Instant,
    mcp_sync_at: Instant,
    /// The next opened project is a recovery copy: it opens untitled so Save asks for a name.
    open_untitled: bool,
    editor: Entity<EditorState>,
    query: Entity<InputState>,
    simulator_path: Entity<InputState>,
    tab: Tab,
    /// Open document tabs in display order; viewing any page opens (and keeps) its tab.
    open_tabs: Vec<Tab>,
    program: Option<Program>,
    simulator: Simulator,
    compile_error: Option<String>,
    source_changed: Option<Instant>,
    bridge: Option<Bridge>,
    busy: bool,
    uploaded: bool,
    running: bool,
    stop_requested: bool,
    local_running: bool,
    last_local: Instant,
    local_cycle_ms: Option<u128>,
    /// Encoded program size in steps, when the program has a native encoding.
    program_steps: Option<usize>,
    /// Forced device values (bits as 0/1) re-applied by the local simulation each scan.
    forced: BTreeMap<Device, i32>,
    /// Where the last program read came from, for comparison texts.
    read_origin: String,
    /// Simulator the bridge runs (`ES2`/`SS2`), for status texts.
    sim_name: &'static str,
    /// Compile error line (1-based) and message, shown in the source editor.
    compile_mark: Option<(usize, String)>,
    /// Mark currently drawn in the editor, to update it only when it changes.
    shown_mark: Option<(usize, String)>,
    snapshot: Option<OnlineSnapshot>,
    online_used: bool,
    values: BTreeMap<(Device, String), u32>,
    inputs: BTreeMap<Device, bool>,
    pending_inputs: BTreeMap<Device, bool>,
    next_poll: Instant,
    poll_ms: Option<u128>,
    read_preview: Option<(String, usize)>,
    comparison: Option<(u64, usize)>,
    status: String,
    logs: VecDeque<String>,
    history: VecDeque<(u64, Vec<(String, u32)>)>,
    device_mode: bool,
    device_type: DeviceType,
    case_sensitive: bool,
    whole_token: bool,
    _subscriptions: Vec<Subscription>,
    _clock: Task<()>,
    editor_sync: bool,
    /// Compile messages (error, warnings, notes) shown in the bottom panel.
    messages: Vec<messages::Message>,
    /// Source line to put the cursor on after the next editor sync.
    jump_line: Option<usize>,
    /// Program being edited: 0 = Main, n = `project.units[n - 1]`.
    active_program: usize,
    /// Networks of the selected program when the project has several programs.
    view: Option<Program>,
    /// The unit name / number inputs must be refilled from the selected unit.
    unit_sync: bool,
    close_requested: bool,
    close_discard: bool,
    displayed_stale: bool,
    extra: panels::Panels,
    selected: usize,
    undo: Vec<(SourceLanguage, String)>,
    redo: Vec<(SourceLanguage, String)>,
    ladder_scroll: ScrollHandle,
    selected_sync: bool,
    after_stop_save: bool,
    ui: widgets::WorkspaceUi,
    ladder_ui: interactive::LadderUi,
}
impl Drop for Workbench {
    fn drop(&mut self) {
        if let Some(bridge) = &self.bridge {
            bridge.disconnect();
        }
        if let Some(hardware) = &self.extra.hardware {
            hardware.disconnect();
        }
    }
}
impl Workbench {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            weak.update(cx, |this, cx| {
                this.close_requested = true;
                if this.running || this.busy {
                    this.stop(cx);
                    return false;
                }
                if this.unsaved() {
                    this.log(
                        tr("Kaydedilmemiş değişiklikler var. Kaydedin veya Kaydetmeden kapat’ı seçin."),
                    );
                    cx.notify();
                    return false;
                }
                true
            })
            .unwrap_or(true)
        });
        let settings = opendvp::settings::get();
        let startup = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .or_else(|| settings.reopen_last.then(|| settings.last_project.clone()).flatten())
            .and_then(|p| Project::load(&p).ok().map(|project| (project, p)));
        let (project, startup_path) = match startup {
            Some((project, path)) => (project, Some(path)),
            None => (Project::default(), None),
        };
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("plain_text")
                .line_number(true)
                .searchable(true)
                .soft_wrap(false)
                .default_value(project.source.clone())
        });
        let query =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr("Metin veya cihaz adresi…")));
        let simulator_path = cx
            .new(|cx| InputState::new(window, cx).default_value(es2_bridge::DEFAULT_SS2_SIMULATOR));
        let extra = panels::Panels::new(window, cx);
        let ui = widgets::WorkspaceUi::new(&project, window, cx);
        let ladder_ui = interactive::LadderUi::new(window, cx);
        let subscriptions = vec![
            cx.subscribe_in(&editor, window, |this, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let source = this.editor.read(cx).value().to_string();
                    if source != this.active_source() {
                        this.undo
                            .push((
                                this.project.program_language(this.active_program),
                                this.active_source().to_owned(),
                            ));
                        this.redo.clear();
                        if this.undo.len() > 100 {
                            this.undo.remove(0);
                        }
                        let index = this.active_program;
                        this.project.set_program_source(index, source);
                        this.dirty = true;
                        this.uploaded = false;
                        if let Some(bridge) = this.bridge.take() {
                            bridge.disconnect();
                        }
                        this.values.clear();
                        this.snapshot = None;
                        this.inputs.clear();
                        this.pending_inputs.clear();
                        this.log(
                            tr("Kaynak değişti · online görüntü temizlendi; programı yeniden yazın."),
                        );
                        this.read_preview = None;
                        this.comparison = None;
                        this.source_changed = Some(Instant::now());
                        cx.notify();
                    }
                }
            }),
            cx.subscribe(&query, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        let clock = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(25)).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut app = Self {
            focus,
            project,
            path: startup_path,
            dirty: false,
            autosave_at: Instant::now(),
            mcp_sync_at: Instant::now(),
            open_untitled: false,
            editor,
            query,
            simulator_path,
            tab: Tab::Ladder,
            open_tabs: vec![Tab::Ladder, Tab::Source, Tab::Io, Tab::Trend],
            program: None,
            simulator: Simulator::default(),
            compile_error: None,
            source_changed: None,
            bridge: None,
            busy: false,
            uploaded: false,
            running: false,
            stop_requested: false,
            local_running: false,
            last_local: Instant::now(),
            local_cycle_ms: None,
            compile_mark: None,
            program_steps: None,
            sim_name: "SIM",
            forced: BTreeMap::new(),
            read_origin: String::new(),
            shown_mark: None,
            snapshot: None,
            online_used: false,
            values: BTreeMap::new(),
            inputs: BTreeMap::new(),
            pending_inputs: BTreeMap::new(),
            next_poll: Instant::now(),
            poll_ms: None,
            read_preview: None,
            comparison: None,
            status: tr("Hazır · simülatöre bağlanın veya yerel taramayı başlatın.").into(),
            logs: VecDeque::new(),
            history: VecDeque::new(),
            device_mode: false,
            device_type: DeviceType::Any,
            case_sensitive: false,
            whole_token: false,
            _subscriptions: subscriptions,
            _clock: clock,
            editor_sync: false,
            messages: Vec::new(),
            jump_line: None,
            active_program: 0,
            view: None,
            unit_sync: true,
            close_requested: false,
            close_discard: false,
            displayed_stale: false,
            extra,
            selected: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            ladder_scroll: ScrollHandle::new(),
            selected_sync: true,
            after_stop_save: false,
            ui,
            ladder_ui,
        };
        app.compile();
        app
    }
    fn log(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.status = message.clone();
        self.logs.push_front(message);
        self.logs.truncate(200);
    }
    fn compile(&mut self) {
        self.ladder_ui.selection = None;
        self.ladder_ui.placement = None;
        self.ladder_ui.feedback = None;
        self.source_changed = None;
        self.local_running = false;
        self.simulator.restart_preserving_file_store();
        self.inputs.clear();
        self.apply_initial_values();
        match self.project.assign_symbol_addresses() {
            Ok(true) => self.dirty = true,
            Ok(false) => {}
            Err(e) => {
                self.compile_error = Some(e);
                self.compile_mark = None;
                self.program = None;
                self.view = None;
                self.collect_messages();
                return;
            }
        }
        match self.project.compile_located() {
            Ok(program) => {
                self.compile_error = None;
                // Program size in steps (encoded words) against the model's capacity.
                self.program_steps = opendvp::es2_encoder::encode(&program)
                    .ok()
                    .map(|e| e.words.len());
                if let (Some(steps), Some(model)) = (
                    self.program_steps,
                    opendvp::devices::find(&self.project.device),
                ) && steps as u32 > model.steps
                {
                    self.log(tfmt!(
                        "Uyarı: program {steps} adım; {} kapasitesi {} adım. PLC'ye yüklenemez.", "Warning: the program is {steps} steps; the capacity of {} is {} steps. It cannot be downloaded to the PLC.",
                        model.title, model.steps
                    ));
                }
                self.program = Some(program);
                self.refresh_view();
                let column = opendvp::devices::find(&self.project.device)
                    .map_or("ES2", |m| m.special_column);
                self.warn_unsupported_specials(column);
            }
            Err(e) => {
                // With several programs the error is reported inside the program it is in.
                let diagnostic = opendvp::plc::Diagnostic {
                    line: e.line,
                    message: e.message.clone(),
                };
                let text = if self.project.units.is_empty() {
                    diagnostic.to_string()
                } else {
                    if e.program != self.active_program {
                        self.active_program = e.program;
                        self.undo.clear();
                        self.redo.clear();
                        self.ladder_ui.selection = None;
                        self.editor_sync = true;
                        self.unit_sync = true;
                    }
                    format!("[{}] {diagnostic}", self.project.program_name(e.program))
                };
                self.compile_error = Some(text);
                self.compile_mark = Some((e.line, e.message.clone()));
                self.program = None;
                self.view = None;
            }
        }
        if self.compile_error.is_none() {
            self.compile_mark = None;
        }
        self.selected = self.selected.min(
            self.view_program()
                .map_or(0, |p| p.networks.len().saturating_sub(1)),
        );
        self.collect_messages();
    }
    /// Delta's per-model table (Delta editor DVP2AS_SM/SD) for special M/D named in the source.
    fn warn_unsupported_specials(&mut self, model: &str) {
        let Ok(il) = self.project.instruction_list() else {
            return;
        };
        let devices = opendvp::special_registers::unsupported_in_il(model, &il);
        if !devices.is_empty() {
            let names: Vec<String> = devices.iter().map(ToString::to_string).collect();
            self.log(tfmt!(
                "Uyarı: {model} bu özel cihazları desteklemiyor (Delta tablosu): {}", "Warning: {model} does not support these special devices (Delta table): {}",
                names.join(", ")
            ));
        }
    }
    fn editing_locked(&self) -> bool {
        self.busy || self.running || self.local_running || self.extra.hardware.is_some()
    }
    fn dispatch(&mut self, future: impl Future<Output = Done> + 'static, cx: &mut Context<Self>) {
        self.busy = true;
        let started = Instant::now();
        cx.spawn(async move |this, cx| {
            let result = future.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.complete(result, started, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn accept_snapshot(&mut self, snapshot: OnlineSnapshot) {
        self.running = snapshot.running;
        self.values = snapshot
            .rows
            .iter()
            .map(|r| ((r.device, r.kind.to_string()), r.value))
            .collect();
        let row = snapshot
            .rows
            .iter()
            .filter(|r| r.kind != "bit" || matches!(r.device.area, Area::X | Area::Y))
            .map(|r| (r.device.to_string(), r.value))
            .collect();
        self.history.push_back((snapshot.sequence, row));
        if self.history.len() > 120 {
            self.history.pop_front();
        }
        self.snapshot = Some(snapshot);
    }
    fn fault(&mut self, error: String) {
        self.after_stop_save = false;
        if let Some(bridge) = self.bridge.take() {
            bridge.disconnect();
        }
        self.running = false;
        self.uploaded = false;
        self.stop_requested = false;
        self.values.clear();
        self.snapshot = None;
        self.pending_inputs.clear();
        self.log(tfmt!("Bağlantı hatası: {error}", "Connection error: {error}"));
    }
    fn complete(&mut self, done: Done, started: Instant, cx: &mut Context<Self>) {
        match done {
            Done::Connect(result) => match result {
                Ok(bridge) => {
                    self.log(tfmt!(
                        "{} simülatörüne bağlı · 127.0.0.1:{} · STOP", "Connected to the {} simulator · 127.0.0.1:{} · STOP",
                        self.sim_name,
                        bridge.port()
                    ));
                    self.bridge = Some(bridge);
                }
                Err(e) => self.fault(e),
            },
            Done::Upload(result) => match result {
                Ok(c) => {
                    self.extra.comparison_rows = c
                        .rows
                        .iter()
                        .map(|r| {
                            (
                                r.device.to_string(),
                                r.kind.to_string(),
                                r.local,
                                r.original,
                            )
                        })
                        .collect();
                    self.uploaded = true;
                    self.online_used = false;
                    self.inputs.clear();
                    self.pending_inputs.clear();
                    self.snapshot = None;
                    self.values = c
                        .rows
                        .iter()
                        .map(|r| ((r.device, r.kind.to_string()), r.original))
                        .collect();
                    self.comparison = Some((c.scan, c.mismatches));
                    self.log(tfmt!(
                        "Program SS2’ye yazıldı ve geri doğrulandı · {} okuma · {} fark", "Program written to SS2 and verified by reading back · {} reads · {} differences",
                        c.rows.len(),
                        c.mismatches
                    ));
                }
                Err(e) => self.fault(e),
            },
            Done::Start(result) | Done::Stop(result) => match result {
                Ok(snapshot) => {
                    let running = snapshot.running;
                    self.online_used |= running;
                    self.accept_snapshot(snapshot);
                    self.next_poll = Instant::now();
                    self.log(if running {
                        tfmt!("{} RUN · sürekli online izleme", "{} RUN · continuous online monitoring", self.sim_name)
                    } else {
                        tfmt!("{} STOP · durdurma onaylandı", "{} STOP · stop confirmed", self.sim_name)
                    });
                }
                Err(e) => self.fault(e),
            },
            Done::Poll(result) => match result {
                Ok(snapshot) => {
                    self.poll_ms = Some(started.elapsed().as_millis());
                    self.accept_snapshot(snapshot);
                    self.next_poll = started + Duration::from_millis(opendvp::settings::get().poll_ms);
                    if !self.running {
                        self.log(tfmt!("{} STOP durumu okundu.", "{} STOP status read.", self.sim_name));
                    }
                }
                Err(e) => self.fault(e),
            },
            Done::Step(result) => match result {
                Ok(c) => {
                    self.extra.comparison_rows = c
                        .rows
                        .iter()
                        .map(|r| {
                            (
                                r.device.to_string(),
                                r.kind.to_string(),
                                r.local,
                                r.original,
                            )
                        })
                        .collect();
                    self.values = c
                        .rows
                        .iter()
                        .map(|r| ((r.device, r.kind.to_string()), r.original))
                        .collect();
                    self.comparison = Some((c.scan, c.mismatches));
                    self.log(tfmt!(
                        "Karşılaştırma taraması {} · {} fark", "Comparison scan {} · {} differences",
                        c.scan, c.mismatches
                    ));
                }
                Err(e) => self.fault(e),
            },
            Done::Read(result) => match result {
                Ok(preview) => {
                    self.log(tfmt!(
                        "{} programı okundu · {} WORD", "{} program read · {} WORD",
                        self.read_origin, preview.1
                    ));
                    self.read_preview = Some(preview);
                    if self.tab != Tab::Diff {
                        self.tab = Tab::Source;
                    }
                }
                Err(e) => self.log(tfmt!("Program okunamadı: {e}", "The program could not be read: {e}")),
            },
            Done::File(Some(result)) => match result {
                Ok((project, path)) => {
                    self.project = project;
                    self.active_program = 0;
                    self.unit_sync = true;
                    self.undo.clear();
                    self.redo.clear();
                    self.forced.clear();
                    if std::mem::take(&mut self.open_untitled) {
                        self.path = None;
                        self.dirty = true;
                    } else {
                        opendvp::settings::update(|s| s.last_project = Some(path.clone()));
                        opendvp::settings::push_recent(&path);
                        self.path = Some(path);
                        self.dirty = false;
                    }
                    self.editor_sync = true;
                    self.compile();
                    self.selected = 0;
                    self.selected_sync = true;
                    self.log(tr("Proje açıldı."));
                }
                Err(e) => self.log(e),
            },
            Done::Saved(Some(result)) => match result {
                Ok(path) => {
                    opendvp::settings::update(|s| s.last_project = Some(path.clone()));
                    opendvp::settings::push_recent(&path);
                    self.path = Some(path);
                    self.dirty = false;
                    self.discard_recovery();
                    self.log(tr("Proje kaydedildi."));
                }
                Err(e) => self.log(e),
            },
            Done::File(None) | Done::Saved(None) => {}
            Done::Ports(result) => match result {
                Ok(ports) => self.extra.profiles.set_ports(ports),
                Err(e) => self.log(e),
            },
            Done::Hardware(result) => match result {
                Ok((client, sample)) => {
                    self.extra.hardware = Some(client);
                    self.extra.hardware_polling = true;
                    self.adopt_hardware(sample);
                    self.log(tr("Fiziksel PLC bağlı · yalnız X/Y okuma"));
                }
                Err(e) => self.log(tfmt!("PLC bağlantısı kurulamadı: {e}", "Could not connect to the PLC: {e}")),
            },
            Done::Download(result) => match result {
                Ok(report) => {
                    self.log(tfmt!(
                    "Program PLC’ye yazıldı ve geri okunarak doğrulandı · {} adım · model {} · kapasite {} · {}", "Program written to the PLC and verified by reading back · {} steps · model {} · capacity {} · {}",
                    report.words_written,
                    report.identity.model.unwrap_or("bilinmiyor"),
                    report.identity.capacity_steps,
                    if report.running { "RUN" } else { "STOP" }
                    ));
                    if let Some(model) = report.identity.model {
                        self.warn_unsupported_specials(model);
                    }
                }
                Err(e) => self.log(tfmt!("PLC’ye program yüklenemedi: {e}", "The program could not be downloaded to the PLC: {e}")),
            },
            Done::ModuleScan(result) => {
                match &result {
                    Ok(scan) => self.log(tfmt!(
                        "PLC modül taraması · {} özel modül · {} giriş / {} çıkış genişleme", "PLC module scan · {} special modules · {} inputs / {} outputs expansion",
                        scan.types.len(),
                        scan.x_points,
                        scan.y_points
                    )),
                    Err(e) => self.log(tfmt!("PLC modül taraması başarısız: {e}", "PLC module scan failed: {e}")),
                }
                self.extra.hw_scan = Some(result);
            }
            Done::Force(result) => match result {
                Ok(message) => self.log(message),
                Err(e) => {
                    self.forced.clear();
                    self.log(tfmt!("PLC zorlama komutu başarısız: {e}", "PLC force command failed: {e}"));
                }
            },
            Done::Control(result) => match result {
                Ok(message) => self.log(message),
                Err(e) => self.log(tfmt!("PLC komutu başarısız: {e}", "PLC command failed: {e}")),
            },
            Done::Probe(result) => {
                match &result {
                    Ok(probe) => self.log(tfmt!(
                        "Bağlantı testi başarılı · model {} · kapasite {} adım", "Connection test succeeded · model {} · capacity {} steps",
                        probe.identity.model.unwrap_or("bilinmiyor"),
                        probe.identity.capacity_steps
                    )),
                    Err(e) => self.log(tfmt!("Bağlantı testi başarısız: {e}", "Connection test failed: {e}")),
                }
                self.extra.last_probe = Some(result);
            }
            Done::HardwarePoll(result) => match result {
                Ok(sample) => self.adopt_hardware(sample),
                Err(e) => {
                    self.disconnect_hardware();
                    self.log(tfmt!("PLC okuma hatası: {e}", "PLC read error: {e}"));
                }
            },
            Done::MemoryLoaded(result) => match result {
                Some(Ok((load, path))) => {
                    self.extra.memory_recovery =
                        load.recovery_source == opendvp::file_memory::RecoverySource::Previous;
                    self.simulator.memory.file_memory.configure(load.store);
                    self.extra.memory_path = Some(path);
                    self.log(tr("Dosya belleği yüklendi."));
                }
                Some(Err(e)) => self.log(e),
                None => {}
            },
            Done::MemorySaved(result) => match result {
                Some(Ok((store, path))) => {
                    self.simulator.memory.file_memory.configure(store);
                    self.extra.memory_path = Some(path);
                    self.extra.memory_recovery = false;
                    self.log(tr("Dosya belleği kaydedildi."));
                }
                Some(Err(e)) => self.log(e),
                None => {}
            },
            Done::Report(result) => match result {
                Some(Ok(path)) => {
                    self.log(tfmt!("Proje belgesi yazıldı: {}", "Project report written: {}", path.display()));
                    // Open in the default browser for reading, printing or saving as PDF.
                    let _ = std::process::Command::new("explorer").arg(&path).spawn();
                }
                Some(Err(e)) => self.log(e),
                None => {}
            },
            Done::Exported(result) => match result {
                Some(Ok(())) => self.log(tr("Kaynak dışa aktarıldı.")),
                Some(Err(e)) => self.log(e),
                None => {}
            },
            Done::PackSaved(result) => match result {
                Some(Ok(path)) => self.log(tfmt!("Program paketi yazıldı: {path}", "Program package written: {path}")),
                Some(Err(e)) => self.log(e),
                None => {}
            },
            Done::PackRead(result) => match result {
                Some(text) => self.import_package_text(text, cx),
                None => {}
            },
        }
        self.capture_trend(cx);
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        self.sync_mcp(cx);
        if self.dirty {
            self.autosave();
        } else {
            self.autosave_at = Instant::now();
        }
        let stale = self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.observed_at.elapsed() > Duration::from_secs(1))
            || (self.extra.hardware.is_some() && self.hardware_stale());
        if stale != self.displayed_stale {
            self.displayed_stale = stale;
            cx.notify();
        }
        if self
            .source_changed
            .is_some_and(|t| t.elapsed() > Duration::from_millis(350))
        {
            self.compile();
            cx.notify();
        }
        if self.local_running && self.last_local.elapsed() >= Duration::from_millis(20) {
            self.apply_forces(true);
            if let Some(program) = &self.program {
                self.simulator.scan(program, opendvp::settings::get().scan_ms);
            }
            self.apply_forces(false);
            // Measured wall-clock interval between local scans, shown in the status bar.
            self.local_cycle_ms = Some(self.last_local.elapsed().as_millis());
            self.last_local = Instant::now();
            self.capture_trend(cx);
            cx.notify();
        }
        if self.busy {
            return;
        }
        if self.extra.hardware_polling
            && self.extra.hardware.is_some()
            && Instant::now() >= self.extra.hardware_next
        {
            let hardware = self.extra.hardware.clone().unwrap();
            self.dispatch(async move { Done::HardwarePoll(hardware.read().await) }, cx);
            return;
        }
        if self.stop_requested {
            self.stop_requested = false;
            if let Some(bridge) = self.bridge.clone() {
                self.dispatch(async move { Done::Stop(bridge.stop_online().await) }, cx);
                return;
            }
        }
        if self.after_stop_save && !self.running {
            self.after_stop_save = false;
            self.save(cx);
            return;
        }
        if self.running
            && Instant::now() >= self.next_poll
            && let Some(bridge) = self.bridge.clone()
        {
            let inputs = std::mem::take(&mut self.pending_inputs)
                .into_iter()
                .collect();
            self.dispatch(
                async move { Done::Poll(bridge.poll_online(inputs).await) },
                cx,
            );
        }
    }
    fn connect(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.bridge.is_some() || self.extra.hardware.is_some() {
            return;
        }
        self.local_running = false;
        if !self.forced.is_empty() {
            self.forced.clear();
            self.log(tr("Simülatöre bağlanırken yerel zorlamalar kaldırıldı."));
        }
        let mut path = self.simulator_path.read(cx).value().to_string();
        // Default ES2/SS2 simulator paths follow the project model; a custom
        // executable chosen in the connection profile is kept as is.
        let file = std::path::Path::new(&path)
            .file_name()
            .map(|f| f.to_string_lossy().to_ascii_lowercase());
        if matches!(
            file.as_deref(),
            Some("dvpsimulator_es2.exe" | "dvpsimulator_ss2.exe")
        ) {
            let wanted = opendvp::devices::bridge_simulator(&self.project.device);
            path = std::path::Path::new(&path)
                .with_file_name(wanted)
                .to_string_lossy()
                .into_owned();
        }
        if !opendvp::devices::runs_natively_on_bridge(&self.project.device) {
            self.log(format!(
                "{} için doğrulanmış simülatör yok; ES2 simülatörüyle bağlanılıyor. \
                 Modele özgü komutlar farklı davranabilir.",
                self.project.device
            ));
        }
        let name = std::path::Path::new(&path)
            .file_stem()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.sim_name = if name.to_ascii_uppercase().ends_with("SS2") {
            "SS2"
        } else if name.to_ascii_uppercase().ends_with("ES2") {
            "ES2"
        } else {
            "SIM"
        };
        self.log(tfmt!("{name} başlatılıyor…", "Starting {name}…"));
        self.dispatch(
            async move { Done::Connect(es2_bridge::launch(path).await) },
            cx,
        );
    }
    fn upload(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.running {
            return;
        }
        self.compile();
        let source = match self.project.instruction_list() {
            Ok(s) => s,
            Err(e) => {
                self.log(e.to_string());
                return;
            }
        };
        if let Some(bridge) = self.bridge.clone() {
            self.log(tr("Program yazılıyor ve doğrulanıyor…"));
            self.dispatch(async move { Done::Upload(bridge.upload(source).await) }, cx);
        }
    }
    fn run(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.running || !self.uploaded {
            return;
        }
        if let Some(bridge) = self.bridge.clone() {
            if !bridge.supports_online() {
                self.log(tr("Sürekli online RUN için COMMGR SS2 seçin; bu hedefte tek tarama karşılaştırması kullanılabilir."));
                cx.notify();
                return;
            }
            self.dispatch(async move { Done::Start(bridge.start_online().await) }, cx);
        }
    }
    fn stop(&mut self, cx: &mut Context<Self>) {
        self.local_running = false;
        if self.bridge.is_some() {
            self.stop_requested = true;
            self.log(tr("STOP bekleniyor…"));
            self.tick(cx);
        }
        cx.notify();
    }
    fn read(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.running {
            return;
        }
        if let Some(bridge) = self.bridge.clone() {
            self.read_origin = tfmt!("{} simülatörü", "{} simulator", self.sim_name);
            self.dispatch(
                async move {
                    Done::Read(
                        bridge
                            .discover_program_words()
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|words| {
                                opendvp::es2_program_decode::decode(&words)
                                    .map(|s| (s, words.len()))
                                    .map_err(|e| e.to_string())
                            }),
                    )
                },
                cx,
            );
        }
    }
    fn toggle_input(&mut self, device: Device, cx: &mut Context<Self>) {
        if self.extra.hardware.is_some() {
            return;
        }
        let value = !self.inputs.get(&device).copied().unwrap_or(false);
        self.inputs.insert(device, value);
        if self.bridge.is_some() {
            self.pending_inputs.insert(device, value);
        } else {
            self.simulator.memory.set_input(device, value);
        }
        cx.notify();
    }
    fn disconnect(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.running {
            return;
        }
        if let Some(bridge) = self.bridge.take() {
            bridge.disconnect();
        }
        self.uploaded = false;
        self.snapshot = None;
        self.values.clear();
        self.pending_inputs.clear();
        self.inputs.clear();
        self.log(tr("Bağlantı kapatıldı."));
        cx.notify();
    }
    /// Open a project from the recent / recovery lists of the settings page. A recovery copy
    /// opens untitled (unsaved) so the original file is never overwritten by accident.
    pub(super) fn open_path(&mut self, path: PathBuf, untitled: bool, cx: &mut Context<Self>) {
        if self.editing_locked() || self.bridge.is_some() {
            return;
        }
        if self.dirty {
            self.log(tr("Kaydedilmemiş değişiklikler var. Önce Kaydet’i kullanın."));
            cx.notify();
            return;
        }
        self.open_untitled = untitled;
        let task = panels::dialog(move || Some(Project::load(&path).map(|p| (p, path))));
        self.dispatch(async move { Done::File(task.await) }, cx);
    }
    /// Share the open project with the built-in MCP server and adopt a project it pushed.
    fn sync_mcp(&mut self, cx: &mut Context<Self>) {
        if !matches!(crate::mcp_server::status(), crate::mcp_server::Status::Running(_)) {
            crate::mcp_server::clear_window_project();
            return;
        }
        if let Some(project) = crate::mcp_server::take_pushed_project() {
            if self.editing_locked() || self.bridge.is_some() {
                self.log(tr("Yapay zekanın gönderdiği proje alınamadı: pencere meşgul veya simülatöre bağlı."));
            } else {
                if let Some(file) = self.recovery_file() {
                    if let Some(dir) = file.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = self.project.save(&file.with_extension("onceki.odvp"));
                }
                self.project = project;
                self.active_program = 0;
                self.unit_sync = true;
                self.undo.clear();
                self.redo.clear();
                self.forced.clear();
                self.editor_sync = true;
                self.selected = 0;
                self.selected_sync = true;
                self.dirty = true;
                self.compile();
                self.log(tr("Yapay zekanın gönderdiği proje açıldı (önceki hali kurtarma klasöründe)."));
                cx.notify();
            }
            self.mcp_sync_at = Instant::now() - Duration::from_secs(5);
        }
        if self.mcp_sync_at.elapsed() >= Duration::from_millis(700) {
            self.mcp_sync_at = Instant::now();
            crate::mcp_server::publish_window_project(&self.project, self.path.as_deref());
        }
    }
    fn recovery_file(&self) -> Option<PathBuf> {
        let stem: String = self
            .project
            .name
            .chars()
            .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' })
            .collect();
        let stem = stem.trim();
        let stem = if stem.is_empty() { "project" } else { stem };
        Some(opendvp::settings::recovery_dir()?.join(format!("{stem}.odvp")))
    }
    /// Write a recovery copy once the unsaved stretch has lasted the configured interval.
    fn autosave(&mut self) {
        if self.autosave_at.elapsed() < Duration::from_secs(60) {
            return;
        }
        let minutes = opendvp::settings::get().autosave_min;
        if minutes == 0 || self.autosave_at.elapsed() < Duration::from_secs(u64::from(minutes) * 60) {
            return;
        }
        self.autosave_at = Instant::now();
        let Some(file) = self.recovery_file() else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = self.project.save(&file);
    }
    fn discard_recovery(&self) {
        if let Some(file) = self.recovery_file() {
            let _ = std::fs::remove_file(file);
        }
    }
    fn file_open(&mut self, cx: &mut Context<Self>) {
        if self.editing_locked() || self.bridge.is_some() {
            return;
        }
        if self.dirty {
            self.log(tr("Kaydedilmemiş değişiklikler var. Önce Kaydet’i kullanın."));
            cx.notify();
            return;
        }
        let task = panels::dialog(|| {
            panels::start_dir(rfd::FileDialog::new())
                .add_filter(
                    "OpenDVP / IL / ST / DVZ / Delta editor",
                    &["odvp", "json", "il", "st", "dvz", "dvp", "isp"],
                )
                .pick_file()
                .map(|path| Project::load(&path).map(|p| (p, path)))
        });
        self.dispatch(async move { Done::File(task.await) }, cx);
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        if self.running || (self.busy && self.bridge.is_some()) {
            self.after_stop_save = true;
            self.stop_requested = true;
            self.log(tr("Kaydetmeden önce SS2 STOP onayı bekleniyor…"));
            self.tick(cx);
            cx.notify();
            return;
        }
        if self.busy {
            return;
        }
        let project = self.project.clone();
        let path = self.path.clone().filter(|p| {
            !p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("il") || e.eq_ignore_ascii_case("st"))
        });
        let task = panels::dialog(move || {
            path.or_else(|| {
                panels::start_dir(rfd::FileDialog::new())
                    .add_filter("OpenDVP", &["odvp"])
                    .set_file_name("program.odvp")
                    .save_file()
            })
            .map(|path| {
                if opendvp::settings::get().backup_on_save && path.is_file() {
                    let mut backup = path.clone().into_os_string();
                    backup.push(".bak");
                    let _ = std::fs::copy(&path, backup);
                }
                project.save(&path).map(|_| path)
            })
        });
        self.dispatch(async move { Done::Saved(task.await) }, cx);
    }
    fn export(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let project = self.project.clone();
        let task = panels::dialog(move || {
            panels::start_dir(rfd::FileDialog::new())
                .add_filter(tr("Kaynak"), &[project.language.extension()])
                .add_filter("Delta editor", &["dvp"])
                .set_file_name(format!("program.{}", project.language.extension()))
                .save_file()
                .map(|path| project.export_source(&path))
        });
        self.dispatch(async move { Done::Exported(task.await) }, cx);
    }
    /// Write the printable project documentation (HTML) and open it.
    fn export_report(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let html = crate::report::html(&self.project, self.program.as_ref());
        let name = tfmt!("{} - belge.html", "{} - report.html", self.project.name);
        let task = panels::dialog(move || {
            panels::start_dir(rfd::FileDialog::new())
                .add_filter(tr("HTML belge"), &["html"])
                .set_file_name(name)
                .save_file()
                .map(|path| {
                    std::fs::write(&path, html)
                        .map(|_| path)
                        .map_err(|e| tfmt!("Belge yazılamadı: {e}", "The report could not be written: {e}"))
                })
        });
        self.dispatch(async move { Done::Report(task.await) }, cx);
    }
    fn new_project(&mut self, cx: &mut Context<Self>) {
        if self.editing_locked() || self.bridge.is_some() {
            return;
        }
        if self.dirty {
            self.log(tr("Önce mevcut değişiklikleri kaydedin."));
            cx.notify();
            return;
        }
        // The project is created from the device selection screen.
        self.tab = Tab::NewProject;
        cx.notify();
    }
    /// Start an empty project for `model` (id from `devices::MODELS`).
    fn create_project(&mut self, model: &str, name: &str, st: bool, cx: &mut Context<Self>) {
        let file_memory = std::mem::take(&mut self.simulator.memory.file_memory);
        self.simulator = Simulator::default();
        self.simulator.memory.file_memory = file_memory;
        let name = name.trim();
        self.project = Project {
            source: if st {
                opendvp::project::ST_TEMPLATE.into()
            } else {
                "; Ağ 1\nLD X0\nOUT Y0\nEND\n".into()
            },
            language: if st {
                SourceLanguage::St
            } else {
                SourceLanguage::Il
            },
            name: if name.is_empty() {
                tr("Adsız proje").into()
            } else {
                name.chars().take(200).collect()
            },
            device: model.into(),
            watch: vec![],
            ..Project::default()
        };
        self.undo.clear();
        self.redo.clear();
        self.active_program = 0;
        self.unit_sync = true;
        self.forced.clear();
        self.selected = 0;
        self.selected_sync = true;
        self.path = None;
        self.dirty = true;
        self.editor_sync = true;
        self.read_preview = None;
        self.compile();
        self.tab = if st { Tab::Source } else { Tab::Ladder };
        self.log(tfmt!("{} için yeni proje oluşturuldu.", "New project created for {}.", model));
        cx.notify();
    }
    fn observed_bit(&self, device: Device) -> Option<bool> {
        if self.extra.hardware.is_some() {
            return if self.hardware_stale() {
                None
            } else {
                self.values.get(&(device, "bit".into())).map(|v| *v != 0)
            };
        }
        if self.bridge.is_none() {
            Some(self.simulator.memory.bit(device))
        } else if self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.observed_at.elapsed() > Duration::from_secs(1))
        {
            None
        } else {
            self.values.get(&(device, "bit".into())).map(|v| *v != 0)
        }
    }
    fn device_value(&self, device: Device) -> String {
        if self.extra.hardware.is_some() {
            return self
                .observed_bit(device)
                .map(|v| u8::from(v).to_string())
                .unwrap_or_else(|| "—".into());
        }
        if self.bridge.is_none() {
            if matches!(device.area, Area::D | Area::T | Area::C) {
                self.simulator.memory.word(device).to_string()
            } else {
                u8::from(self.simulator.memory.bit(device)).to_string()
            }
        } else if self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.observed_at.elapsed() > Duration::from_secs(1))
        {
            tr("? · eski").into()
        } else {
            let text = self
                .values
                .iter()
                .filter(|((d, _), _)| *d == device)
                .map(|((_, kind), v)| format!("{kind}={v}"))
                .collect::<Vec<_>>()
                .join(" · ");
            if text.is_empty() { "?".into() } else { text }
        }
    }
    fn button(&self, id: &'static str, label: impl Into<SharedString>, disabled: bool) -> Button {
        Button::new(id)
            .label(label)
            .small()
            .flex_shrink_0()
            .disabled(disabled)
    }
    fn heading(text: impl Into<SharedString>) -> Div {
        div().text_sm().text_color(rgb(MUTED)).child(text.into())
    }
    fn source(&self, cx: &mut Context<Self>) -> AnyElement {
        let locked = self.editing_locked();
        let mut body = div()
            .flex()
            .flex_col()
            .size_full()
            .gap_2()
            .p_3()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .mr_2()
                            .child(tfmt!(
                                "Kaynak · {}",
                                "Source · {}",
                                self.project.program_language(self.active_program).label()
                            )),
                    )
                    .child(
                        self.button("language", "IL / ST", locked || self.bridge.is_some())
                            .on_click(cx.listener(|this, _, _, cx| {
                                let flip = |language| {
                                    if language == SourceLanguage::Il {
                                        SourceLanguage::St
                                    } else {
                                        SourceLanguage::Il
                                    }
                                };
                                if this.active_program == 0 {
                                    this.project.language = flip(this.project.language);
                                } else if let Err(e) = this.project.set_unit_language(
                                    this.active_program,
                                    flip(this.project.program_language(this.active_program)),
                                ) {
                                    this.log(e);
                                    return cx.notify();
                                } else {
                                    this.editor_sync = true;
                                }
                                this.dirty = true;
                                this.compile();
                                cx.notify();
                            })),
                    )
                    .child(
                        self.button("export", tr("Dışa aktar"), self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.export(cx))),
                    )
                    .child(
                        self.button("new-st", tr("Yeni ST"), locked || self.bridge.is_some())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.dirty {
                                    this.log(tr("Yeni ST programından önce değişiklikleri kaydedin."));
                                    cx.notify();
                                    return;
                                }
                                this.commit_source(
                                    opendvp::project::ST_TEMPLATE.into(),
                                    SourceLanguage::St,
                                    cx,
                                );
                            })),
                    )
                    .child(
                        self.button("generated-il", tr("Üretilen IL"), false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.extra.generated = !this.extra.generated;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(Self::heading(tr("Ctrl+F ara · Ctrl+Z geri al · F7 derle")))
                    .child(match &self.compile_mark {
                        Some((line, message)) => div()
                            .max_w(px(420.))
                            .truncate()
                            .px_2()
                            .py(px(2.))
                            .rounded(px(4.))
                            .bg(rgb(0xfdecea))
                            .text_xs()
                            .text_color(rgb(ERROR))
                            .child(tfmt!("Satır {line}: {message}", "Line {line}: {message}")),
                        None => div()
                            .px_2()
                            .py(px(2.))
                            .rounded(px(4.))
                            .bg(rgb(0xeaf6ef))
                            .text_xs()
                            .text_color(rgb(GREEN))
                            .child(tr("Derleme hatasız")),
                    }),
            )
            .child(
                div().flex_1().min_h_0().child(
                    Editor::new(&self.editor)
                        .h_full()
                        .disabled(locked)
                        .font_family("Consolas")
                        .text_sm(),
                ),
            );
        if self.extra.generated {
            let text = self
                .project
                .instruction_list()
                .unwrap_or_else(|e| e.to_string());
            let symbols = if self.project.program_language(self.active_program) == SourceLanguage::St {
                opendvp::st::lower_with(self.active_source(), &self.project.lower_options())
                    .map(|l| {
                        l.symbols
                            .iter()
                            .map(|s| format!("{} → {} ({})", s.name, s.address, s.data_type.name()))
                            .collect::<Vec<_>>()
                            .join("   ")
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            body = body.child(
                div()
                    .h(px(220.))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .bg(rgb(PANEL))
                    .p_3()
                    .child(Self::heading(tfmt!("ÜRETİLEN IL · {symbols}", "GENERATED IL · {symbols}")))
                    .child(
                        div()
                            .id("generated-il-scroll")
                            .overflow_y_scroll()
                            .font_family("Consolas")
                            .text_sm()
                            .child(text),
                    ),
            );
        }
        if let Some((source, words)) = &self.read_preview {
            body=body.child(div().h(px(220.)).flex().flex_col().p_3().bg(rgb(PANEL)).gap_2()
                .child(div().flex().items_center().gap_3().child(tfmt!("SS2 program önizlemesi · {words} WORD", "SS2 program preview · {words} WORD"))
                    .child(self.button("import",tr("Kaynağa aktar"),locked).on_click(cx.listener(|this,_,_,cx|{
                        if let Some((source,_))=this.read_preview.take() {
                            // The PLC program is one flat program: replace Main and the units,
                            // then split it back into programs when its structure is clear.
                            this.disconnect(cx);
                            this.project.units.clear();
                            this.active_program = 0;
                            this.commit_source(source,SourceLanguage::Il,cx);
                            let split = this.project.split_main();
                            this.editor_sync = true; this.unit_sync = true; this.dirty = true;
                            this.compile();
                            this.log(match split {
                                Ok(n) => tfmt!(
                                    "Okunan program Main ve {n} ek programa ayrıldı. Yeniden bağlanıp yazabilirsiniz.",
                                    "The program that was read was split into Main and {n} more programs. You can reconnect and write."
                                ),
                                Err(_) => tr("Okunan IL kaynağa aktarıldı. Yeniden bağlanıp yazabilirsiniz.").to_owned(),
                            });
                        } cx.notify();
                    }))))
                .child(div().id("read-preview").overflow_y_scroll().font_family("Consolas").text_sm().child(source.clone())));
        }
        body.into_any_element()
    }
    fn search(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.query.read(cx).value();
        let results = if self.device_mode {
            device_search::find_devices(self.active_source(), &query, self.device_type)
        } else {
            Ok(crate::editor_search::find_matches(
                self.active_source(),
                &query,
                self.whole_token,
                self.case_sensitive,
            ))
        };
        let mut body = div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(Input::new(&self.query))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        self.button("search-text", tr("Metin"), false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.device_mode = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        self.button("search-device", tr("Cihaz"), false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.device_mode = true;
                                cx.notify();
                            })),
                    )
                    .child(Self::heading(if self.device_mode {
                        tr("Cihaz ve tip filtresi")
                    } else {
                        tr("Metin araması")
                    })),
            );
        let mut filters = div().flex().gap_2();
        if self.device_mode {
            for (id, label, kind) in [
                ("any", tr("Tümü"), DeviceType::Any),
                ("contact", tr("Kontak"), DeviceType::Contact),
                ("coil", tr("Bobin"), DeviceType::Coil),
                ("param", tr("Parametre"), DeviceType::Parameter),
                ("comment", tr("Yorum"), DeviceType::Comment),
                ("function", tr("Fonksiyon"), DeviceType::Function),
            ] {
                filters = filters.child(
                    self.button(
                        id,
                        if self.device_type == kind {
                            format!("• {label}")
                        } else {
                            label.into()
                        },
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.device_type = kind;
                        cx.notify();
                    })),
                );
            }
        } else {
            filters = filters
                .child(
                    self.button(
                        "case",
                        tfmt!(
                            "Harf duyarlı: {}", "Case sensitive: {}",
                            if self.case_sensitive {
                                tr("Açık")
                            } else {
                                tr("Kapalı")
                            }
                        ),
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.case_sensitive = !this.case_sensitive;
                        cx.notify();
                    })),
                )
                .child(
                    self.button(
                        "token",
                        tfmt!(
                            "Tam sözcük: {}", "Whole word: {}",
                            if self.whole_token {
                                tr("Açık")
                            } else {
                                tr("Kapalı")
                            }
                        ),
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.whole_token = !this.whole_token;
                        cx.notify();
                    })),
                );
        }
        body = body.child(filters);
        let mut rows = div()
            .id("search-results")
            .overflow_y_scroll()
            .flex_1()
            .flex()
            .flex_col()
            .gap_2();
        match results {
            Err(e) => rows = rows.child(e),
            Ok(matches) => {
                body = body.child(Self::heading(tfmt!("{} eşleşme", "{} matches", matches.len())));
                for (index, m) in matches.iter().take(500).enumerate() {
                    let line = self
                        .project
                        .source
                        .lines()
                        .nth(m.start_position.line)
                        .unwrap_or_default();
                    rows = rows.child(
                        div()
                            .px_3()
                            .py_2()
                            .bg(rgb(PANEL))
                            .font_family("Consolas")
                            .text_sm()
                            .child(format!(
                                "{}:{}  {}",
                                m.start_position.line + 1,
                                m.start_position.column + 1,
                                line
                            ))
                            .id(("match", index))
                            .cursor_pointer()
                            .on_click(cx.listener({
                                let position = gpui_component::input::Position::new(
                                    m.start_position.line as u32,
                                    line[..m.start_position.column].encode_utf16().count() as u32,
                                );
                                move |this, _, window, cx| {
                                    this.tab = Tab::Source;
                                    this.editor.update(cx, |editor, cx| {
                                        editor.set_cursor_position(position, window, cx)
                                    });
                                    cx.notify();
                                }
                            })),
                    );
                }
            }
        }
        body.child(rows).into_any_element()
    }
}
