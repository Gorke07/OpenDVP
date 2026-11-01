use super::*;
use crate::gpui_model::{ElementEdit, Placement, Target};
use gpui_component::{
    Icon, IconName,
    alert::Alert,
    tab::{Tab as KitTab, TabBar},
};

/// A palette entry; the catalog and its compile checks live in `palette.rs`.
pub(super) type Tool = &'static crate::palette::Item;
pub(super) struct LadderUi {
    pub selection: Option<(usize, Target)>,
    pub placement: Option<Placement>,
    pub tool: Option<Tool>,
    pub instruction: Entity<InputState>,
    pub(super) query: Entity<InputState>,
    pub feedback: Option<String>,
    /// Expanded palette groups (all groups open while searching).
    open_groups: std::collections::BTreeSet<&'static str>,
}
impl LadderUi {
    pub fn new(window: &mut Window, cx: &mut Context<Workbench>) -> Self {
        let instruction = cx.new(|cx| InputState::new(window, cx).default_value("LD X0"));
        let query = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Komut ara…")));
        cx.subscribe(&query, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        Self {
            selection: None,
            placement: None,
            tool: None,
            instruction,
            query,
            feedback: None,
            open_groups: [tr("Bit mantığı"), tr("Karşılaştırma"), tr("Zamanlayıcı / sayaç")]
                .into_iter()
                .collect(),
        }
    }
}
impl Workbench {
    pub(super) fn ladder_editable(&self) -> bool {
        self.topology_editable()
            && self.project.program_language(self.active_program) == SourceLanguage::Il
            && self
                .view_program()
                .is_some_and(|p| p.graphically_editable())
    }
    fn selected_symbol(&self) -> Option<crate::ladder_plot::Symbol> {
        let (net, target) = self.ladder_ui.selection?;
        crate::ladder_plot::network(self.view_program()?.networks.get(net)?)
            .symbols
            .into_iter()
            .find(|s| s.target == target)
    }
    pub(super) fn select_element(
        &mut self,
        net: usize,
        target: Target,
        placement: Option<Placement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected = net;
        self.selected_sync = true;
        self.ladder_ui.selection = Some((net, target));
        self.ladder_ui.placement = placement;
        self.ladder_ui.feedback = None;
        let text = match target {
            Target::Condition(i) => self
                .view_program()
                .and_then(|p| p.networks.get(net))
                .and_then(|n| crate::gpui_model::condition_at(&n.condition, i))
                .map(|e| {
                    let mut text = String::new();
                    crate::ladder_source::emit_expr(e, &mut text);
                    text.trim().to_owned()
                }),
            Target::Action(i) => self
                .view_program()
                .and_then(|p| p.networks.get(net))
                .and_then(|n| n.actions.get(i))
                .map(|a| a.label()),
        };
        if let Some(text) = text {
            self.ladder_ui
                .instruction
                .update(cx, |s, cx| s.set_value(text, window, cx));
        }
        if placement.is_some() && self.ladder_ui.tool.is_some() && self.ladder_editable() {
            self.insert_tool(window, cx);
        }
        cx.notify();
    }
    fn element_change(&mut self, op: ElementEdit, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ladder_editable() {
            return;
        }
        let Some((net, target)) = self.ladder_ui.selection else {
            return;
        };
        let next = match &op {
            ElementEdit::Replace(_) => Some(target),
            ElementEdit::Delete => None,
            ElementEdit::InsertContact(_, _, place) | ElementEdit::InsertCondition(_, place) => {
                Some(match target {
                    Target::Condition(i) => {
                        Target::Condition(i + usize::from(*place != Placement::Before))
                    }
                    Target::Action(_) => {
                        let n = &self.view_program().unwrap().networks[net];
                        Target::Condition(
                            crate::ladder_plot::network(n)
                                .symbols
                                .iter()
                                .filter(|s| matches!(s.target, Target::Condition(_)))
                                .count(),
                        )
                    }
                })
            }
            ElementEdit::InsertAction(_, place) => Some(match target {
                Target::Action(i) => Target::Action(i + usize::from(*place != Placement::Before)),
                Target::Condition(_) => {
                    Target::Action(self.view_program().unwrap().networks[net].actions.len())
                }
            }),
        };
        let compiler = self.model_compiler();
        let edited = crate::gpui_model::scoped(compiler, || {
            crate::gpui_model::edit_element(
                self.active_source(),
                self.project.program_language(self.active_program),
                net,
                target,
                op,
            )
        });
        match edited {
            Ok(source) => {
                self.commit_source(source, SourceLanguage::Il, cx);
                self.ladder_ui.tool = None;
                self.ladder_ui.placement = None;
                self.ladder_ui.feedback = None;
                if let Some(target) = next {
                    self.select_element(net, target, None, window, cx);
                }
                self.log(tr("Ladder değişikliği derlendi · geri al kullanılabilir."));
            }
            Err(error) => {
                self.ladder_ui.feedback = Some(error);
                cx.notify();
            }
        }
    }
    fn insert_tool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tool) = self.ladder_ui.tool else {
            return;
        };
        let Some(place) = self.ladder_ui.placement else {
            return;
        };
        let op = if tool.is_condition() {
            ElementEdit::InsertCondition(tool.template.into(), place)
        } else {
            ElementEdit::InsertAction(tool.template.into(), place)
        };
        self.element_change(op, window, cx);
    }
    pub(super) fn ladder_palette(&self, cx: &mut Context<Self>) -> AnyElement {
        let editable = self.ladder_editable();
        let tabs = TabBar::new("ladder-tools-tabs")
            .underline()
            .small()
            .selected_index(usize::from(self.ui.watch_mode))
            .child(KitTab::new().label(tr("Komutlar")))
            .child(KitTab::new().label(tr("İzleme")))
            .on_click(cx.listener(|t, i, _, cx| {
                t.ui.watch_mode = *i == 1;
                cx.notify();
            }));
        let mut body = div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .bg(rgb(PANEL))
            .child(div().p_3().flex_shrink_0().child(tabs));
        if self.ui.watch_mode {
            return body.child(self.inspector_view(cx)).into_any_element();
        }
        let query = self.ladder_ui.query.read(cx).value().to_lowercase();
        let mut tools = div()
            .id("ladder-palette-scroll")
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(Input::new(&self.ladder_ui.query).small());
        let searching = !query.trim().is_empty();
        for &group in crate::palette::GROUPS {
            let items: Vec<Tool> = crate::palette::ITEMS
                .iter()
                .filter(|item| item.group == group)
                .filter(|item| {
                    !searching
                        || format!("{} {} {}", item.symbol, tr(item.name), item.template)
                            .to_lowercase()
                            .contains(query.trim())
                })
                .collect();
            if items.is_empty() {
                continue;
            }
            let open = searching || self.ladder_ui.open_groups.contains(group);
            // Folder header, collapsible like the instruction cards of PLC IDEs.
            tools = tools.child(
                div()
                    .id(SharedString::from(format!("palette-group-{group}")))
                    .mt_2()
                    .px_1()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0xf1f4f7)))
                    .on_click(cx.listener(move |t, _, _, cx| {
                        if !t.ladder_ui.open_groups.remove(group) {
                            t.ladder_ui.open_groups.insert(group);
                        }
                        cx.notify();
                    }))
                    .child(
                        Icon::new(if open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(px(14.))
                        .text_color(rgb(MUTED)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(tr(group).to_uppercase()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(items.len().to_string()),
                    ),
            );
            if !open {
                continue;
            }
            for tool in items {
                // Instruction card row: symbol tile, name, IL mnemonic.
                let active = self.ladder_ui.tool == Some(tool);
                let row = div()
                    .id(SharedString::from(format!("palette-{}", tool.template)))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_2()
                    .h(px(34.))
                    .rounded(px(5.))
                    .border_1()
                    .border_color(rgb(if active { ACCENT } else { PANEL }))
                    .bg(rgb(if active { 0xe3f3f4 } else { PANEL }))
                    .when(editable, |row| {
                        row.cursor_pointer().hover(|style| style.bg(rgb(0xf1f4f7)))
                    })
                    .when(!editable, |row| row.opacity(0.5))
                    .child(
                        div()
                            .w(px(64.))
                            .h(px(24.))
                            .flex_shrink_0()
                            .rounded(px(4.))
                            .bg(rgb(if tool.is_condition() {
                                0xeef6f7
                            } else {
                                0xf3f5f8
                            }))
                            .border_1()
                            .border_color(rgb(LINE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .font_family("Consolas")
                            .text_xs()
                            .child(tool.symbol),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .child(tr(tool.name)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .font_family("Consolas")
                            .child(tool.mnemonic()),
                    );
                tools = tools.child(if editable {
                    row.on_click(cx.listener(move |t, _, w, cx| {
                        t.ladder_ui.tool = Some(tool);
                        t.ladder_ui.feedback = None;
                        if t.ladder_ui.placement.is_some() {
                            t.insert_tool(w, cx);
                        }
                        cx.notify();
                    }))
                    .into_any_element()
                } else {
                    row.into_any_element()
                });
            }
        }
        tools=tools.child(div().mt_4().text_xs().text_color(rgb(MUTED)).child(if !editable{tr("Online oturumda komut ekleme kilitli. Eleman seçerek değerlerini izleyebilirsiniz.")}else if self.ladder_ui.tool.is_some(){tr("Komut seçildi. Ladder’daki + noktasına tıklayın.")}else{tr("Ladder’da + noktası seçin, ardından komuta tıklayın. Paralel dal seçili koşula eklenir.")}));
        body = body.child(tools).child(
            div()
                .p_3()
                .border_t_1()
                .border_color(rgb(LINE))
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            self.button("network-add", tr("Yeni ağ"), !editable)
                                .icon(IconName::Plus)
                                .on_click(cx.listener(|t, _, _, cx| t.graph_edit(4, cx))),
                        )
                        .child(
                            self.button("network-paste", tr("Yapıştır"), !editable)
                                .tooltip(tr("Panodaki IL ağlarını seçili ağın altına ekle"))
                                .on_click(
                                    cx.listener(|t, _, _, cx| t.network_command("paste", cx)),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            self.button(
                                "element-undo",
                                tr("Geri al"),
                                !editable || self.undo.is_empty(),
                            )
                            .on_click(cx.listener(|t, _, _, cx| t.source_undo(false, cx))),
                        )
                        .child(
                            self.button(
                                "element-redo",
                                tr("Yinele"),
                                !editable || self.redo.is_empty(),
                            )
                            .on_click(cx.listener(|t, _, _, cx| t.source_undo(true, cx))),
                        ),
                ),
        );
        body.into_any_element()
    }
    pub(super) fn ladder_properties(&self, cx: &mut Context<Self>) -> AnyElement {
        let editable = self.ladder_editable();
        let symbol = self.selected_symbol();
        let mut body = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_t_1()
            .border_color(rgb(LINE));
        body = body.child(
            div()
                .h(px(32.))
                .flex_shrink_0()
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .bg(rgb(0xf5f7fa))
                .border_b_1()
                .border_color(rgb(LINE))
                .child(
                    div().font_weight(FontWeight::SEMIBOLD).child(
                        self.selected_symbol()
                            .map(|s| s.label.clone())
                            .unwrap_or_else(|| tr("Seçili eleman").into()),
                    ),
                )
                .child(Self::heading(
                    self.ladder_ui
                        .selection
                        .map(|(n, _)| tfmt!("Ağ {:02}", "Network {:02}", n + 1))
                        .unwrap_or_else(|| tr("Eleman seçilmedi").into()),
                )),
        );
        let Some(symbol) = symbol else {
            return body
                .child(self.network_comment_editor(cx))
                .into_any_element();
        };
        let mut edit = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(Self::heading(tr("Komut / operandlar")))
            .child(Input::new(&self.ladder_ui.instruction).disabled(!editable))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        self.button("element-apply", tr("Uygula"), !editable)
                            .primary()
                            .on_click(cx.listener(|t, _, w, cx| {
                                let text = t.ladder_ui.instruction.read(cx).value().to_string();
                                t.element_change(ElementEdit::Replace(text), w, cx);
                            })),
                    )
                    .child(
                        self.button("element-delete", tr("Elemanı kaldır"), !editable)
                            .on_click(cx.listener(|t, _, w, cx| {
                                t.element_change(ElementEdit::Delete, w, cx)
                            })),
                    )
                    .child(
                        self.button(
                            "element-parallel",
                            tr("Paralel dal"),
                            !editable || !matches!(symbol.target, Target::Condition(_)),
                        )
                        .on_click(cx.listener(|t, _, w, cx| {
                            t.element_change(
                                ElementEdit::InsertContact(
                                    "X0".parse().unwrap(),
                                    false,
                                    Placement::Parallel,
                                ),
                                w,
                                cx,
                            )
                        })),
                    ),
            );
        if let Some(error) = &self.ladder_ui.feedback {
            edit = edit.child(Alert::error("element-error", error.clone()).small());
        }
        let mut live = div()
            .w(px(265.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .pl_4()
            .border_l_1()
            .border_color(rgb(LINE));
        if let Some(device) = symbol.device {
            live = live
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .font_family("Consolas")
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(device.to_string()),
                        )
                        .child(Self::heading(if self.extra.hardware.is_some() {
                            tr("Fiziksel PLC")
                        } else if self.bridge.is_some() {
                            tr("Simülatör")
                        } else {
                            tr("Yerel simülasyon")
                        })),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(ACCENT))
                                .child(
                                    self.project
                                        .comment(device)
                                        .unwrap_or(tr("Açıklama yok"))
                                        .to_owned(),
                                ),
                        )
                        .child(
                            Button::new("element-comment")
                                .label(tr("Açıklamayı düzenle"))
                                .xsmall()
                                .ghost()
                                .on_click(cx.listener(move |t, _, w, cx| {
                                    t.edit_device_comment(device, w, cx)
                                })),
                        ),
                )
                .child(
                    div()
                        .font_family("Consolas")
                        .text_color(rgb(if self.observed_bit(device) == Some(true) {
                            GREEN
                        } else {
                            TEXT
                        }))
                        .child(self.device_value(device)),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            self.button(
                                "selected-watch",
                                tr("İzlemeye ekle"),
                                self.project.watch.contains(&device.to_string()),
                            )
                            .on_click(cx.listener(
                                move |t, _, _, cx| {
                                    let name = device.to_string();
                                    if !t.project.watch.contains(&name) {
                                        t.project.watch.push(name);
                                        t.dirty = true;
                                    }
                                    t.ui.watch_mode = true;
                                    cx.notify();
                                },
                            )),
                        )
                        .when(device.area == Area::X, |row| {
                            row.child(
                                self.button(
                                    "selected-input",
                                    tr("X değiştir"),
                                    self.extra.hardware.is_some()
                                        || (self.bridge.is_some() && !self.running),
                                )
                                .on_click(
                                    cx.listener(move |t, _, _, cx| t.toggle_input(device, cx)),
                                ),
                            )
                        }),
                );
        } else {
            live = live.child(Self::heading(tr("Bu koşulun tek bir cihaz değeri yok.")));
        }
        live = live.child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(if self.bridge.is_some() {
                    self.poll_ms
                        .map(|ms| tfmt!("Son okuma: {ms} ms · RUN: {}", "Last read: {ms} ms · RUN: {}", self.running))
                        .unwrap_or_else(|| tr("Henüz veri alınmadı").into())
                } else if editable {
                    tr("Düzenleme RAM’i sıfırlar; kaynak geri alınabilir.").into()
                } else {
                    tr("Düzenleme kilitli · canlı değerler okunuyor.").into()
                }),
        );
        body.child(
            div()
                .id("element-properties-scroll")
                .overflow_y_scroll()
                .flex_1()
                .min_h_0()
                .p_3()
                .flex()
                .gap_4()
                .child(edit)
                .child(live),
        )
        .into_any_element()
    }
}
