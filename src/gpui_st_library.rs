//! ST standard library browser: searchable block list on the left, the selected block's
//! signature, documentation (in the UI language), parameters and an insertable template on
//! the right. "Insert" adds an instance declaration and a call to the active ST program.
use super::*;
use opendvp::st::{LibraryEntry, library_entries};

fn category_title(category: &str) -> &'static str {
    match category {
        "logic" => tr("Mantık ve zamanlama"),
        "math" => tr("Matematik ve dönüşüm"),
        "control" => tr("Kontrol"),
        "process" => tr("Proses ekipmanı"),
        _ => tr("Diğer"),
    }
}

impl Workbench {
    fn st_library_entries() -> &'static [LibraryEntry] {
        static ENTRIES: std::sync::OnceLock<Vec<LibraryEntry>> = std::sync::OnceLock::new();
        ENTRIES.get_or_init(library_entries)
    }

    /// Adds `entry` to the active program (ST only) and shows the inserted call.
    pub(super) fn insert_library_block(&mut self, entry: &LibraryEntry, cx: &mut Context<Self>) {
        if self.project.program_language(self.active_program) != SourceLanguage::St {
            self.log(tr("Kütüphane blokları yalnız ST programına eklenebilir."));
            cx.notify();
            return;
        }
        let (source, line) = entry.insert_into(self.active_source());
        self.commit_source(source, SourceLanguage::St, cx);
        self.compile();
        self.tab = Tab::Source;
        self.jump_line = Some(line);
        self.log(tfmt!(
            "{} programa eklendi (satır {line}); girişleri düzenleyin.",
            "{} was added to the program (line {line}); edit its inputs.",
            entry.name
        ));
        cx.notify();
    }

    pub(super) fn st_library_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let entries = Self::st_library_entries();
        let turkish = !opendvp::i18n::is_en();
        let query = self
            .extra
            .st_lib_query
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let matches = |e: &LibraryEntry| {
            query.is_empty()
                || e.name.to_lowercase().contains(&query)
                || e.doc_in(turkish).to_lowercase().contains(&query)
                || e.inputs
                    .iter()
                    .chain(&e.outputs)
                    .any(|p| p.name.to_lowercase().contains(&query))
        };
        let selected = self
            .extra
            .st_lib_selected
            .as_deref()
            .and_then(|n| entries.iter().find(|e| e.name == n))
            .or_else(|| entries.iter().find(|e| matches(e)));
        let mut list = div()
            .id("st-lib-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.));
        let mut shown = 0;
        let mut category = "";
        for e in entries.iter().filter(|e| matches(e)) {
            if e.category != category {
                category = e.category;
                list = list.child(
                    div()
                        .pt_2()
                        .pb_1()
                        .px_2()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(MUTED))
                        .child(category_title(category).to_uppercase()),
                );
            }
            shown += 1;
            let active = selected.is_some_and(|s| s.name == e.name);
            let name = e.name.clone();
            let summary = e
                .doc_in(turkish)
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned();
            list = list.child(
                div()
                    .id(SharedString::from(format!("st-lib-{}", e.name)))
                    .flex()
                    .flex_col()
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .bg(rgb(if active { 0xe3f3f4 } else { PANEL }))
                    .hover(|s| s.bg(rgb(0xf1f4f7)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.extra.st_lib_selected = Some(name.clone());
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .font_family("Consolas")
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(e.name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(if e.kind == "FUNCTION" { "FUN" } else { "FB" }),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .truncate()
                            .child(summary),
                    ),
            );
        }
        let left = div()
            .w(px(340.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .border_r_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .child(Input::new(&self.extra.st_lib_query).small())
            .child(Self::heading(tfmt!(
                "{shown} / {} blok · tanımlamadan kullanılır",
                "{shown} / {} blocks · usable without declaration",
                entries.len()
            )))
            .child(list);
        let right = match selected {
            Some(e) => self.st_library_detail(e, turkish, cx),
            None => div()
                .p_6()
                .text_color(rgb(MUTED))
                .child(tr("Aramaya uyan blok yok."))
                .into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .child(left)
            .child(
                div()
                    .id("st-lib-detail")
                    .flex_1()
                    .min_w_0()
                    .overflow_y_scroll()
                    .child(right),
            )
            .into_any_element()
    }

    fn st_library_detail(&self, e: &LibraryEntry, turkish: bool, cx: &mut Context<Self>) -> AnyElement {
        let card = || {
            div()
                .p_4()
                .rounded(px(6.))
                .border_1()
                .border_color(rgb(LINE))
                .bg(rgb(PANEL))
                .flex()
                .flex_col()
                .gap_2()
        };
        let mono = |text: String| div().font_family("Consolas").text_sm().child(text);
        let signature = match &e.returns {
            Some(t) => format!("FUNCTION {} : {t}", e.name),
            None => format!("FUNCTION_BLOCK {}", e.name),
        };
        let params = |title: &'static str, list: &[opendvp::st::LibraryParam]| {
            let mut c = card().child(div().font_weight(FontWeight::SEMIBOLD).child(title));
            if list.is_empty() {
                c = c.child(Self::heading(tr("Yok")));
            }
            for p in list {
                c = c.child(
                    div()
                        .flex()
                        .gap_3()
                        .child(
                            div()
                                .w(px(140.))
                                .font_family("Consolas")
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(p.name.clone()),
                        )
                        .child(div().font_family("Consolas").text_color(rgb(MUTED)).child(p.ty.clone())),
                );
            }
            c
        };
        let instance = format!("{}1", e.name.to_ascii_lowercase());
        let (decl, call) = e.template(&instance);
        let language = self.project.program_language(self.active_program);
        let entry = e.clone();
        let mut detail = div()
            .p_5()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .font_family("Consolas")
                                    .child(e.name.clone()),
                            )
                            .child(Self::heading(format!(
                                "{} · {}",
                                category_title(e.category),
                                if e.kind == "FUNCTION" { tr("Fonksiyon") } else { tr("Fonksiyon bloğu") }
                            ))),
                    )
                    .child(
                        self.button(
                            "st-lib-insert",
                            tr("Programa ekle"),
                            language != SourceLanguage::St || self.editing_locked(),
                        )
                        .primary()
                        .tooltip(tr("Etkin ST programına bir örnek bildirimi ve çağrısı ekler"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.insert_library_block(&entry, cx);
                        })),
                    ),
            )
            .child(card().child(mono(signature)))
            .child(
                card().child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .text_sm()
                        .children(e.doc_in(turkish).lines().map(|l| {
                            let heading = l.ends_with(':') && !l.starts_with('-');
                            div()
                                .when(l.starts_with('-'), |d| d.pl_3())
                                .when(heading, |d| d.pt_2().font_weight(FontWeight::SEMIBOLD))
                                .child(if l.is_empty() { " ".to_owned() } else { l.to_owned() })
                        })),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(div().flex_1().child(params(tr("Girişler"), &e.inputs)))
                    .child(div().flex_1().child(if e.returns.is_some() {
                        let mut c = card().child(div().font_weight(FontWeight::SEMIBOLD).child(tr("Dönüş değeri")));
                        c = c.child(mono(e.returns.clone().unwrap_or_default()));
                        c
                    } else {
                        params(tr("Çıkışlar"), &e.outputs)
                    })),
            )
            .child(
                card()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(tr("Şablon")))
                    .child(
                        div()
                            .p_3()
                            .rounded(px(4.))
                            .bg(rgb(0xf6f8fa))
                            .flex()
                            .flex_col()
                            .child(mono(format!("VAR  {decl}  END_VAR")))
                            .child(mono(call)),
                    ),
            );
        if language != SourceLanguage::St {
            detail = detail.child(Self::heading(tr(
                "Etkin program IL; kütüphane blokları ST programlarında kullanılır.",
            )));
        }
        detail.into_any_element()
    }
}
