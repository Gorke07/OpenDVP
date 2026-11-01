//! Device configuration in the style of the device view of PLC IDEs: a rack with the CPU
//! and right-side extension modules, a hardware catalog and the selected slot's addresses.
use super::*;
use opendvp::hardware::{self, Assignment, CATALOG, GROUPS, Kind, Module};

const SLOT_W: f32 = 112.;
const RACK_H: f32 = 236.;

impl Workbench {
    fn hardware_changed(&mut self, message: String, cx: &mut Context<Self>) {
        self.dirty = true;
        self.log(message);
        cx.notify();
    }

    fn add_module(&mut self, module: &'static Module, cx: &mut Context<Self>) {
        let mut next = self.project.hardware.clone();
        let at = self
            .extra
            .hw_selected
            .map_or(next.len(), |i| (i + 1).min(next.len()));
        next.insert(at, module.name.to_string());
        match hardware::assign(&next) {
            Ok(_) => {
                self.project.hardware = next;
                self.extra.hw_selected = Some(at);
                self.hardware_changed(tfmt!("{} slot {}'e eklendi.", "{} added to slot {}.", module.name, at + 2), cx);
            }
            Err(e) => {
                self.log(e);
                cx.notify();
            }
        }
    }

    fn move_module(&mut self, index: usize, left: bool, cx: &mut Context<Self>) {
        let target = if left {
            index.checked_sub(1)
        } else {
            Some(index + 1).filter(|&t| t < self.project.hardware.len())
        };
        if let Some(target) = target {
            let mut next = self.project.hardware.clone();
            next.swap(index, target);
            if hardware::assign(&next).is_ok() {
                self.project.hardware = next;
                self.extra.hw_selected = Some(target);
                self.hardware_changed(tr("Modül sırası değişti; adresler yeniden atandı.").into(), cx);
            }
        }
    }

    pub(super) fn hardware_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let model = opendvp::devices::find(&self.project.device);
        let assignments = hardware::assign(&self.project.hardware).unwrap_or_default();
        let locked = self.editing_locked() || self.bridge.is_some();
        // Slot numbers above the rack, like the device view header.
        let mut numbers = div().flex().child(slot_number("1", 170.));
        let mut rack = div().flex().items_end().gap(px(2.)).child(self.cpu_tile(
            model,
            self.extra.hw_selected.is_none(),
            cx,
        ));
        for (index, name) in self.project.hardware.iter().enumerate() {
            let Some(module) = hardware::find(name) else {
                continue;
            };
            numbers = numbers.child(slot_number(&(index + 2).to_string(), SLOT_W + 2.));
            rack = rack.child(self.module_tile(
                index,
                module,
                assignments.get(index),
                self.extra.hw_selected == Some(index),
                cx,
            ));
        }
        for empty in
            self.project.hardware.len()..hardware::MAX_MODULES.min(self.project.hardware.len() + 3)
        {
            numbers = numbers.child(slot_number(&(empty + 2).to_string(), SLOT_W + 2.));
            rack = rack.child(
                div()
                    .w(px(SLOT_W))
                    .h(px(RACK_H - 20.))
                    .rounded(px(4.))
                    .border_1()
                    .border_dashed()
                    .border_color(rgb(0xb8c4d1))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x9aa8b7))
                    .text_xl()
                    .child("+"),
            );
        }
        let totals = summary(&self.project.hardware, &assignments);
        let rack_card = div()
            .m_3()
            .p_3()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(0xf7f9fb))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr("Cihaz görünümü")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(Self::heading(totals))
                            .child(
                                self.button("hw-scan", tr("PLC'den oku ve karşılaştır"), self.busy)
                                    .tooltip(tr("Etkin bağlantı profiliyle D1140/D1142/D1320 okunur; PLC'ye yazılmaz"))
                                    .on_click(cx.listener(|this, _, _, cx| this.scan_hardware(cx))),
                            ),
                    ),
            )
            .child(
                div().id("rack-scroll").overflow_x_scroll().child(
                    div().flex().flex_col().child(numbers).child(
                        div()
                            .p_2()
                            .rounded(px(4.))
                            .bg(rgb(0xe9eef3))
                            .border_1()
                            .border_color(rgb(0xc9d3dd))
                            .child(rack),
                    ),
                ),
            );
        let properties = match self.extra.hw_selected {
            Some(index) if index < self.project.hardware.len() => {
                self.module_properties(index, assignments.get(index), locked, cx)
            }
            _ => self.cpu_properties(model, locked, cx),
        };
        div()
            .size_full()
            .flex()
            .bg(rgb(BG))
            .child(
                div()
                    .id("hardware-main")
                    .flex_1()
                    .min_w_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .child(rack_card)
                    .when_some(self.scan_card(), |column, card| {
                        column.child(div().mx_3().mb_3().child(card))
                    })
                    .child(div().mx_3().mb_3().child(properties)),
            )
            .child(self.hardware_catalog(locked, cx))
            .into_any_element()
    }

    /// Result of the last PLC module scan compared with the configuration.
    fn scan_card(&self) -> Option<AnyElement> {
        let scan = match self.extra.hw_scan.as_ref()? {
            Ok(scan) => scan,
            Err(e) => {
                return Some(
                    div()
                        .p_3()
                        .rounded(px(6.))
                        .bg(rgb(0xfdecea))
                        .text_color(rgb(ERROR))
                        .child(tfmt!("PLC modül taraması başarısız: {e}", "PLC module scan failed: {e}"))
                        .into_any_element(),
                );
            }
        };
        let result = hardware::compare_scan(&self.project.hardware, scan);
        let (ok_bg, ok_border) = if result.matches {
            (0xeaf6ef, 0x86c9a1)
        } else {
            (0xfff7e6, 0xf59e0b)
        };
        let mut card = div()
            .p_3()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(ok_border))
            .bg(rgb(ok_bg))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if result.matches {
                        tr("PLC ile yapılandırma uyuşuyor")
                    } else {
                        tr("PLC ile yapılandırma farklı")
                    }),
            )
            .child(property(
                tr("Dijital genişleme"),
                tfmt!(
                    "yapılandırma {} giriş / {} çıkış · PLC (D1142/D1143) {} / {}", "configured {} inputs / {} outputs · PLC (D1142/D1143) {} / {}",
                    result.configured_points.0,
                    result.configured_points.1,
                    result.found_points.0,
                    result.found_points.1
                ),
            ));
        for slot in &result.specials {
            let same = slot.configured.is_some() && slot.configured == slot.found;
            card = card.child(
                div()
                    .flex()
                    .py_1()
                    .child(
                        div()
                            .w(px(170.))
                            .text_color(rgb(MUTED))
                            .child(tfmt!("Özel modül K{}", "Special module K{}", slot.number)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(if same { GREEN } else { AMBER }))
                            .child(tfmt!(
                                "yapılandırma {} · PLC {}", "configured {} · PLC {}",
                                slot.configured.unwrap_or("—"),
                                match (slot.found, slot.found_code) {
                                    (Some(name), _) => name.to_string(),
                                    (None, Some(code)) => tfmt!("bilinmeyen tip {code}", "unknown type {code}"),
                                    (None, None) => "—".into(),
                                }
                            )),
                    ),
            );
        }
        Some(card.into_any_element())
    }

    fn cpu_tile(
        &self,
        model: Option<&'static opendvp::devices::PlcModel>,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ethernet = model.is_some_and(|m| m.comm.contains("Ethernet"));
        div()
            .id("rack-cpu")
            .w(px(168.))
            .h(px(RACK_H))
            .flex_shrink_0()
            .rounded(px(4.))
            .bg(rgb(0x2a3542))
            .border_2()
            .border_color(rgb(if selected { ACCENT } else { 0x2a3542 }))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.extra.hw_selected = None;
                cx.notify();
            }))
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .child(leds(&["RUN", "ERR"], &[0x22c55e, 0x4b5866]))
            .child(
                div()
                    .h(px(64.))
                    .rounded(px(3.))
                    .bg(rgb(0x18222d))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(div().text_xs().text_color(rgb(CHROME_MUTED)).child("DELTA"))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(CHROME_TEXT))
                            .child(model.map_or(self.project.device.clone(), |m| m.title.into())),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(port(0x16a34a, "485"))
                    .child(port(0x16a34a, "232"))
                    .when(ethernet, |row| row.child(port(0xa855f7, "ETH"))),
            )
            .child(terminals(10))
            .into_any_element()
    }

    fn module_tile(
        &self,
        index: usize,
        module: &'static Module,
        assignment: Option<&Assignment>,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (face, accent) = match module.kind {
            Kind::Digital { .. } => (0xdfe7ee, 0x6b8299),
            Kind::Special { .. } => (0xe4e9f5, 0x5b6fb3),
        };
        let address = match assignment {
            Some(Assignment::Digital { inputs, outputs }) => {
                let mut lines = Vec::new();
                if let Some(range) = inputs {
                    lines.push(hardware::range_text('X', *range));
                }
                if let Some(range) = outputs {
                    lines.push(hardware::range_text('Y', *range));
                }
                lines.join("\n")
            }
            Some(Assignment::Special { number }) => format!("K{number}"),
            None => String::new(),
        };
        div()
            .id(("rack-module", index))
            .w(px(SLOT_W))
            .h(px(RACK_H - 20.))
            .flex_shrink_0()
            .rounded(px(4.))
            .bg(rgb(face))
            .border_2()
            .border_color(rgb(if selected { ACCENT } else { 0xa9b8c7 }))
            .cursor_pointer()
            .hover(|s| s.border_color(rgb(if selected { ACCENT } else { accent })))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.extra.hw_selected = Some(index);
                cx.notify();
            }))
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(leds(&["", "", ""], &[0x22c55e, 0x9aa8b7, 0x9aa8b7]))
            .child(
                div()
                    .mt_1()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0x24364c))
                    .child(module.name),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(0x52647a))
                    .child(match module.kind {
                        Kind::Digital {
                            inputs, outputs, ..
                        } => tfmt!("{inputs} DI / {outputs} DO", "{inputs} DI / {outputs} DO"),
                        Kind::Special { channels, .. } => channels.to_string(),
                    }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family("Consolas")
                    .text_size(px(10.))
                    .text_color(rgb(accent))
                    .children(address.lines().map(|l| div().child(l.to_owned()))),
            )
            .child(terminals(6))
            .into_any_element()
    }

    /// Change the project's PLC model. The simulator and the warnings follow the model, so an
    /// open simulator connection has to be closed first.
    fn set_model(&mut self, id: &'static str, cx: &mut Context<Self>) {
        if self.project.device.eq_ignore_ascii_case(id) {
            return;
        }
        self.project.device = id.into();
        self.warn_unsupported_specials(id);
        self.hardware_changed(tfmt!("PLC modeli {id} olarak değiştirildi.", "PLC model changed to {id}."), cx);
    }

    fn cpu_properties(
        &self,
        model: Option<&'static opendvp::devices::PlcModel>,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut card = properties_card("CPU · slot 1");
        match model {
            Some(m) => {
                card = card
                    .child(property("Model", m.title.into()))
                    .child(property(tr("Aile"), m.family.into()))
                    .child(property(tr("Program kapasitesi"), tfmt!("{} adım", "{} steps", m.steps)))
                    .child(property(tr("Dahili G/Ç"), m.io.into()))
                    .child(property(tr("Haberleşme"), m.comm.into()))
                    .child(property(
                        tr("Genişleme"),
                        tr("Sağ taraf: dijital modüller X20/Y20'den başlar, özel modüller K0–K7")
                            .into(),
                    ));
            }
            None => card = card.child(property("Model", self.project.device.clone())),
        }
        let switch_locked = locked || self.bridge.is_some();
        let mut models = div().flex().flex_wrap().gap_2();
        for m in opendvp::devices::MODELS {
            let id: &'static str = m.id;
            models = models.child(
                Button::new(SharedString::from(format!("model-{id}")))
                    .label(id)
                    .small()
                    .disabled(switch_locked)
                    .when(self.project.device.eq_ignore_ascii_case(id), |b| b.primary())
                    .on_click(cx.listener(move |this, _, _, cx| this.set_model(id, cx))),
            );
        }
        card.child(Self::heading(tr("Proje modeli")))
            .child(models)
            .child(Self::heading(if switch_locked {
                tr("Modeli değiştirmek için simülatör bağlantısını kapatın.")
            } else {
                tr("Katalogdan modül ekleyin; adresler slot sırasına göre atanır.")
            }))
            .into_any_element()
    }

    fn module_properties(
        &self,
        index: usize,
        assignment: Option<&Assignment>,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(module) = hardware::find(&self.project.hardware[index]) else {
            return div().into_any_element();
        };
        let mut card = properties_card(tfmt!("{} · slot {}", "{} · slot {}", module.name, index + 2))
            .child(property(tr("Açıklama"), module.summary.into()))
            .child(property(tr("Grup"), module.group.into()));
        match (module.kind, assignment) {
            (
                Kind::Digital {
                    inputs,
                    outputs,
                    output,
                },
                Some(Assignment::Digital {
                    inputs: x,
                    outputs: y,
                }),
            ) => {
                card = card
                    .child(property(
                        tr("Girişler"),
                        x.map_or("—".into(), |r| {
                            tfmt!("{} ({inputs} nokta)", "{} ({inputs} points)", hardware::range_text('X', r))
                        }),
                    ))
                    .child(property(
                        tr("Çıkışlar"),
                        y.map_or("—".into(), |r| {
                            tfmt!(
                                "{} ({outputs} nokta · {output})", "{} ({outputs} points · {output})",
                                hardware::range_text('Y', r)
                            )
                        }),
                    ));
            }
            (Kind::Special { code, channels }, Some(Assignment::Special { number })) => {
                card = card
                    .child(property(tr("Kanallar"), channels.into()))
                    .child(property(tr("Modül numarası"), format!("K{number} (FROM/TO m1)")))
                    .child(property(tr("Tip kodu"), tfmt!("{code} (D1320+{number} ve CR0)", "{code} (D1320+{number} and CR0)")))
                    .child(property(
                        tr("Erişim"),
                        tfmt!("FROM K{number} K<CR> D<hedef> K<adet> · TO K{number} K<CR> <kaynak> K<adet>", "FROM K{number} K<CR> D<target> K<count> · TO K{number} K<CR> <source> K<count>"),
                    ));
            }
            _ => {}
        }
        card.child(
            div()
                .flex()
                .gap_2()
                .mt_2()
                .child(
                    self.button("hw-left", tr("◀ Sola taşı"), locked || index == 0)
                        .on_click(cx.listener(move |this, _, _, cx| this.move_module(index, true, cx))),
                )
                .child(
                    self.button(
                        "hw-right",
                        tr("Sağa taşı ▶"),
                        locked || index + 1 >= self.project.hardware.len(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.move_module(index, false, cx))),
                )
                .child(
                    self.button("hw-remove", tr("Kaldır"), locked)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let removed = this.project.hardware.remove(index);
                            this.extra.hw_selected = index.checked_sub(1);
                            this.hardware_changed(tfmt!("{removed} kaldırıldı.", "{removed} removed."), cx);
                        })),
                ),
        )
        .child(Self::heading(
            tr("Adresler Delta'nın sağ genişleme numaralandırmasına göre hesaplanır; gerçek PLC'de D1142/D1143 ile doğrulayın."),
        ))
        .into_any_element()
    }

    fn hardware_catalog(&self, locked: bool, cx: &mut Context<Self>) -> AnyElement {
        let query = self.extra.hw_query.read(cx).value().trim().to_lowercase();
        let mut list = div()
            .id("hw-catalog")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.));
        for &group in GROUPS {
            let modules: Vec<&'static Module> = CATALOG
                .iter()
                .filter(|m| m.group == group)
                .filter(|m| {
                    query.is_empty()
                        || format!("{} {}", m.name, m.summary)
                            .to_lowercase()
                            .contains(&query)
                })
                .collect();
            if modules.is_empty() {
                continue;
            }
            list = list.child(
                div()
                    .mt_2()
                    .px_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child(group.to_uppercase()),
            );
            for module in modules {
                list = list.child(
                    div()
                        .id(SharedString::from(format!("hw-cat-{}", module.name)))
                        .flex()
                        .items_center()
                        .gap_2()
                        .p_2()
                        .rounded(px(5.))
                        .when(!locked, |row| {
                            row.cursor_pointer()
                                .hover(|s| s.bg(rgb(0xf1f4f7)))
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.add_module(module, cx)),
                                )
                        })
                        .child(
                            div()
                                .w(px(10.))
                                .h(px(30.))
                                .flex_shrink_0()
                                .rounded(px(2.))
                                .bg(rgb(if module.is_special() {
                                    0x5b6fb3
                                } else {
                                    0x6b8299
                                })),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(module.name),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .truncate()
                                        .child(module.summary),
                                ),
                        )
                        .child(div().text_color(rgb(ACCENT)).child("+")),
                );
            }
        }
        div()
            .w(px(330.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .border_l_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr("Donanım kataloğu")),
            )
            .child(Input::new(&self.extra.hw_query).small())
            .child(Self::heading(if locked {
                tr("Çevrimiçi / çalışırken yapılandırma kilitli.")
            } else {
                tr("Tıklanan modül seçili slotun sağına eklenir.")
            }))
            .child(list)
            .into_any_element()
    }
}

fn summary(modules: &[String], assignments: &[Assignment]) -> String {
    let mut inputs = 0u16;
    let mut outputs = 0u16;
    let mut specials = 0;
    for (name, a) in modules.iter().zip(assignments) {
        match (hardware::find(name).map(|m| m.kind), a) {
            (
                Some(Kind::Digital {
                    inputs: i,
                    outputs: o,
                    ..
                }),
                _,
            ) => {
                inputs += i;
                outputs += o;
            }
            (_, Assignment::Special { .. }) => specials += 1,
            _ => {}
        }
    }
    tfmt!(
        "{} modül · genişleme {inputs} giriş / {outputs} çıkış · özel modül {specials}/{}", "{} modules · expansion {inputs} inputs / {outputs} outputs · special modules {specials}/{}",
        modules.len(),
        hardware::MAX_SPECIAL
    )
}

fn slot_number(text: &str, width: f32) -> Div {
    div()
        .w(px(width))
        .flex_shrink_0()
        .text_center()
        .text_xs()
        .text_color(rgb(MUTED))
        .child(text.to_owned())
}

fn leds(labels: &[&str], colors: &[u32]) -> Div {
    div()
        .flex()
        .gap_2()
        .children(labels.iter().zip(colors).map(|(label, color)| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(div().size(px(6.)).rounded_full().bg(rgb(*color)))
                .when(!label.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(9.))
                            .text_color(rgb(CHROME_MUTED))
                            .child(label.to_string()),
                    )
                })
        }))
}

fn port(color: u32, label: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .child(
            div()
                .size(px(22.))
                .rounded(px(3.))
                .border_2()
                .border_color(rgb(color))
                .bg(rgb(0x111821)),
        )
        .child(
            div()
                .text_size(px(9.))
                .text_color(rgb(CHROME_MUTED))
                .child(label.to_owned()),
        )
}

fn terminals(count: usize) -> Div {
    div()
        .flex()
        .gap(px(2.))
        .children((0..count).map(|_| div().flex_1().h(px(8.)).rounded(px(1.)).bg(rgb(0x9aa8b7))))
}

fn properties_card(title: impl Into<SharedString>) -> Div {
    div()
        .p_4()
        .rounded(px(6.))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(PANEL))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .mb_1()
                .child(title.into()),
        )
}

fn property(label: &'static str, value: String) -> Div {
    div()
        .flex()
        .py_1()
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
}
