//! Watch and force table in the style of PLC IDEs: monitor values, modify once, or force
//! a value that the local simulation re-applies every scan.
use super::*;
use gpui_component::IconName;
use opendvp::plc_force::Force;

/// Accepts `123`, `-5`, `H1F` / `16#1F`, `true` / `false`.
fn parse_value(text: &str) -> Option<i32> {
    let t = text.trim();
    match t.to_ascii_lowercase().as_str() {
        "true" | "on" => return Some(1),
        "false" | "off" => return Some(0),
        _ => {}
    }
    if let Some(hex) = t
        .strip_prefix("16#")
        .or_else(|| t.strip_prefix('H'))
        .or_else(|| t.strip_prefix('h'))
    {
        return i32::from_str_radix(hex, 16).ok();
    }
    t.strip_prefix('K').unwrap_or(t).parse().ok()
}

impl Workbench {
    /// Re-apply forced values to the local simulation; inputs before a scan, every
    /// forced device after it so the program cannot overwrite them.
    pub(super) fn apply_forces(&mut self, inputs_only: bool) {
        for (&device, &value) in &self.forced {
            if inputs_only && device.area != Area::X {
                continue;
            }
            if device.bit() && !matches!(device.area, Area::T | Area::C) {
                self.simulator.memory.write_bit(device, value != 0);
            } else {
                self.simulator.memory.set_word(device, value as i16);
            }
        }
    }

    fn watch_target(&self, cx: &App) -> Option<(Device, i32)> {
        let device = self.extra.watch_selected?;
        let text = self.extra.watch_value.read(cx).value().to_string();
        parse_value(&text).map(|v| (device, v))
    }

    fn modify_device(&mut self, device: Device, value: i32, cx: &mut Context<Self>) {
        if self.extra.hardware.is_some() {
            self.log(tr("Fiziksel PLC izleme bağlantısı salt okunurdur; değer yazılmadı."));
        } else if self.bridge.is_some() {
            if device.area == Area::X {
                self.inputs.insert(device, value != 0);
                self.pending_inputs.insert(device, value != 0);
                self.log(tfmt!("{device} ← {value} (simülatöre gönderilecek)", "{device} ← {value} (will be sent to the simulator)"));
            } else {
                self.log(tr("Simülatör oturumunda yalnız X girişleri değiştirilebilir."));
            }
        } else {
            if device.bit() && !matches!(device.area, Area::T | Area::C) {
                self.simulator.memory.write_bit(device, value != 0);
            } else {
                self.simulator.memory.set_word(device, value as i16);
            }
            self.log(format!("{device} ← {value}"));
        }
        cx.notify();
    }

    fn force_device(&mut self, device: Device, value: i32, cx: &mut Context<Self>) {
        if let Some(client) = self.extra.hardware.clone() {
            self.force_physical(client, device, value, cx);
            return;
        } else if self.bridge.is_some() {
            self.log(tr("Simülatör oturumunda zorlama kullanılamaz; yerel simülasyon veya fiziksel PLC kullanın."));
        } else {
            self.forced.insert(device, value);
            self.apply_forces(false);
            self.log(tfmt!("{device} {value} değerine zorlandı.", "{device} forced to {value}."));
        }
        cx.notify();
    }

    /// Force X/Y in the real PLC (Delta editor FC64 01 30). Value 0/1 = OFF/ON.
    fn force_physical(
        &mut self,
        client: crate::plc_connection::Client,
        device: Device,
        value: i32,
        cx: &mut Context<Self>,
    ) {
        if self.plc_write_blocked(cx) {
        } else if !matches!(device.area, Area::X | Area::Y) {
            self.log(tr("Fiziksel PLC'de yalnız X ve Y aygıtları zorlanabilir."));
        } else if self.busy {
            self.log(tr("Önceki işlem sürüyor; birkaç saniye sonra tekrar deneyin."));
        } else {
            let mode = if value != 0 { Force::On } else { Force::Off };
            self.forced.insert(device, value);
            self.log(tfmt!("{device} PLC'de {} zorlanıyor…", "Forcing {device} {} on the PLC…", if value != 0 { "ON" } else { "OFF" }));
            self.dispatch(
                async move {
                    Done::Force(
                        client
                            .force(device, mode)
                            .await
                            .map(|()| tfmt!("{device} PLC'de zorlandı (PLC tutar; kaldırana kadar geçerli).", "{device} forced on the PLC (the PLC holds it; valid until released).")),
                    )
                },
                cx,
            );
        }
        cx.notify();
    }
    fn release_physical(&mut self, device: Option<Device>, cx: &mut Context<Self>) {
        let Some(client) = self.extra.hardware.clone() else {
            return;
        };
        if self.busy {
            self.log(tr("Önceki işlem sürüyor; birkaç saniye sonra tekrar deneyin."));
            cx.notify();
            return;
        }
        match device {
            Some(device) => {
                self.forced.remove(&device);
                self.dispatch(
                    async move {
                        Done::Force(
                            client
                                .force(device, Force::Release)
                                .await
                                .map(|()| tfmt!("{device} zorlaması PLC'de kaldırıldı.", "Force on {device} released on the PLC.")),
                        )
                    },
                    cx,
                );
            }
            None => {
                self.forced.clear();
                self.dispatch(
                    async move {
                        Done::Force(
                            client
                                .release_all_forces()
                                .await
                                .map(|()| tr("Tüm zorlamalar PLC'de kaldırıldı.").to_string()),
                        )
                    },
                    cx,
                );
            }
        }
    }

    pub(super) fn watch_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let online = self.bridge.is_some() || self.extra.hardware.is_some();
        let selected = self.extra.watch_selected;
        let mut table = div()
            .id("watch-table")
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
                    .child(div().w(px(170.)).px_3().child(tr("Adres")))
                    .child(div().flex_1().child(tr("Açıklama")))
                    .child(div().w(px(80.)).child(tr("Biçim")))
                    .child(div().w(px(150.)).child(tr("İzleme değeri")))
                    .child(div().w(px(110.)).child(tr("Zorlama")))
                    .child(div().w(px(150.)).child("")),
            );
        for name in &self.project.watch {
            let Ok(device) = name.parse::<Device>() else {
                continue;
            };
            let bit = device.bit() && !matches!(device.area, Area::T | Area::C);
            let format = self.extra.watch_format.get(&device).copied().unwrap_or(0);
            let value = if bit {
                match self.observed_bit(device) {
                    Some(true) => "TRUE".to_string(),
                    Some(false) => "FALSE".to_string(),
                    None => "—".to_string(),
                }
            } else if online {
                self.device_value(device)
                    .replace("word=", "")
                    .replace("dword=", "")
            } else {
                let w = self.simulator.memory.word(device);
                match format {
                    1 => format!("16#{:04X}", w as u16),
                    2 => format!("2#{:016b}", w as u16),
                    _ => w.to_string(),
                }
            };
            let forced = self.forced.get(&device).copied();
            let row_selected = selected == Some(device);
            table = table.child(
                div()
                    .id(SharedString::from(format!("watch-{device}")))
                    .flex()
                    .items_center()
                    .h(px(34.))
                    .border_b_1()
                    .border_color(rgb(0xedf0f4))
                    .cursor_pointer()
                    .bg(rgb(if row_selected {
                        0xe3f3f4
                    } else if forced.is_some() {
                        0xfff4e5
                    } else {
                        PANEL
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.extra.watch_selected = Some(device);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(170.))
                            .px_3()
                            .font_family("Consolas")
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(self.device_title(device)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(self.project.comment(device).unwrap_or_default().to_owned()),
                    )
                    .child(div().w(px(80.)).child(if bit {
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child("BOOL")
                            .into_any_element()
                    } else {
                        Button::new(SharedString::from(format!("watch-fmt-{device}")))
                            .label(["DEC", "HEX", "BIN"][usize::from(format)])
                            .xsmall()
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let next =
                                    (this.extra.watch_format.get(&device).copied().unwrap_or(0)
                                        + 1)
                                        % 3;
                                this.extra.watch_format.insert(device, next);
                                cx.notify();
                            }))
                            .into_any_element()
                    }))
                    .child(
                        div()
                            .w(px(150.))
                            .font_family("Consolas")
                            .text_sm()
                            .text_color(rgb(if value == "TRUE" { GREEN } else { TEXT }))
                            .child(value),
                    )
                    .child(
                        div().w(px(110.)).child(match forced {
                            Some(v) => div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .px_1()
                                        .rounded(px(3.))
                                        .bg(rgb(ONLINE))
                                        .text_xs()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(0xffffff))
                                        .child("F"),
                                )
                                .child(
                                    div().font_family("Consolas").text_sm().child(v.to_string()),
                                ),
                            None => div().text_xs().text_color(rgb(MUTED)).child("—"),
                        }),
                    )
                    .child(
                        div()
                            .w(px(150.))
                            .flex()
                            .gap_1()
                            .when(bit, |cell| {
                                cell.child(
                                    Button::new(SharedString::from(format!("watch-1-{device}")))
                                        .label("1")
                                        .xsmall()
                                        .outline()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.modify_device(device, 1, cx)
                                        })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!("watch-0-{device}")))
                                        .label("0")
                                        .xsmall()
                                        .outline()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.modify_device(device, 0, cx)
                                        })),
                                )
                            })
                            .child(
                                Button::new(SharedString::from(format!("watch-del-{device}")))
                                    .icon(IconName::Close)
                                    .xsmall()
                                    .ghost()
                                    .tooltip(tr("Tablodan kaldır"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.project.watch.retain(|d| d != &device.to_string());
                                        this.forced.remove(&device);
                                        this.dirty = true;
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        }
        let target_label = selected.map_or(tr("Satır seçin").to_string(), |d| tfmt!("Seçili: {d}", "Selected: {d}"));
        let controls = div()
            .m_3()
            .p_3()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(90.))
                    .child(Input::new(&self.extra.watch_new).small()),
            )
            .child(
                self.button("watch-add", tr("Ekle"), false)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let text = this.extra.watch_new.read(cx).value().trim().to_uppercase();
                        match text.parse::<Device>() {
                            Ok(device) => {
                                let name = device.to_string();
                                if !this.project.watch.contains(&name)
                                    && this.project.watch.len() < 64
                                {
                                    this.project.watch.push(name);
                                    this.dirty = true;
                                }
                                this.extra.watch_selected = Some(device);
                            }
                            Err(e) => this.log(tfmt!("Geçersiz adres: {e}", "Invalid address: {e}")),
                        }
                        cx.notify();
                    })),
            )
            .child(div().w(px(1.)).h(px(22.)).mx_2().bg(rgb(LINE)))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(target_label))
            .child(
                div()
                    .w(px(120.))
                    .child(Input::new(&self.extra.watch_value).small()),
            )
            .child(
                self.button("watch-modify", tr("Değiştir"), selected.is_none())
                    .tooltip(tr("Değeri bir kez yaz"))
                    .on_click(cx.listener(|this, _, _, cx| match this.watch_target(cx) {
                        Some((device, value)) => this.modify_device(device, value, cx),
                        None => {
                            this.log(tr("Değer okunamadı (örnek: 125, H1F, TRUE)."));
                            cx.notify();
                        }
                    })),
            )
            .child(
                self.button("watch-force", tr("Zorla"), selected.is_none() || self.bridge.is_some())
                    .warning()
                    .tooltip(tr("Yerel simülasyonda her taramada uygula; fiziksel PLC'de X/Y'yi PLC'ye zorla (Delta editor FC64)"))
                    .on_click(cx.listener(|this, _, _, cx| match this.watch_target(cx) {
                        Some((device, value)) => this.force_device(device, value, cx),
                        None => {
                            this.log(tr("Değer okunamadı (örnek: 125, H1F, TRUE)."));
                            cx.notify();
                        }
                    })),
            )
            .child(
                self.button(
                    "watch-unforce",
                    tr("Zorlamayı kaldır"),
                    selected.is_none_or(|d| !self.forced.contains_key(&d)),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.extra.hardware.is_some() {
                        if let Some(device) = this.extra.watch_selected {
                            this.release_physical(Some(device), cx);
                        }
                    } else if let Some(device) = this.extra.watch_selected
                        && this.forced.remove(&device).is_some()
                    {
                        this.log(tfmt!("{device} zorlaması kaldırıldı.", "Force on {device} released."));
                    }
                    cx.notify();
                })),
            )
            .child(
                self.button(
                    "watch-unforce-all",
                    tr("Tüm zorlamaları kaldır"),
                    self.forced.is_empty(),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.extra.hardware.is_some() {
                        this.release_physical(None, cx);
                        return;
                    }
                    this.forced.clear();
                    this.log(tr("Tüm zorlamalar kaldırıldı."));
                    cx.notify();
                })),
            );
        let note = if self.extra.hardware.is_some() {
            tr("Fiziksel PLC: izleme salt okunur; yalnız X/Y zorlama (PLC'de tutulur) kullanılabilir.")
        } else if self.bridge.is_some() {
            tr("Simülatör: yalnız X girişleri değiştirilebilir; zorlama yerel simülasyona özeldir.")
        } else {
            tr("Yerel simülasyon: tüm cihazlar değiştirilebilir; zorlanan değer her taramada yeniden uygulanır.")
        };
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
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr("İzleme ve zorlama tablosu")),
                    )
                    .child(Self::heading(note))
                    .when(!self.forced.is_empty(), |row| {
                        row.child(
                            div()
                                .px_2()
                                .py(px(2.))
                                .rounded(px(4.))
                                .bg(rgb(ONLINE))
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0xffffff))
                                .child(tfmt!("ZORLAMA ETKİN · {}", "FORCE ACTIVE · {}", self.forced.len())),
                        )
                    }),
            )
            .child(controls)
            .child(table)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_value;

    #[test]
    fn modify_values_accept_decimal_hex_and_bool() {
        assert_eq!(parse_value("125"), Some(125));
        assert_eq!(parse_value("K-5"), Some(-5));
        assert_eq!(parse_value("H1F"), Some(31));
        assert_eq!(parse_value("16#ff"), Some(255));
        assert_eq!(parse_value("TRUE"), Some(1));
        assert_eq!(parse_value("x"), None);
    }
}
