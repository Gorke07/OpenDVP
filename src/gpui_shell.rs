use super::*;
use gpui_component::{
    Icon, IconName,
    highlighter::{Diagnostic, DiagnosticSeverity},
    input::Position,
    resizable::{h_resizable, resizable_panel, v_resizable},
    menu::{ContextMenuExt, DropdownMenu, PopupMenuItem},
    tab::{Tab as KitTab, TabBar},
};

impl Workbench {
    /// Underline the line of the current compile error in the source editor; hovering
    /// it shows the message.
    fn sync_compile_mark(&mut self, cx: &mut Context<Self>) {
        if self.shown_mark == self.compile_mark {
            return;
        }
        let mark = self.compile_mark.clone();
        // The editor holds the project source; measure the marked line there.
        let lines: Vec<&str> = self.active_source().lines().collect();
        let place = mark.as_ref().map(|(line, _)| {
            let row = line.saturating_sub(1).min(lines.len().saturating_sub(1));
            let width = lines.get(row).map_or(1, |l| l.chars().count().max(1));
            (row, width)
        });
        self.editor.update(cx, |input, cx| {
            let text = input.text().clone();
            if let Some(set) = input.diagnostics_mut() {
                set.reset(&text);
                if let (Some((_, message)), Some((row, width))) = (&mark, place) {
                    set.push(
                        Diagnostic::new(
                            Position::new(row as u32, 0)..Position::new(row as u32, width as u32),
                            message.clone(),
                        )
                        .with_severity(DiagnosticSeverity::Error),
                    );
                }
            }
            cx.notify();
        });
        self.shown_mark = mark;
    }

    /// Program size against the model's capacity, e.g. `29 / 7920 adım`, and whether
    /// it exceeds the capacity.
    fn capacity_text(&self) -> Option<(String, bool)> {
        let steps = self.program_steps?;
        let model = opendvp::devices::find(&self.project.device)?;
        Some((
            tfmt!("{steps} / {} adım", "{steps} / {} steps", model.steps),
            steps as u32 > model.steps,
        ))
    }

    /// Live timing in the status bar: online read latency, local scan interval, or state.
    /// Continuous online monitoring keeps `busy` set, so the latency must win over it.
    fn timing_text(&self) -> String {
        let online = self.bridge.is_some() || self.extra.hardware.is_some();
        match (online, self.poll_ms, self.local_cycle_ms) {
            (true, Some(ms), _) => tfmt!("Okuma {ms} ms", "Read {ms} ms"),
            (false, _, Some(ms)) if self.local_running => tfmt!("Tarama {ms} ms", "Scan {ms} ms"),
            _ if self.busy => tr("İşlem sürüyor").into(),
            _ => tr("Hazır").into(),
        }
    }

    /// Inspector window under the editor: element properties and the info/log list.
    fn bottom_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tabs = TabBar::new("bottom-tabs")
            .underline()
            .small()
            .selected_index(self.ui.bottom_tab)
            .child(KitTab::new().label(tr("Özellikler")))
            .child(KitTab::new().label(tfmt!("Bilgi ({})", "Info ({})", self.logs.len())))
            .child(KitTab::new().label(tfmt!("Mesajlar ({})", "Messages ({})", self.problem_count())))
            .on_click(cx.listener(|this, index, _, cx| {
                this.ui.bottom_tab = *index;
                cx.notify();
            }));
        let body = if self.ui.bottom_tab == 0 {
            div()
                .id("bottom-properties-scroll")
                .size_full()
                .overflow_y_scroll()
                .child(self.ladder_properties(cx))
                .into_any_element()
        } else if self.ui.bottom_tab == 2 {
            self.messages_view(cx)
        } else {
            let compile = match &self.compile_error {
                Some(error) => div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .bg(rgb(0xfdecea))
                    .text_color(rgb(ERROR))
                    .child(Icon::new(IconName::CircleX).size(px(14.)))
                    .child(error.clone()),
                None => div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .bg(rgb(0xeaf6ef))
                    .text_color(rgb(GREEN))
                    .child(Icon::new(IconName::CircleCheck).size(px(14.)))
                    .child(tfmt!(
                        "Derleme hatasız · {} ağ · {} komut{}", "Compiled without errors · {} networks · {} instructions{}",
                        self.program.as_ref().map_or(0, |p| p.networks.len()),
                        self.program.as_ref().map_or(0, |p| p.instructions),
                        self.capacity_text()
                            .map(|(text, _)| format!(" · {text}"))
                            .unwrap_or_default()
                    )),
            };
            div()
                .id("bottom-log")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .text_xs()
                .child(compile)
                .children(self.logs.iter().map(|text| {
                    div()
                        .px_3()
                        .py(px(5.))
                        .border_b_1()
                        .border_color(rgb(LINE))
                        .child(text.clone())
                }))
                .into_any_element()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(div().px_3().flex_shrink_0().child(tabs))
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
            .into_any_element()
    }
}

impl Workbench {
    /// Switch between Turkish and English; every view re-reads its texts on the next render.
    pub(super) fn toggle_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lang = opendvp::i18n::lang().other();
        opendvp::i18n::set_lang(lang);
        gpui_component::set_locale(lang.code());
        opendvp::i18n::save(lang);
        // Placeholders are stored in the input states, not re-read on render.
        let placeholders = [
            (&self.ladder_ui.query, tr("Komut ara…")),
            (&self.extra.comment_text, tr("Örn. Başlat butonu")),
            (&self.extra.comment_filter, tr("Adres veya açıklama ara…")),
            (&self.extra.xref_query, tr("Adres veya açıklama ara…")),
            (&self.extra.hw_query, tr("Modül ara…")),
            (&self.extra.library_query, tr("Komut, açıklama veya konu ara…")),
            (&self.query, tr("Metin veya cihaz adresi…")),
        ];
        for (input, text) in [
            (&self.extra.sym_name, tr("Ad")),
            (&self.extra.sym_device, tr("otomatik")),
            (&self.extra.sym_comment, tr("Açıklama")),
            (&self.extra.sym_init, tr("Başlangıç")),
        ] {
            input.update(cx, |state, cx| state.set_placeholder(text, window, cx));
        }
        self.extra.network_comment.update(cx, |state, cx| {
            state.set_placeholder(tr("İlk satır ağ başlığı, sonraki satırlar açıklama"), window, cx)
        });
        for (input, text) in placeholders {
            input.update(cx, |state, cx| state.set_placeholder(text, window, cx));
        }
        cx.refresh_windows();
        cx.notify();
    }
    /// Move the active tab by `step` positions, wrapping around the strip.
    fn cycle_tab(&mut self, step: isize) {
        let count = self.open_tabs.len() as isize;
        if count == 0 {
            return;
        }
        let current = self.open_tabs.iter().position(|t| *t == self.tab).unwrap_or(0) as isize;
        self.tab = self.open_tabs[(current + step).rem_euclid(count) as usize];
    }
    /// Close every closable tab except `keep`, which becomes active.
    fn close_others(&mut self, keep: Tab) {
        self.open_tabs.retain(|t| *t == keep || !t.closable());
        self.tab = keep;
    }
    /// Close the closable tabs to the right of `anchor`.
    fn close_right(&mut self, anchor: Tab) {
        if let Some(index) = self.open_tabs.iter().position(|t| *t == anchor) {
            let mut i = 0;
            self.open_tabs.retain(|t| {
                i += 1;
                i <= index + 1 || !t.closable()
            });
            if !self.open_tabs.contains(&self.tab) {
                self.tab = anchor;
            }
        }
    }
    /// Close every closable tab and return to the ladder.
    fn close_all_tabs(&mut self) {
        self.open_tabs.retain(|t| !t.closable());
        if !self.open_tabs.contains(&self.tab) {
            self.tab = Tab::Ladder;
        }
    }
    /// Close a document tab; if it was active, move to the tab on its left.
    fn close_tab(&mut self, tab: Tab) {
        if !tab.closable() {
            return;
        }
        if let Some(index) = self.open_tabs.iter().position(|t| *t == tab) {
            self.open_tabs.remove(index);
            if self.tab == tab {
                self.tab = self
                    .open_tabs
                    .get(index.saturating_sub(1))
                    .copied()
                    .unwrap_or(Tab::Ladder);
            }
        }
    }
}
impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open_tabs.contains(&self.tab) {
            self.open_tabs.push(self.tab);
        }
        self.sync_widgets(cx);
        if self.tab == Tab::Recipes { self.sync_recipes(window, cx); }
        self.sync_config(window, cx);
        if self.selected_sync {
            self.sync_network_fields(window, cx);
        }
        if self.close_requested
            && !self.busy
            && !self.running
            && (!self.unsaved() || self.close_discard)
        {
            window.remove_window();
        }
        if self.editor_sync {
            self.editor_sync = false;
            let source = self.active_source().to_owned();
            self.editor
                .update(cx, |input, cx| input.set_value(source, window, cx));
            self.shown_mark = None;
        }
        let language = self.project.program_language(self.active_program);
        if self.editor.read(cx).language_name().as_ref() != source_editor::language_name(language) {
            self.editor.update(cx, |editor, cx| source_editor::configure(editor, language, cx));
        }
        if self.unit_sync {
            self.unit_sync = false;
            let (name, number) = match self.active_program {
                0 => (String::new(), String::new()),
                i => {
                    let unit = &self.project.units[i - 1];
                    (unit.name.clone(), unit.number.to_string())
                }
            };
            self.extra
                .unit_name
                .update(cx, |input, cx| input.set_value(name, window, cx));
            self.extra
                .unit_number
                .update(cx, |input, cx| input.set_value(number, window, cx));
            let alloc = self.project.alloc;
            let retain = self.project.retain;
            for (input, value) in [
                (&self.extra.alloc_d_lo, alloc.d.0),
                (&self.extra.alloc_d_hi, alloc.d.1),
                (&self.extra.alloc_m_lo, alloc.m.0),
                (&self.extra.alloc_m_hi, alloc.m.1),
                (&self.extra.retain_d_lo, retain.d.0),
                (&self.extra.retain_d_hi, retain.d.1),
                (&self.extra.retain_m_lo, retain.m.0),
                (&self.extra.retain_m_hi, retain.m.1),
            ] {
                input.update(cx, |state, cx| state.set_value(value.to_string(), window, cx));
            }
        }
        if let Some(line) = self.jump_line.take() {
            let position = Position::new(line.saturating_sub(1) as u32, 0);
            self.editor
                .update(cx, |editor, cx| editor.set_cursor_position(position, window, cx));
        }
        self.sync_compile_mark(cx);
        let connected = self.bridge.is_some() || self.extra.hardware.is_some();
        let sim = self.sim_name;
        let mode_text = if self.extra.hardware.is_some() {
            if self.hardware_stale() {
                tr("PLC · veri eski").to_string()
            } else {
                tr("PLC · bağlı").to_string()
            }
        } else if self.running {
            if self.displayed_stale {
                tfmt!("{sim} · veri eski", "{sim} · stale data")
            } else {
                format!("{sim} · RUN")
            }
        } else if self.bridge.is_some() {
            format!("{sim} · STOP")
        } else if self.local_running {
            tr("Yerel · RUN").to_string()
        } else {
            tr("Çevrimdışı").to_string()
        };
        let mode = SharedString::from(mode_text);
        let title = if self.project.units.is_empty()
            || !matches!(self.tab, Tab::Ladder | Tab::Source)
        {
            self.tab.title().to_owned()
        } else {
            format!(
                "{}  /  {}",
                self.project.program_name(self.active_program),
                self.tab.title()
            )
        };
        let online = connected;
        let cpu_running = self.running || self.local_running;
        // Mode badge: green RUN, amber STOP, grey offline; orange marks an online session.
        let (badge_text, badge_bg) = if online {
            if cpu_running {
                ("RUN", GREEN)
            } else {
                ("STOP", AMBER)
            }
        } else if self.local_running {
            (tr("RUN · yerel"), GREEN)
        } else {
            (tr("ÇEVRİMDIŞI"), 0x4b5866)
        };
        // The dark application bar doubles as the window title bar: the left part drags
        // the window, the buttons on the right are the system window controls.
        let control = |id: &'static str, icon: IconName, area: WindowControlArea| {
            let close = area == WindowControlArea::Close;
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .w(px(46.))
                .h_full()
                .flex_shrink_0()
                .text_color(rgb(CHROME_TEXT))
                .hover(move |style| {
                    if close {
                        style.bg(rgb(0xe81123)).text_color(rgb(0xffffff))
                    } else {
                        style.bg(rgb(0x2a3846))
                    }
                })
                .window_control_area(area)
                .child(Icon::new(icon).size(px(14.)))
        };
        let window_controls = div()
            .flex()
            .h_full()
            .flex_shrink_0()
            .child(control(
                "window-minimize",
                IconName::WindowMinimize,
                WindowControlArea::Min,
            ))
            .child(control(
                "window-maximize",
                if window.is_maximized() {
                    IconName::WindowRestore
                } else {
                    IconName::WindowMaximize
                },
                WindowControlArea::Max,
            ))
            .child(control(
                "window-close",
                IconName::WindowClose,
                WindowControlArea::Close,
            ));
        let header = div()
            .h(px(40.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .pl_4()
            .bg(rgb(CHROME))
            .text_color(rgb(CHROME_TEXT))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .flex_1()
                    .h_full()
                    .window_control_area(WindowControlArea::Drag)
                    .child(
                        div()
                            .size(px(22.))
                            .rounded(px(5.))
                            .bg(rgb(ACCENT))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xffffff))
                            .child("D"),
                    )
                    .child(div().font_weight(FontWeight::BOLD).child("OpenDVP"))
                    .child(div().w(px(1.)).h(px(18.)).bg(rgb(0x3a4856)))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(format!(
                        "{}{}",
                        self.project.name,
                        if self.dirty { " •" } else { "" }
                    )))
                    .child(
                        div()
                            .px_2()
                            .py(px(1.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(rgb(0x3a4856))
                            .text_xs()
                            .text_color(rgb(CHROME_MUTED))
                            .child(format!(
                                "{} · {}",
                                opendvp::devices::find(&self.project.device)
                                    .map_or(self.project.device.as_str(), |m| m.title),
                                self.project.language.label()
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .pr_3()
                    .text_xs()
                    .when(!self.forced.is_empty(), |row| {
                        row.child(
                            div()
                                .px_2()
                                .py(px(2.))
                                .rounded(px(4.))
                                .bg(rgb(0xdc2626))
                                .text_color(rgb(0xffffff))
                                .font_weight(FontWeight::BOLD)
                                .child(tfmt!("ZORLAMA · {}", "FORCE · {}", self.forced.len())),
                        )
                    })
                    .child({
                        let dark = opendvp::settings::get().dark;
                        div()
                            .id("theme-toggle")
                            .px_2()
                            .py(px(2.))
                            .rounded(px(4.))
                            .cursor_pointer()
                            .text_color(rgb(CHROME_MUTED))
                            .hover(|style| style.bg(rgb(0x24313d)))
                            .tooltip(move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(if dark { tr("Açık tema") } else { tr("Koyu tema") })
                                    .build(window, cx)
                            })
                            .child(
                                gpui_component::Icon::new(if dark {
                                    gpui_component::IconName::Sun
                                } else {
                                    gpui_component::IconName::Moon
                                })
                                .size_4(),
                            )
                            .on_click(cx.listener(|_, _, _, cx| {
                                let on = !opendvp::settings::get().dark;
                                opendvp::settings::update(|s| s.dark = on);
                                super::colors::apply(on, cx);
                                cx.refresh_windows();
                                cx.notify();
                            }))
                    })
                    .child(
                        div()
                            .id("open-settings")
                            .px_2()
                            .py(px(2.))
                            .rounded(px(4.))
                            .cursor_pointer()
                            .text_color(rgb(CHROME_MUTED))
                            .hover(|style| style.bg(rgb(0x24313d)))
                            .tooltip(|window, cx| gpui_component::tooltip::Tooltip::new(tr("Ayarlar")).build(window, cx))
                            .child(gpui_component::Icon::new(gpui_component::IconName::Settings).size_4())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.tab = Tab::Settings;
                                cx.notify();
                            })),
                    )
                    .when(online, |row| {
                        row.child(div().text_color(rgb(CHROME_MUTED)).child(mode.clone()))
                    })
                    .when(online, |row| {
                        row.child(
                            div()
                                .px_2()
                                .py(px(2.))
                                .rounded(px(4.))
                                .bg(rgb(ONLINE))
                                .text_color(rgb(0x1d1205))
                                .font_weight(FontWeight::BOLD)
                                .child(tr("ÇEVRİMİÇİ")),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py(px(2.))
                            .rounded(px(4.))
                            .bg(rgb(badge_bg))
                            .text_color(rgb(0xffffff))
                            .font_weight(FontWeight::BOLD)
                            .child(div().size(px(6.)).rounded_full().bg(rgb(0xffffff)))
                            .child(badge_text),
                    ),
            )
            .child(window_controls);
        let tool = |id: &'static str, icon: IconName, label: &'static str, disabled: bool| {
            Button::new(id)
                .icon(icon)
                .label(label)
                .ghost()
                .small()
                .flex_shrink_0()
                .disabled(disabled)
        };
        let group = |_caption: &'static str, buttons: Vec<AnyElement>| {
            div()
                .flex()
                .items_center()
                .px_2()
                .child(div().flex().items_center().gap_1().children(buttons))
        };
        let divider = || div().w(px(1.)).h(px(22.)).bg(rgb(LINE));
        let step_disabled = self.running
            || self.local_running
            || self.busy
            || self.program.is_none()
            || self.extra.hardware.is_some()
            || (self.bridge.is_some() && (!self.uploaded || self.online_used));
        let run_disabled = self.busy
            || self.running
            || self.local_running
            || self.program.is_none()
            || self.extra.hardware.is_some()
            || (self.bridge.is_some() && !self.uploaded);
        let stop_disabled =
            (!self.running && !self.local_running && !self.busy) || self.extra.hardware.is_some();
        let toolbar = div()
            .min_h(px(42.))
            .flex_shrink_0()
            .flex()
            .flex_wrap()
            .items_center()
            .py_1()
            .px_2()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(LINE))
            .when(online, |bar| bar.border_t_2().border_color(rgb(ONLINE)))
            .child(group(
                tr("Proje"),
                vec![
                    tool(
                        "new",
                        IconName::Plus,
                        tr("Yeni"),
                        self.editing_locked() || connected,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.new_project(cx)))
                    .into_any_element(),
                    tool(
                        "open",
                        IconName::FolderOpen,
                        tr("Aç"),
                        self.editing_locked() || connected,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.file_open(cx)))
                    .into_any_element(),
                    tool(
                        "save",
                        IconName::Inbox,
                        tr("Kaydet"),
                        self.busy && !self.running,
                    )
                    .tooltip(tr("Kaydet · Ctrl+S"))
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx)))
                    .into_any_element(),
                    tool("report", IconName::BookOpen, tr("Belge"), self.busy)
                        .tooltip(tr("Yazdırılabilir proje belgesi (HTML · tarayıcıdan PDF)"))
                        .on_click(cx.listener(|this, _, _, cx| this.export_report(cx)))
                        .into_any_element(),
                ],
            ))
            .child(divider())
            .child(group(
                tr("Derleme"),
                vec![
                    tool(
                        "compile",
                        IconName::CircleCheck,
                        tr("Derle"),
                        self.editing_locked(),
                    )
                    .tooltip(tr("Programı derle · F7"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.compile();
                        this.log(
                            this.compile_error
                                .clone()
                                .unwrap_or_else(|| tr("Derleme başarılı.").into()),
                        );
                        cx.notify();
                    }))
                    .into_any_element(),
                ],
            ))
            .child(divider())
            .child(group(
                tr("Simülasyon"),
                vec![
                    tool("reset", IconName::Undo, tr("Sıfırla"), connected || self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.local_running = false;
                            this.simulator.restart_preserving_file_store();
                            cx.notify();
                        }))
                        .into_any_element(),
                    tool("step", IconName::ChevronRight, tr("Tek tarama"), step_disabled)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(bridge) = this.bridge.clone() {
                                let inputs = std::mem::take(&mut this.pending_inputs)
                                    .into_iter()
                                    .collect();
                                this.dispatch(
                                    async move { Done::Step(bridge.step(inputs).await) },
                                    cx,
                                );
                            } else if let Some(program) = &this.program {
                                this.simulator.scan(program, opendvp::settings::get().scan_ms);
                                this.capture_trend(cx);
                                cx.notify();
                            }
                        }))
                        .into_any_element(),
                ],
            ))
            .child(div().flex_1())
            .child(divider())
            .child(group(
                tr("Çevrimiçi"),
                vec![
                    tool(
                        "connect",
                        IconName::Globe,
                        if connected {
                            tr("Bağlantıyı kes")
                        } else {
                            tr("Simülatöre bağlan")
                        },
                        self.running || self.busy,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.extra.hardware.is_some() {
                            this.disconnect_hardware();
                            cx.notify();
                        } else if this.bridge.is_some() {
                            this.disconnect(cx);
                        } else {
                            this.connect(cx);
                        }
                    }))
                    .into_any_element(),
                    tool(
                        "write",
                        IconName::ArrowDown,
                        tr("Yaz"),
                        self.bridge.is_none()
                            || self.busy
                            || self.running
                            || self.program.is_none(),
                    )
                    .tooltip(tr("Programı SS2 simülatörüne yaz"))
                    .on_click(cx.listener(|this, _, _, cx| this.upload(cx)))
                    .into_any_element(),
                    tool(
                        "read",
                        IconName::ArrowUp,
                        tr("Oku"),
                        self.bridge.is_none() || self.busy || self.running,
                    )
                    .tooltip(tr("Programı SS2 simülatöründen oku"))
                    .on_click(cx.listener(|this, _, _, cx| this.read(cx)))
                    .into_any_element(),
                ],
            ))
            .child(divider())
            .child(group(
                "CPU",
                vec![
                    Button::new("run")
                        .label(if connected { "RUN" } else { tr("Yerel RUN") })
                        .success()
                        .small()
                        .disabled(run_disabled)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.bridge.is_some() {
                                this.run(cx);
                            } else {
                                this.local_running = true;
                                cx.notify();
                            }
                        }))
                        .into_any_element(),
                    Button::new("stop")
                        .label("STOP")
                        .warning()
                        .small()
                        .disabled(stop_disabled)
                        .tooltip("STOP · Shift+F5")
                        .on_click(cx.listener(|this, _, _, cx| this.stop(cx)))
                        .into_any_element(),
                ],
            ));
        let content = match self.tab {
            Tab::Ladder => self.ladder(cx),
            Tab::NewProject => self.new_project_view(cx),
            Tab::Hardware => self.hardware_view(cx),
            Tab::CrossRef => self.xref_view(cx),
            Tab::Watch => self.watch_view(cx),
            Tab::Diff => self.diff_view(cx),
            Tab::Source => self.source(cx),
            Tab::Comments => self.comments_view(cx),
            Tab::Symbols => self.symbols_view(cx),
            Tab::Search => self.search(cx),
            Tab::Connection => self.physical_view(cx),
            Tab::Modules => self.modules_view(cx),
            Tab::FileMemory => self.memory_view(cx),
            Tab::Io => self.io_view(cx),
            Tab::Trend => self.trend_view(cx),
            Tab::Comparison => self.comparison_view(),
            Tab::Help => self.library_view(cx),
            Tab::StLibrary => self.st_library_view(cx),
            Tab::Settings => self.settings_view(cx),
            Tab::Recipes => self.recipes_view(cx),
            Tab::Log => div()
                .id("log-scroll")
                .flex_1()
                .overflow_y_scroll()
                .p_4()
                .children(self.logs.iter().map(|text| {
                    div()
                        .py_2()
                        .border_b_1()
                        .border_color(rgb(LINE))
                        .child(text.clone())
                }))
                .into_any_element(),
        };
        let selected = self.open_tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
        let targets = self.open_tabs.clone();
        let labels: Vec<String> = targets.iter().map(|t| self.tab_text(*t)).collect();
        let view = cx.entity();
        let closed: Vec<Tab> = Tab::ALL
            .into_iter()
            .filter(|t| !self.open_tabs.contains(t))
            .collect();
        let add_button = Button::new("add-tab")
            .icon(IconName::Plus)
            .ghost()
            .small()
            .tooltip(tr("Sayfa aç"))
            .dropdown_menu(move |menu, _, _| {
                if closed.is_empty() {
                    return menu.label(tr("Tüm sayfalar açık"));
                }
                closed.iter().fold(menu, |menu, tab| {
                    let tab = *tab;
                    let view = view.clone();
                    menu.item(
                        PopupMenuItem::new(tab.title())
                            .icon(tab.icon())
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.tab = tab;
                                    cx.notify();
                                });
                            }),
                    )
                })
            });
        let documents = TabBar::new("documents")
            .underline()
            .small()
            .menu(true)
            .suffix(add_button)
            .selected_index(selected)
            .children(targets.iter().enumerate().map(|(index, tab)| {
                let tab = *tab;
                let menu_view = cx.entity();
                let label = div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(Icon::new(tab.icon()).size(px(14.)))
                    .child(labels[index].clone())
                    .context_menu(move |menu, _, _| {
                        let act = |label: &'static str, f: fn(&mut Workbench, Tab)| {
                            let view = menu_view.clone();
                            PopupMenuItem::new(label).on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    f(this, tab);
                                    cx.notify();
                                });
                            })
                        };
                        menu.item(act(tr("Kapat"), Workbench::close_tab).disabled(!tab.closable()))
                            .item(act(tr("Diğerlerini kapat"), Workbench::close_others))
                            .item(act(tr("Sağdakileri kapat"), Workbench::close_right))
                            .item(PopupMenuItem::separator())
                            .item(act(tr("Tümünü kapat"), |this, _| this.close_all_tabs()))
                    });
                let item = KitTab::new()
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |this, _, _, cx| {
                            this.close_tab(tab);
                            cx.notify();
                        }),
                    );
                if tab.closable() {
                    item.suffix(
                        Button::new(SharedString::from(format!("close-tab-{index}")))
                            .icon(IconName::Close)
                            .ghost()
                            .xsmall()
                            .tooltip(tr("Sekmeyi kapat"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_tab(tab);
                                cx.notify();
                            })),
                    )
                } else {
                    item
                }
            }))
            .on_click(cx.listener(move |this, index, _, cx| {
                if let Some(tab) = targets.get(*index)
                    && this.open_tabs.contains(tab)
                {
                    this.tab = *tab;
                    cx.notify();
                }
            }));
        let program_bar = self.program_bar(cx);
        let layout = opendvp::settings::get();
        let show_programs =
            matches!(self.tab, Tab::Ladder | Tab::Source);
        let center = div()
            .size_full()
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .flex_shrink_0()
                    .bg(rgb(PANEL))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .pl_2()
                    .child(documents),
            )
            .when(show_programs, |column| column.child(program_bar))
            .child(
                div()
                    .h(px(30.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(0xf8fafc))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(format!("{}  /  {}", self.project.name, title)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                Button::new("symbol-names")
                                    .label(if self.extra.show_symbols {
                                        tr("Adlar")
                                    } else {
                                        tr("Adresler")
                                    })
                                    .ghost()
                                    .xsmall()
                                    .tooltip(tr("Sembol adlarını / cihaz adreslerini göster"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.extra.show_symbols = !this.extra.show_symbols;
                                        cx.notify();
                                    })),
                            )
                            .when(self.tab == Tab::Ladder, |row| row.child(
                                Button::new("toggle-inspector")
                                    .label(tr("Alt panel"))
                                    .tooltip(tr("Alt paneli göster / gizle · Ctrl+J"))
                                    .ghost()
                                    .xsmall()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.ui.inspector_open = !this.ui.inspector_open;
                                        opendvp::settings::update(|s| s.inspector_open = this.ui.inspector_open);
                                        cx.notify();
                                    })),
                            ))
                            .child(
                                Button::new("focus-workspace")
                                    .icon(IconName::PanelRight)
                                    .label(if self.ui.focus_mode { tr("Düzeni geri getir") } else { tr("Odak modu") })
                                    .tooltip(tr("Odak modu · Ctrl+Shift+Enter"))
                                    .ghost()
                                    .xsmall()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.ui.focus_mode = !this.ui.focus_mode;
                                        opendvp::settings::update(|s| s.focus_mode = this.ui.focus_mode);
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(if self.tab == Tab::Ladder && self.ui.inspector_open && !self.ui.focus_mode {
                        v_resizable("ladder-workspace")
                            .on_resize(|state, _, cx| {
                                if let Some(size) = state.read(cx).sizes().get(1) {
                                    let height = f32::from(*size).round() as u16;
                                    opendvp::settings::update(|s| s.panel_bottom_height = height.clamp(120, 390));
                                }
                            })
                            .child(
                                resizable_panel()
                                    .size_range(px(170.)..px(2000.))
                                    .child(content),
                            )
                            .child(
                                resizable_panel()
                                    .size(px(f32::from(layout.panel_bottom_height)))
                                    .size_range(px(120.)..px(390.))
                                    .child(self.bottom_panel(cx)),
                            )
                            .into_any_element()
                    } else {
                        content
                    }),
            );
        let inspector =
            matches!(self.tab, Tab::Ladder | Tab::Source | Tab::Io) && !self.ui.focus_mode;
        // The right-hand panel lives inside the centre panel so that showing or hiding it
        // never redistributes width to the project explorer.
        let side = if self.tab == Tab::Ladder {
            self.ladder_palette(cx)
        } else {
            self.inspector_view(cx)
        };
        let document = h_resizable("document-workspace")
            .on_resize(move |state, _, cx| {
                if inspector && let Some(size) = state.read(cx).sizes().get(1) {
                    let width = f32::from(*size).round() as u16;
                    opendvp::settings::update(|s| s.panel_tools_width = width.clamp(265, 480));
                }
            })
            .child(
                resizable_panel()
                    .size_range(px(300.)..px(4000.))
                    .child(center),
            )
            .child(
                resizable_panel()
                    .size(px(f32::from(layout.panel_tools_width)))
                    .size_range(px(265.)..px(480.))
                    .visible(inspector)
                    .child(side),
            );
        let workspace = div().flex_1().min_h_0().child(
            h_resizable("main-workspace")
                .on_resize(|state, _, cx| {
                    if let Some(size) = state.read(cx).sizes().first() {
                        let width = f32::from(*size).round() as u16;
                        if width >= 140 {
                            opendvp::settings::update(|s| s.panel_tree_width = width.clamp(140, 420));
                        }
                    }
                })
                .child(
                    resizable_panel()
                        .size(px(f32::from(layout.panel_tree_width)))
                        .size_range(px(140.)..px(420.))
                        .visible(!self.ui.focus_mode)
                        .child(self.project_tree()),
                )
                .child(
                    resizable_panel()
                        .size_range(px(500.)..px(4000.))
                        .child(document),
                ),
        );
        let status = self
            .compile_error
            .clone()
            .unwrap_or_else(|| self.status.clone());
        // The header sits outside the focusable root: a focusable ancestor marks every
        // mouse-down as handled, which would stop the window-control buttons from working.
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family("Segoe UI")
            .text_sm()
            .child(header)
            .child(
        div()
            .id("workbench-root")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SaveProject, _, cx| this.save(cx)))
            .on_action(cx.listener(|this, _: &OpenProject, _, cx| this.file_open(cx)))
            .on_action(cx.listener(|this, _: &CompileProgram, _, cx| {
                if !this.editing_locked() {
                    this.compile();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &StopProgram, _, cx| this.stop(cx)))
            .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
                this.close_tab(this.tab);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NextTab, _, cx| {
                this.cycle_tab(1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PrevTab, _, cx| {
                this.cycle_tab(-1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ShowSearch, _, cx| {
                this.tab = Tab::Search;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleFocusMode, _, cx| {
                this.ui.focus_mode = !this.ui.focus_mode;
                opendvp::settings::update(|s| s.focus_mode = this.ui.focus_mode);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleInspector, _, cx| {
                this.ui.inspector_open = !this.ui.inspector_open;
                opendvp::settings::update(|s| s.inspector_open = this.ui.inspector_open);
                cx.notify();
            }))
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(workspace)
            .when(self.close_requested && self.unsaved(), |body| {
                body.child(
                    div()
                        .flex()
                        .gap_3()
                        .p_3()
                        .bg(rgb(0xfff4db))
                        .child(tr("Kaydedilmemiş proje / dosya belleği değişiklikleri"))
                        .child(
                            self.button("close-save", tr("Kaydet"), self.busy || self.running)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.dirty {
                                        this.save(cx);
                                    } else {
                                        this.memory_save(false, cx);
                                    }
                                })),
                        )
                        .child(
                            self.button(
                                "close-discard",
                                tr("Kaydetmeden kapat"),
                                self.busy || self.running,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_discard = true;
                                cx.notify();
                            })),
                        )
                        .child(
                            self.button("close-cancel", tr("Vazgeç"), false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_requested = false;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                div()
                    .h(px(26.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .gap_4()
                    .bg(rgb(if online { ONLINE_SOFT } else { PANEL }))
                    .border_t_1()
                    .border_color(rgb(if online { ONLINE } else { LINE }))
                    .text_xs()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_color(rgb(if self.compile_error.is_some() {
                                ERROR
                            } else {
                                TEXT
                            }))
                            .child(status),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .text_color(rgb(MUTED))
                            .child(tfmt!(
                                "{} ağ · {} komut", "{} networks · {} instructions",
                                self.program.as_ref().map_or(0, |p| p.networks.len()),
                                self.program.as_ref().map_or(0, |p| p.instructions)
                            ))
                            .when_some(self.capacity_text(), |row, (text, over)| {
                                row.child(div().w(px(1.)).h(px(12.)).bg(rgb(LINE))).child(
                                    div()
                                        .text_color(rgb(if over { ERROR } else { MUTED }))
                                        .child(text),
                                )
                            })
                            .child(div().w(px(1.)).h(px(12.)).bg(rgb(LINE)))
                            .child(mode.clone())
                            .child(div().w(px(1.)).h(px(12.)).bg(rgb(LINE)))
                            .child(self.timing_text()),
                    ),
            ),
            )
    }
}
