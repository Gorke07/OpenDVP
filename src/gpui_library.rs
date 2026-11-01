//! Instruction library in the style of the information system of PLC IDEs: a searchable
//! command list on the left, the selected command's syntax, variants and notes on the right.
use super::*;
use crate::instruction_help::GUIDE;
use crate::palette::{self, GROUPS, ITEMS, Item};

impl Workbench {
    pub(super) fn library_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self
            .extra
            .library_query
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let matches = |text: &str| query.is_empty() || text.to_lowercase().contains(&query);
        let mut list = div()
            .id("library-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.));
        let mut shown = 0;
        for &group in GROUPS {
            let items: Vec<&'static Item> = ITEMS
                .iter()
                .filter(|i| i.group == group)
                .filter(|i| matches(&format!("{} {} {}", i.mnemonic(), tr(i.name), i.template)))
                .collect();
            if items.is_empty() {
                continue;
            }
            list = list.child(section_title(tr(group)));
            for item in items {
                shown += 1;
                let selected =
                    self.extra.library_topic.is_none() && self.extra.library_item == Some(item);
                list = list.child(
                    div()
                        .id(SharedString::from(format!("lib-{}", item.template)))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .h(px(28.))
                        .rounded(px(4.))
                        .cursor_pointer()
                        .bg(rgb(if selected { 0xe3f3f4 } else { PANEL }))
                        .hover(|s| s.bg(rgb(0xf1f4f7)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.extra.library_item = Some(item);
                            this.extra.library_topic = None;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .w(px(64.))
                                .flex_shrink_0()
                                .font_family("Consolas")
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(if selected { ACCENT } else { TEXT }))
                                .child(item.mnemonic()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_sm()
                                .text_color(rgb(MUTED))
                                .child(tr(item.name)),
                        ),
                );
            }
        }
        let topics: Vec<(usize, &(&str, &str))> = GUIDE
            .iter()
            .enumerate()
            .filter(|(_, (title, text))| matches(title) || (!query.is_empty() && matches(text)))
            .collect();
        if !topics.is_empty() {
            list = list.child(section_title(tr("Ayrıntılı notlar")));
            for (index, (title, _)) in topics {
                shown += 1;
                let selected = self.extra.library_topic == Some(index);
                list = list.child(
                    div()
                        .id(("lib-topic", index))
                        .px_2()
                        .py_1()
                        .rounded(px(4.))
                        .cursor_pointer()
                        .text_sm()
                        .text_color(rgb(if selected { ACCENT } else { TEXT }))
                        .bg(rgb(if selected { 0xe3f3f4 } else { PANEL }))
                        .hover(|s| s.bg(rgb(0xf1f4f7)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.extra.library_topic = Some(index);
                            cx.notify();
                        }))
                        .child(*title),
                );
            }
        }
        if shown == 0 {
            list = list.child(
                div()
                    .p_3()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(tr("Eşleşen komut veya not yok.")),
            );
        }
        let left = div()
            .w(px(360.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .border_r_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .child(Input::new(&self.extra.library_query).small())
            .child(Self::heading(tfmt!(
                "{} komut · {} ayrıntılı not", "{} instructions · {} detailed notes",
                ITEMS.len(),
                GUIDE.len()
            )))
            .child(list);
        let right = match (self.extra.library_topic, self.extra.library_item) {
            (Some(index), _) => self.library_topic(index),
            (None, Some(item)) => self.library_item(item, cx),
            (None, None) => div().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .bg(rgb(BG))
            .child(left)
            .child(
                div()
                    .id("library-detail")
                    .flex_1()
                    .min_w_0()
                    .overflow_y_scroll()
                    .child(right),
            )
            .into_any_element()
    }

    fn library_item(&self, item: &'static Item, cx: &mut Context<Self>) -> AnyElement {
        let operands: Vec<&str> = item.template.split_whitespace().skip(1).collect();
        let forms = palette::variants(item);
        let topics = palette::related_topics(item);
        let example = if item.is_condition() {
            format!("{}\nOUT Y0\nEND", item.template)
        } else {
            format!("LD X0\n{}\nEND", item.template)
        };
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
        let mut syntax = card().child(card_title(tr("Sözdizimi"))).child(
            div()
                .font_family("Consolas")
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(item.template),
        );
        for (index, operand) in operands.iter().enumerate() {
            syntax = syntax.child(
                div()
                    .flex()
                    .gap_3()
                    .child(
                        div()
                            .w(px(90.))
                            .text_color(rgb(MUTED))
                            .child(tfmt!("Operand {}", "Operand {}", index + 1)),
                    )
                    .child(
                        div()
                            .w(px(80.))
                            .font_family("Consolas")
                            .child(operand.to_string()),
                    )
                    .child(div().child(palette::operand_role(operand))),
            );
        }
        if operands.is_empty() {
            syntax = syntax.child(Self::heading(tr("Operand almaz.")));
        }
        syntax = syntax.child(Self::heading(if item.is_condition() {
            tr("Koşul (kontak) komutu: rung'ın sol tarafına, seri veya paralel eklenir.")
        } else {
            tr("Çıkış / fonksiyon komutu: rung'ın sağ tarafında, koşul doğruyken çalışır.")
        }));
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
                            .min_w(px(72.))
                            .h(px(56.))
                            .px_2()
                            .rounded(px(6.))
                            .bg(rgb(CHROME))
                            .flex()
                            .items_center()
                            .justify_center()
                            .font_family("Consolas")
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(CHROME_TEXT))
                            .child(item.symbol),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(div().text_xl().font_weight(FontWeight::BOLD).child(format!(
                                "{} · {}",
                                item.mnemonic(),
                                tr(item.name)
                            )))
                            .child(Self::heading(tr(item.group))),
                    )
                    .child(
                        self.button("library-insert", tr("Ladder'a ekle"), !self.ladder_editable())
                            .primary()
                            .tooltip(
                                tr("Komutu seçer; ladder'da bir + noktasına tıklayarak yerleştirin"),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ladder_ui.tool = Some(item);
                                this.tab = Tab::Ladder;
                                this.log(tfmt!(
                                    "{} seçildi · ladder'da bir + noktasına tıklayın.", "{} selected · click a + point in the ladder.",
                                    item.mnemonic()
                                ));
                                cx.notify();
                            })),
                    ),
            )
            .child(syntax);
        if !forms.is_empty() {
            detail = detail.child(
                card()
                    .child(card_title(tr("Varyantlar")))
                    .child(
                        div().flex().flex_wrap().gap_2().children(forms.iter().map(|f| {
                            div()
                                .px_2()
                                .py(px(2.))
                                .rounded(px(4.))
                                .border_1()
                                .border_color(rgb(LINE))
                                .font_family("Consolas")
                                .text_sm()
                                .child(f.clone())
                        })),
                    )
                    .child(Self::heading(
                        tr("P: koşulun yalnız yükselen kenarında çalışır · D: 32 bit (çift kelime) işlem. Liste OpenDVP derleyicisinden alınır."),
                    )),
            );
        }
        detail = detail.child(
            card().child(card_title(tr("Örnek IL"))).child(
                div()
                    .p_3()
                    .rounded(px(4.))
                    .bg(rgb(0xf6f8fa))
                    .font_family("Consolas")
                    .text_sm()
                    .children(example.lines().map(|l| div().child(l.to_owned()))),
            ),
        );
        for index in topics {
            let (title, text) = GUIDE[index];
            detail = detail.child(
                card()
                    .child(card_title(title))
                    .child(div().text_sm().text_color(rgb(MUTED)).child(text)),
            );
        }
        detail.into_any_element()
    }

    fn library_topic(&self, index: usize) -> AnyElement {
        let (title, text) = GUIDE[index];
        div()
            .p_5()
            .child(
                div()
                    .p_4()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(PANEL))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_lg().font_weight(FontWeight::BOLD).child(title))
                    .child(div().text_color(rgb(TEXT)).child(text)),
            )
            .into_any_element()
    }
}

fn section_title(text: &str) -> Div {
    div()
        .mt_2()
        .px_2()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(MUTED))
        .child(text.to_uppercase())
}

fn card_title(text: &str) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.to_owned())
}
