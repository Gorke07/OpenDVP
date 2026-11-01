//! Symbol table page: names for devices (usable in IL operands and ST), the allocation range
//! and the automatically assigned addresses.
use super::*;
use opendvp::{
    alloc::Alloc,
    project::Symbol,
    st::DataType::{self, Bool, Dint, Dword, Int, Real, Word},
};

/// Types the table offers, in the order the type button cycles through them.
pub(super) const SYMBOL_TYPES: [DataType; 6] = [Bool, Word, Dword, Int, Dint, Real];

impl Workbench {
    /// A symbol or the allocation range changed: recompile (which assigns missing addresses).
    fn symbols_changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.uploaded = false;
        self.compile();
        cx.notify();
    }
    /// Text for a device: its symbol name when names are shown, else the address.
    pub(super) fn device_name(&self, device: Device) -> String {
        if self.extra.show_symbols
            && let Some((name, _)) = self.project.symbol_at(device)
        {
            return name.to_owned();
        }
        device.to_string()
    }
    /// `Name · X0` for a symbol (names shown), else just the address; for wide cells.
    pub(super) fn device_title(&self, device: Device) -> String {
        match self.device_name(device) {
            name if name != device.to_string() => format!("{name} · {device}"),
            address => address,
        }
    }
    /// Put the symbols' initial values into the local simulator (a fresh run starts from them).
    pub(super) fn apply_initial_values(&mut self) {
        let Ok(values) = self.project.initial_values() else {
            return;
        };
        for value in values {
            if value.bit {
                let on = value.words.first().is_some_and(|w| *w != 0);
                self.simulator.memory.write_bit(value.device, on);
            } else {
                for (i, word) in value.words.iter().enumerate() {
                    let device = Device {
                        area: value.device.area,
                        index: value.device.index + i as u16,
                    };
                    self.simulator.memory.set_word(device, *word as i16);
                }
            }
        }
    }
    pub(super) fn apply_symbol(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.extra.sym_name.read(cx).value().trim().to_owned();
        let device = self.extra.sym_device.read(cx).value().trim().to_uppercase();
        let comment = self.extra.sym_comment.read(cx).value().trim().replace(['\n', '\r'], " ");
        let init = self.extra.sym_init.read(cx).value().trim().to_owned();
        let ty = SYMBOL_TYPES[self.extra.sym_type % SYMBOL_TYPES.len()];
        let existing = self.project.symbols.get(&name).cloned();
        // Leaving the address empty keeps an automatic address the symbol already has.
        let (device, auto) = match (device.is_empty(), &existing) {
            (true, Some(old)) if old.auto && old.ty == ty => (old.device.clone(), true),
            (true, _) => (None, false),
            (false, _) => (Some(device), false),
        };
        let symbol = Symbol {
            ty,
            device,
            auto,
            retain: self.extra.sym_retain,
            comment,
            init: (!init.is_empty()).then_some(init),
        };
        match self.project.set_symbol(&name, symbol) {
            Ok(()) => {
                self.log(tfmt!("Sembol kaydedildi: {name}", "Symbol saved: {name}"));
                // Ready for the next symbol; the range inputs stay as they are.
                for input in [
                    &self.extra.sym_name,
                    &self.extra.sym_device,
                    &self.extra.sym_comment,
                    &self.extra.sym_init,
                ] {
                    input.update(cx, |state, cx| state.set_value("", window, cx));
                }
                self.symbols_changed(cx);
            }
            Err(e) => {
                self.log(e);
                cx.notify();
            }
        }
    }
    pub(super) fn edit_symbol(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(symbol) = self.project.symbols.get(name).cloned() else {
            return;
        };
        self.extra.sym_type = SYMBOL_TYPES.iter().position(|t| *t == symbol.ty).unwrap_or(0);
        self.extra.sym_retain = symbol.retain;
        let device = if symbol.auto { String::new() } else { symbol.device.clone().unwrap_or_default() };
        for (input, text) in [
            (&self.extra.sym_name, name.to_owned()),
            (&self.extra.sym_device, device),
            (&self.extra.sym_comment, symbol.comment),
            (&self.extra.sym_init, symbol.init.unwrap_or_default()),
        ] {
            input.update(cx, |state, cx| state.set_value(text, window, cx));
        }
        cx.notify();
    }
    pub(super) fn delete_symbol(&mut self, name: &str, cx: &mut Context<Self>) {
        self.project.remove_symbol(name);
        self.log(tfmt!("Sembol silindi: {name}", "Symbol deleted: {name}"));
        self.symbols_changed(cx);
    }
    /// Apply the four range inputs as the new allocation range.
    pub(super) fn apply_alloc(&mut self, cx: &mut Context<Self>) {
        let number = |input: &Entity<InputState>, cx: &App| input.read(cx).value().trim().parse::<u16>();
        let parsed = (
            number(&self.extra.alloc_d_lo, cx),
            number(&self.extra.alloc_d_hi, cx),
            number(&self.extra.alloc_m_lo, cx),
            number(&self.extra.alloc_m_hi, cx),
        );
        let kept = (
            number(&self.extra.retain_d_lo, cx),
            number(&self.extra.retain_d_hi, cx),
            number(&self.extra.retain_m_lo, cx),
            number(&self.extra.retain_m_hi, cx),
        );
        let ((Ok(d_lo), Ok(d_hi), Ok(m_lo), Ok(m_hi)), (Ok(rd_lo), Ok(rd_hi), Ok(rm_lo), Ok(rm_hi))) =
            (parsed, kept)
        else {
            self.log(tr("Aralık sınırları sayı olmalı."));
            cx.notify();
            return;
        };
        let retain = opendvp::alloc::Retain {
            d: (rd_lo, rd_hi),
            m: (rm_lo, rm_hi),
        };
        let result = self
            .project
            .set_alloc(Alloc {
                d: (d_lo, d_hi),
                m: (m_lo, m_hi),
            })
            .and_then(|()| self.project.set_retain(retain));
        match result {
            Ok(()) => {
                self.log(tr("Tahsis aralığı uygulandı. Atanmış adresler yerinde kalır."));
                self.symbols_changed(cx);
            }
            Err(e) => {
                self.log(e);
                cx.notify();
            }
        }
    }
    /// Forget the automatic addresses; the compile that follows assigns them again.
    pub(super) fn reassign_symbols(&mut self, cx: &mut Context<Self>) {
        self.project.clear_auto_addresses();
        self.log(tr("Otomatik adresler yeniden atandı."));
        self.symbols_changed(cx);
    }

    pub(super) fn symbols_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let locked = self.editing_locked();
        let ty = SYMBOL_TYPES[self.extra.sym_type % SYMBOL_TYPES.len()];
        let field = |w: f32, input: &Entity<InputState>| div().w(px(w)).flex_shrink_0().child(Input::new(input).small());
        let label = |text: &'static str| div().text_xs().text_color(rgb(MUTED)).child(tr(text));
        let ranges = div()
            .flex()
            .items_center()
            .gap_2()
            .flex_wrap()
            .child(label("Otomatik tahsis aralığı"))
            .child(label("D"))
            .child(field(70., &self.extra.alloc_d_lo))
            .child(label("–"))
            .child(field(70., &self.extra.alloc_d_hi))
            .child(label("M"))
            .child(field(70., &self.extra.alloc_m_lo))
            .child(label("–"))
            .child(field(70., &self.extra.alloc_m_hi))
            .child(
                self.button("alloc-apply", tr("Uygula"), locked)
                    .on_click(cx.listener(|this, _, _, cx| this.apply_alloc(cx))),
            )
            .child(
                self.button("alloc-reassign", tr("Adresleri yeniden ata"), locked)
                    .ghost()
                    .tooltip(tr("Otomatik atanan adresleri siler ve aralıktan yeniden verir"))
                    .on_click(cx.listener(|this, _, _, cx| this.reassign_symbols(cx))),
            );
        let retain_ranges = div()
            .flex()
            .items_center()
            .gap_2()
            .flex_wrap()
            .child(label("Kalıcı (latch'li) aralık"))
            .child(label("D"))
            .child(field(70., &self.extra.retain_d_lo))
            .child(label("–"))
            .child(field(70., &self.extra.retain_d_hi))
            .child(label("M"))
            .child(field(70., &self.extra.retain_m_lo))
            .child(label("–"))
            .child(field(70., &self.extra.retain_m_hi));
        let form = div()
            .flex()
            .items_center()
            .gap_2()
            .child(field(150., &self.extra.sym_name))
            .child(
                self.button("sym-type", ty.name(), false)
                    .ghost()
                    .tooltip(tr("Tipi değiştir"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.extra.sym_type = (this.extra.sym_type + 1) % SYMBOL_TYPES.len();
                        cx.notify();
                    })),
            )
            .child(
                self.button(
                    "sym-retain",
                    if self.extra.sym_retain { tr("Kalıcı: Evet") } else { tr("Kalıcı: Hayır") },
                    false,
                )
                .ghost()
                .tooltip(tr("Kalıcı sembol güç kesilince değerini korur; adresi latch'li alanda olmalı"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.extra.sym_retain = !this.extra.sym_retain;
                    cx.notify();
                })),
            )
            .child(field(110., &self.extra.sym_device))
            .child(div().flex_1().min_w_0().child(Input::new(&self.extra.sym_comment).small()))
            .child(field(90., &self.extra.sym_init))
            .child(
                self.button("sym-apply", tr("Ekle / güncelle"), locked)
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| this.apply_symbol(window, cx))),
            );
        let cell = |w: f32| div().w(px(w)).flex_shrink_0().px_3();
        let mut table = div()
            .id("symbol-table")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
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
                    .child(cell(170.).child(tr("Ad")))
                    .child(cell(80.).child(tr("Tip")))
                    .child(cell(150.).child(tr("Adres")))
                    .child(div().flex_1().px_3().child(tr("Açıklama")))
                    .child(cell(100.).child(tr("Başlangıç")))
                    .child(cell(150.).child("")),
            );
        if self.project.symbols.is_empty() {
            table = table.child(
                div()
                    .p_6()
                    .text_color(rgb(MUTED))
                    .child(tr("Henüz sembol yok. Yukarıdaki formdan ad, tip ve (isteğe bağlı) adres girin.")),
            );
        }
        for (name, symbol) in &self.project.symbols {
            let name = name.clone();
            let device = match (&symbol.device, symbol.auto) {
                (Some(d), true) => format!("{d} ({})", tr("otomatik")),
                (Some(d), false) => d.clone(),
                (None, _) => "—".to_owned(),
            };
            let (edit_name, delete_name) = (name.clone(), name.clone());
            table = table.child(
                div()
                    .id(SharedString::from(format!("symbol-row-{name}")))
                    .flex()
                    .items_center()
                    .h(px(32.))
                    .border_b_1()
                    .border_color(rgb(0xedf0f4))
                    .hover(|s| s.bg(rgb(0xf6f8fa)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.edit_symbol(&edit_name, window, cx)
                    }))
                    .child(cell(170.).font_family("Consolas").font_weight(FontWeight::SEMIBOLD).child(name.clone()))
                    .child(cell(80.).text_xs().child(if symbol.retain {
                        format!("{} ⚓", symbol.ty.name())
                    } else {
                        symbol.ty.name().to_owned()
                    }))
                    .child(cell(150.).font_family("Consolas").child(device))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_3()
                            .truncate()
                            .text_color(rgb(if symbol.comment.is_empty() { MUTED } else { TEXT }))
                            .child(if symbol.comment.is_empty() { "—".to_owned() } else { symbol.comment.clone() }),
                    )
                    .child(cell(100.).font_family("Consolas").child(symbol.init.clone().unwrap_or_else(|| "—".into())))
                    .child(
                        cell(150.).flex().gap_1().child(
                            Button::new(SharedString::from(format!("symbol-del-{name}")))
                                .label(tr("Sil"))
                                .xsmall()
                                .ghost()
                                .disabled(locked)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.delete_symbol(&delete_name, cx)
                                })),
                        ),
                    ),
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(Self::heading(tr(
                "Semboller IL işlenenlerinde ve ST'de adla kullanılır; PLC'ye yalnız adres gider.",
            )))
            .child(ranges)
            .child(retain_ranges)
            .child(form)
            .child(table)
            .into_any_element()
    }
}
