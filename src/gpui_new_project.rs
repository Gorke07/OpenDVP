//! New project: device selection, in the style of the "add new device" dialog of PLC IDEs.
use super::*;
use opendvp::devices::{FAMILIES, MODELS, PlcModel};

impl Workbench {
    pub(super) fn new_project_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = opendvp::devices::find(self.extra.new_model).unwrap_or(&MODELS[0]);
        let mut catalog = div()
            .id("device-catalog")
            .w(px(380.))
            .flex_shrink_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .border_r_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL));
        for &family in FAMILIES {
            catalog = catalog.child(
                div()
                    .mt_2()
                    .mb_1()
                    .px_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child(family.to_uppercase()),
            );
            for model in MODELS.iter().filter(|m| m.family == family) {
                catalog = catalog.child(self.device_card(model, model.id == chosen.id, cx));
            }
        }
        let spec = |label: &'static str, value: String| {
            div()
                .flex()
                .py_2()
                .border_b_1()
                .border_color(rgb(0xedf0f4))
                .child(
                    div()
                        .w(px(170.))
                        .flex_shrink_0()
                        .text_color(rgb(MUTED))
                        .child(label),
                )
                .child(div().flex_1().child(value))
        };
        let language = |id: &'static str, label: &'static str, st: bool| {
            let mut button = Button::new(id).label(label).small();
            if self.extra.new_st == st {
                button = button.primary();
            } else {
                button = button.outline();
            }
            button.on_click(cx.listener(move |this, _, _, cx| {
                this.extra.new_st = st;
                cx.notify();
            }))
        };
        let details = div()
            .flex_1()
            .min_w_0()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(device_tile(chosen.id, 72.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child(chosen.title),
                            )
                            .child(Self::heading(tfmt!("Delta DVP · {}", "Delta DVP · {}", chosen.family)))
                            .child(div().child(chosen.summary)),
                    ),
            )
            .child(
                div()
                    .p_4()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(PANEL))
                    .child(spec(
                        tr("Program kapasitesi"),
                        tfmt!(
                            "{} adım{}", "{} steps{}",
                            chosen.steps,
                            if chosen.steps_verified {
                                tr(" (simülatörden okundu)")
                            } else {
                                ""
                            }
                        ),
                    ))
                    .child(spec(tr("Giriş / çıkış"), chosen.io.into()))
                    .child(spec(tr("Haberleşme"), chosen.comm.into()))
                    .child(spec(
                        tr("Delta editor simülatörü"),
                        chosen
                            .simulator
                            .map_or(tr("Yok (yerel simülasyon kullanılır)").into(), |s| {
                                s.to_string()
                            }),
                    ))
                    .child(spec(
                        tr("Özel M/D tablosu"),
                        tfmt!("Delta {} sütunu", "Delta {} column", chosen.special_column),
                    )),
            )
            .child(
                div()
                    .p_4()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(PANEL))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(tr("Proje")))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().w(px(120.)).text_color(rgb(MUTED)).child(tr("Proje adı")))
                            .child(div().flex_1().child(Input::new(&self.extra.new_name))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().w(px(120.)).text_color(rgb(MUTED)).child(tr("Dil")))
                            .child(language("new-lang-il", tr("Ladder / IL"), false))
                            .child(language("new-lang-st", "Structured Text", true)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        self.button("new-create", tr("Projeyi oluştur"), false)
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                let name = this.extra.new_name.read(cx).value().to_string();
                                let (model, st) = (this.extra.new_model, this.extra.new_st);
                                this.create_project(model, &name, st, cx);
                            })),
                    )
                    .child(
                        self.button("new-cancel", tr("Vazgeç"), false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.tab = Tab::Ladder;
                                cx.notify();
                            })),
                    ),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(PANEL))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr("Yeni proje · cihaz seçimi")),
                    )
                    .child(Self::heading(
                        tr("Programın çalışacağı Delta DVP modelini seçin; model sonradan proje dosyasında saklanır."),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(catalog)
                    .child(details),
            )
            .into_any_element()
    }

    fn device_card(
        &self,
        model: &'static PlcModel,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("device-{}", model.id)))
            .flex()
            .items_center()
            .gap_3()
            .p_2()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(if selected { ACCENT } else { PANEL }))
            .bg(rgb(if selected { 0xe3f3f4 } else { PANEL }))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0xf1f4f7)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.extra.new_model = model.id;
                cx.notify();
            }))
            .child(device_tile(model.id, 44.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(model.title))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .truncate()
                            .child(model.summary),
                    ),
            )
            .when(model.simulator.is_some(), |card| {
                card.child(
                    div()
                        .px_2()
                        .py(px(1.))
                        .rounded(px(4.))
                        .bg(rgb(0xeaf6ef))
                        .text_xs()
                        .text_color(rgb(GREEN))
                        .child("SIM"),
                )
            })
            .into_any_element()
    }
}

/// Stylised PLC front: dark body, model label, a row of I/O LEDs.
fn device_tile(label: &str, size: f32) -> Div {
    div()
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .rounded(px(size / 8.))
        .bg(rgb(CHROME))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(size / 12.))
        .child(
            div()
                .text_size(px(size / 4.2))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(CHROME_TEXT))
                .child(label.to_owned()),
        )
        .child(div().flex().gap(px(size / 18.)).children((0..4).map(|i| {
            div().size(px(size / 12.)).rounded_full().bg(rgb(if i == 0 {
                0x22c55e
            } else {
                0x4b5866
            }))
        })))
}
