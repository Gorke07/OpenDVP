//! Cross-reference view: devices with their read/write places; a click opens the network.
use super::*;

impl Workbench {
    pub(super) fn jump_to_network(&mut self, network: usize, cx: &mut Context<Self>) {
        // The cross reference numbers networks over the whole joined program.
        let (program, network) = self.network_home(network).unwrap_or((0, network));
        self.select_program(program, cx);
        self.selected = network;
        self.selected_sync = true;
        self.ladder_ui.selection = None;
        self.tab = Tab::Ladder;
        self.ladder_scroll.scroll_to_item(network);
        cx.notify();
    }

    pub(super) fn xref_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(program) = &self.program else {
            return div()
                .p_6()
                .text_color(rgb(MUTED))
                .child(tr("Çapraz referans için programın hatasız derlenmesi gerekir."))
                .into_any_element();
        };
        let uses = crate::xref::uses(program);
        let doubles = crate::xref::double_coils(program);
        let query = self.extra.xref_query.read(cx).value().trim().to_lowercase();
        let selected = self
            .extra
            .xref_device
            .filter(|d| uses.contains_key(d))
            .or_else(|| uses.keys().next().copied());
        let mut list = div()
            .id("xref-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .h(px(28.))
                    .items_center()
                    .bg(rgb(0xf3f5f8))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child(div().w(px(170.)).px_2().child(tr("Adres")))
                    .child(div().w(px(56.)).child(tr("Yazma")))
                    .child(div().w(px(56.)).child(tr("Okuma")))
                    .child(div().flex_1().child(tr("Açıklama"))),
            );
        for (device, entries) in &uses {
            let comment = self.project.comment(*device).unwrap_or_default();
            if !query.is_empty()
                && !device.to_string().to_lowercase().contains(&query)
                && !comment.to_lowercase().contains(&query)
            {
                continue;
            }
            let device = *device;
            let writes = entries.iter().filter(|u| u.write).count();
            let reads = entries.len() - writes;
            let active = selected == Some(device);
            let double = doubles.contains(&device);
            list = list.child(
                div()
                    .id(SharedString::from(format!("xref-{device}")))
                    .flex()
                    .items_center()
                    .h(px(28.))
                    .border_b_1()
                    .border_color(rgb(0xedf0f4))
                    .cursor_pointer()
                    .bg(rgb(if active { 0xe3f3f4 } else { PANEL }))
                    .hover(|s| s.bg(rgb(0xf1f4f7)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.extra.xref_device = Some(device);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(170.))
                            .px_2()
                            .font_family("Consolas")
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(if double { AMBER } else { TEXT }))
                            .truncate()
                            .child(self.device_title(device)),
                    )
                    .child(div().w(px(56.)).text_sm().child(writes.to_string()))
                    .child(div().w(px(56.)).text_sm().child(reads.to_string()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(comment.to_owned()),
                    ),
            );
        }
        let left = div()
            .w(px(420.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .border_r_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .child(Input::new(&self.extra.xref_query).small())
            .child(Self::heading(tfmt!(
                "{} cihaz · {} çoklu bobin", "{} devices · {} multiple coils",
                uses.len(),
                doubles.len()
            )))
            .child(list);
        let mut right = div().flex_1().min_w_0().p_4().flex().flex_col().gap_3();
        if let Some(device) = selected {
            let entries = &uses[&device];
            right = right.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_xl()
                            .font_family("Consolas")
                            .font_weight(FontWeight::BOLD)
                            .child(device.to_string()),
                    )
                    .child(
                        div().text_color(rgb(MUTED)).child(
                            self.project
                                .comment(device)
                                .unwrap_or(tr("Açıklama yok"))
                                .to_owned(),
                        ),
                    ),
            );
            if doubles.contains(&device) {
                right = right.child(
                    div()
                        .p_3()
                        .rounded(px(6.))
                        .bg(rgb(0xfff7e6))
                        .text_color(rgb(0xb45309))
                        .child(tfmt!(
                            "{device} birden fazla ağda OUT ile yazılıyor (çoklu bobin). Taramanın sonunda yalnız son ağın sonucu geçerli olur.", "{device} is written with OUT in more than one network (multiple coil). At the end of the scan only the result of the last network applies."
                        )),
                );
            }
            let mut table = div()
                .flex()
                .flex_col()
                .rounded(px(6.))
                .border_1()
                .border_color(rgb(LINE))
                .bg(rgb(PANEL))
                .child(
                    div()
                        .flex()
                        .h(px(28.))
                        .items_center()
                        .bg(rgb(0xf3f5f8))
                        .border_b_1()
                        .border_color(rgb(LINE))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(MUTED))
                        .child(div().w(px(80.)).px_3().child(tr("Ağ")))
                        .child(div().w(px(90.)).child(tr("IL satırı")))
                        .child(div().w(px(90.)).child(tr("Erişim")))
                        .child(div().flex_1().child(tr("Komut"))),
                );
            for (row, entry) in entries.iter().enumerate() {
                let network = entry.network;
                let title = program
                    .networks
                    .get(network)
                    .map(|n| n.comment.lines().next().unwrap_or_default().to_owned())
                    .unwrap_or_default();
                table = table.child(
                    div()
                        .id(("xref-use", row))
                        .flex()
                        .items_center()
                        .h(px(30.))
                        .border_b_1()
                        .border_color(rgb(0xedf0f4))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(0xf1f4f7)))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.jump_to_network(network, cx)),
                        )
                        .child(
                            div()
                                .w(px(80.))
                                .px_3()
                                .text_color(rgb(ACCENT))
                                .child(tfmt!("Ağ {}", "Network {}", network + 1)),
                        )
                        .child(div().w(px(90.)).text_sm().child(entry.line.to_string()))
                        .child(
                            div()
                                .w(px(90.))
                                .text_sm()
                                .text_color(rgb(if entry.write { AMBER } else { GREEN }))
                                .child(if entry.write { tr("Yazma") } else { tr("Okuma") }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .flex()
                                .gap_3()
                                .child(
                                    div()
                                        .font_family("Consolas")
                                        .text_sm()
                                        .child(entry.instruction.clone()),
                                )
                                .child(div().text_sm().text_color(rgb(MUTED)).child(title)),
                        ),
                );
            }
            right = right
                .child(table)
                .child(Self::heading(tr("Satıra tıklayınca ladder'da o ağ açılır.")));
        }
        div()
            .size_full()
            .flex()
            .bg(rgb(BG))
            .child(left)
            .child(
                div()
                    .id("xref-detail")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(right),
            )
            .into_any_element()
    }
}
