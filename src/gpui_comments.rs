//! Device comment table (symbol table) and network comment editing.
use super::*;

impl Workbench {
    /// Save the comment typed in the form for the device in the form.
    pub(super) fn apply_device_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let device_text = self.extra.comment_device.read(cx).value().to_string();
        let text = self.extra.comment_text.read(cx).value().to_string();
        match device_text.trim().to_uppercase().parse::<Device>() {
            Ok(device) => {
                self.project.set_comment(device, &text);
                self.dirty = true;
                self.log(if text.trim().is_empty() {
                    tfmt!("{device} açıklaması silindi.", "{device} description deleted.")
                } else {
                    tfmt!("{device} açıklaması kaydedildi.", "{device} description saved.")
                });
                self.extra
                    .comment_text
                    .update(cx, |input, cx| input.set_value("", window, cx));
            }
            Err(e) => self.log(tfmt!("Geçersiz cihaz adresi: {e}", "Invalid device address: {e}")),
        }
        cx.notify();
    }

    /// Fill the form with `device` and its comment and open the table.
    pub(super) fn edit_device_comment(
        &mut self,
        device: Device,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.project.comment(device).unwrap_or_default().to_owned();
        self.extra.comment_device.update(cx, |input, cx| {
            input.set_value(device.to_string(), window, cx)
        });
        self.extra
            .comment_text
            .update(cx, |input, cx| input.set_value(text, window, cx));
        self.tab = Tab::Comments;
        cx.notify();
    }

    /// Copy, duplicate, paste and move whole networks; copies go to the system clipboard as
    /// IL so they can also be pasted into the source editor.
    pub(super) fn network_command(&mut self, command: &str, cx: &mut Context<Self>) {
        if !self.ladder_editable() {
            return;
        }
        let (source, language, index) = (
            self.active_source().to_owned(),
            self.project.program_language(self.active_program),
            self.selected,
        );
        let compiler = self.model_compiler();
        let has_networks = self.view_program().is_some_and(|p| !p.networks.is_empty());
        let result = crate::gpui_model::scoped(compiler, || match command {
            "copy" => crate::gpui_model::copy_network(&source, language, index).map(|text| {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.log(tfmt!("Ağ {} panoya kopyalandı.", "Network {} copied to the clipboard.", index + 1));
                None
            }),
            "duplicate" => crate::gpui_model::copy_network(&source, language, index)
                .and_then(|text| {
                    crate::gpui_model::paste_networks(&source, language, Some(index), &text)
                })
                .map(|(s, n)| Some((s, index + n, tfmt!("Ağ {} çoğaltıldı.", "Network {} duplicated.", index + 1)))),
            "paste" => {
                let text = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .unwrap_or_default();
                let after = has_networks.then_some(index);
                crate::gpui_model::paste_networks(&source, language, after, &text).map(|(s, n)| {
                    let at = after.map_or(0, |i| i + 1);
                    Some((s, at + n - 1, tfmt!("{n} ağ yapıştırıldı.", "{n} networks pasted.")))
                })
            }
            "up" | "down" => {
                let up = command == "up";
                crate::gpui_model::move_network(&source, language, index, up).map(|s| {
                    let to = if up { index - 1 } else { index + 1 };
                    Some((s, to, tfmt!("Ağ {} taşındı.", "Network {} moved.", index + 1)))
                })
            }
            _ => Ok(None),
        });
        match result {
            Ok(Some((source, select, message))) => {
                self.commit_source(source, SourceLanguage::Il, cx);
                self.selected = select;
                self.selected_sync = true;
                self.ladder_scroll.scroll_to_item(select);
                self.log(message);
            }
            Ok(None) => {}
            Err(e) => self.log(e),
        }
        cx.notify();
    }

    pub(super) fn apply_network_comment(&mut self, cx: &mut Context<Self>) {
        let text = self.extra.network_comment.read(cx).value().to_string();
        let compiler = self.model_compiler();
        let edited = crate::gpui_model::scoped(compiler, || {
            crate::gpui_model::set_network_comment(
                self.active_source(),
                self.project.program_language(self.active_program),
                self.selected,
                &text,
            )
        });
        match edited {
            Ok(source) => {
                self.commit_source(source, SourceLanguage::Il, cx);
                self.selected_sync = true;
                self.log(tr("Ağ açıklaması güncellendi. Geri al kullanılabilir."));
            }
            Err(e) => self.log(e),
        }
        cx.notify();
    }

    /// Network comment editor shown in the properties panel when no element is selected.
    pub(super) fn network_comment_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let editable = self.ladder_editable();
        div()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(Self::heading(tfmt!(
                "Ağ {} açıklaması · ilk satır başlık, kalan satırlar açıklama", "Network {} comment · first line is the title, the rest is the description",
                self.selected + 1
            )))
            .child(Textarea::new(&self.extra.network_comment).disabled(!editable))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(
                        self.button("network-comment-apply", tr("Açıklamayı uygula"), !editable)
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.apply_network_comment(cx))),
                    )
                    .child(Self::heading(
                        tr("Kontak, bobin veya blok seçerek özelliklerini görebilirsiniz."),
                    )),
            )
            .into_any_element()
    }

    /// Symbol table: every commented device plus every device the program uses.
    pub(super) fn comments_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let filter = self.extra.comment_filter.read(cx).value().to_lowercase();
        let used: std::collections::BTreeSet<Device> = self
            .program
            .as_ref()
            .map(|p| p.devices.iter().copied().collect())
            .unwrap_or_default();
        let mut devices: std::collections::BTreeSet<Device> = used.clone();
        devices.extend(
            self.project
                .comments
                .keys()
                .filter_map(|k| k.parse::<Device>().ok()),
        );
        let cell = |w: f32| div().w(px(w)).flex_shrink_0().px_3();
        let mut table = div()
            .id("comment-table")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .mx_3()
            .mb_3()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(30.))
                    .bg(rgb(0xf3f5f8))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child(cell(100.).child(tr("Adres")))
                    .child(div().flex_1().px_3().child(tr("Açıklama")))
                    .child(cell(110.).child(tr("Programda")))
                    .child(cell(150.).child("")),
            );
        let mut shown = 0;
        for device in devices {
            let comment = self.project.comment(device).unwrap_or_default().to_owned();
            if !filter.is_empty()
                && !device.to_string().to_lowercase().contains(&filter)
                && !comment.to_lowercase().contains(&filter)
            {
                continue;
            }
            shown += 1;
            let in_use = used.contains(&device);
            table = table.child(
                div()
                    .id(SharedString::from(format!("comment-row-{device}")))
                    .flex()
                    .items_center()
                    .h(px(32.))
                    .border_b_1()
                    .border_color(rgb(0xedf0f4))
                    .hover(|s| s.bg(rgb(0xf6f8fa)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.edit_device_comment(device, window, cx)
                    }))
                    .child(
                        cell(100.)
                            .font_family("Consolas")
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(device.to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_3()
                            .truncate()
                            .text_color(rgb(if comment.is_empty() { MUTED } else { TEXT }))
                            .child(if comment.is_empty() {
                                "—".to_string()
                            } else {
                                comment.clone()
                            }),
                    )
                    .child(
                        cell(110.)
                            .text_xs()
                            .text_color(rgb(if in_use { GREEN } else { MUTED }))
                            .child(if in_use {
                                tr("kullanılıyor")
                            } else {
                                tr("kullanılmıyor")
                            }),
                    )
                    .child(
                        cell(150.).flex().gap_1().child(
                            Button::new(SharedString::from(format!("comment-del-{device}")))
                                .label(tr("Sil"))
                                .xsmall()
                                .ghost()
                                .disabled(comment.is_empty())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.project.set_comment(device, "");
                                    this.dirty = true;
                                    this.log(tfmt!("{device} açıklaması silindi.", "{device} description deleted."));
                                    cx.notify();
                                })),
                        ),
                    ),
            );
        }
        if shown == 0 {
            table = table.child(
                div()
                    .p_4()
                    .text_color(rgb(MUTED))
                    .child(tr("Eşleşen cihaz yok. Yukarıdaki formdan yeni açıklama ekleyin.")),
            );
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(
                div()
                    .px_4()
                    .pt_3()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr("Cihaz açıklamaları · sembol tablosu")),
                    )
                    .child(Self::heading(tfmt!(
                        "{} açıklama · ladder, giriş/çıkış ve izleme listesinde görünür. Satıra tıklayarak düzenleyin.", "{} comments · shown in the ladder, I/O and watch list. Click a row to edit it.",
                        self.project.comments.len()
                    ))),
            )
            .child(
                div()
                    .m_3()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(PANEL))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(110.))
                            .flex_shrink_0()
                            .child(Input::new(&self.extra.comment_device).small()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.extra.comment_text).small()),
                    )
                    .child(
                        self.button("comment-save", tr("Kaydet"), false)
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.apply_device_comment(window, cx)
                            })),
                    )
                    .child(
                        div()
                            .w(px(220.))
                            .flex_shrink_0()
                            .child(Input::new(&self.extra.comment_filter).small()),
                    ),
            )
            .child(table)
            .into_any_element()
    }
}
