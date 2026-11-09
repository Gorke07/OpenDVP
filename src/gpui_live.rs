//! Live values of the active ST program's variables, from the local simulator or,
//! while a PLC is connected, read from the PLC (read-only FC01/02/03).
use super::*;
use opendvp::plc_monitor::{self, Value, Watch};

/// State of the live-value panel (kept in `Panels`).
pub(super) struct Live {
    pub open: bool,
    pub watches: Vec<Watch>,
    /// Program index and source the watches were derived from.
    key: Option<(usize, u64)>,
    /// Lowering error of the active source, shown instead of an empty list.
    pub error: Option<String>,
    /// Last PLC read: values in watch order, or the read error.
    pub plc: Option<Result<Vec<Value>, String>>,
    pub plc_at: Option<Instant>,
    pub next: Instant,
}

impl Default for Live {
    fn default() -> Self {
        Self {
            open: false,
            watches: Vec::new(),
            key: None,
            error: None,
            plc: None,
            plc_at: None,
            next: Instant::now(),
        }
    }
}

fn source_key(source: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut h);
    h.finish()
}

impl Workbench {
    /// Re-derive the watch list when the active ST source changed.
    pub(super) fn refresh_live(&mut self) {
        if !self.extra.live.open {
            return;
        }
        let key = (self.active_program, source_key(self.active_source()));
        if self.extra.live.key == Some(key) {
            return;
        }
        self.extra.live.key = Some(key);
        self.extra.live.plc = None;
        if self.project.program_language(self.active_program) != SourceLanguage::St {
            self.extra.live.watches.clear();
            self.extra.live.error =
                Some(tr("Canlı değerler yalnız ST programlarında gösterilir.").into());
            return;
        }
        match opendvp::st::lower_with(self.active_source(), &self.project.lower_options()) {
            Ok(lowered) => {
                self.extra.live.watches = plc_monitor::watches(&lowered.symbols);
                self.extra.live.error = None;
            }
            Err(e) => {
                // Keep the last good list so values stay visible while typing.
                self.extra.live.error = Some(e.message);
            }
        }
    }

    /// The PLC read due now, if any: only while the panel is open and a PLC is connected.
    pub(super) fn live_poll(&mut self) -> Option<impl Future<Output = Done> + 'static> {
        let live = &mut self.extra.live;
        if !live.open || live.watches.is_empty() || Instant::now() < live.next {
            return None;
        }
        let hardware = self.extra.hardware.clone()?;
        live.next = Instant::now() + Duration::from_millis(500);
        let wanted: Vec<_> = live.watches.iter().map(|w| (w.device, w.ty)).collect();
        Some(async move { Done::LiveValues(hardware.read_values(wanted).await) })
    }

    pub(super) fn adopt_live(&mut self, result: Result<Vec<Value>, String>) {
        if result.is_ok() {
            self.extra.live.plc_at = Some(Instant::now());
        }
        self.extra.live.plc = Some(result);
    }

    pub(super) fn live_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let live = &self.extra.live;
        let wanted: Vec<_> = live.watches.iter().map(|w| (w.device, w.ty)).collect();
        let (source, values, problem): (String, Option<Vec<Value>>, Option<String>) =
            if self.extra.hardware.is_some() {
                let age = live
                    .plc_at
                    .map(|t| tfmt!(" · {} ms önce", " · {} ms ago", t.elapsed().as_millis()))
                    .unwrap_or_default();
                match &live.plc {
                    Some(Ok(v)) if v.len() == wanted.len() => (
                        tfmt!("PLC (salt okunur){age}", "PLC (read-only){age}"),
                        Some(v.clone()),
                        None,
                    ),
                    Some(Err(e)) => (tr("PLC (salt okunur)").into(), None, Some(e.clone())),
                    _ => (tr("PLC (salt okunur)").into(), None, None),
                }
            } else {
                let note = if self.bridge.is_some() {
                    tr("Yerel model (COMMGR değerleri bu panelde değil)")
                } else {
                    tr("Yerel simülatör")
                };
                (
                    note.into(),
                    Some(plc_monitor::read_memory(&self.simulator.memory, &wanted)),
                    None,
                )
            };
        let header = div()
            .flex()
            .items_center()
            .gap_3()
            .child(Self::heading(tfmt!(
                "CANLI DEĞERLER · {} · {} satır",
                "LIVE VALUES · {} · {} rows",
                source,
                live.watches.len()
            )))
            .child(div().flex_1())
            .children(live.error.clone().map(|e| {
                div()
                    .max_w(px(420.))
                    .truncate()
                    .text_xs()
                    .text_color(rgb(ERROR))
                    .child(e)
            }))
            .children(problem.map(|e| {
                div()
                    .max_w(px(420.))
                    .truncate()
                    .text_xs()
                    .text_color(rgb(ERROR))
                    .child(e)
            }));
        let cell = |w: f32| div().w(px(w)).px_2().truncate();
        let mut rows = div()
            .id("live-values")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .h(px(24.))
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child(cell(200.).child(tr("Değişken")))
                    .child(cell(70.).child(tr("Tip")))
                    .child(cell(80.).child(tr("Adres")))
                    .child(div().flex_1().px_2().child(tr("Değer"))),
            );
        for (index, w) in live.watches.iter().enumerate() {
            let value = values
                .as_ref()
                .and_then(|v| v.get(index))
                .map_or_else(|| "—".to_owned(), |v| v.to_string());
            let on = values
                .as_ref()
                .and_then(|v| v.get(index))
                .is_some_and(|v| *v == Value::Bool(true));
            let line = w.line;
            rows = rows.child(
                div()
                    .id(("live-row", index))
                    .flex()
                    .h(px(24.))
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(0xedf0f4))
                    .font_family("Consolas")
                    .text_sm()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0xf1f4f7)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = Tab::Source;
                        this.jump_line = Some(line);
                        cx.notify();
                    }))
                    .child(
                        cell(200.)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(w.label.clone()),
                    )
                    .child(cell(70.).text_color(rgb(MUTED)).child(w.ty.name()))
                    .child(cell(80.).text_color(rgb(MUTED)).child(w.device.to_string()))
                    .child(
                        div()
                            .flex_1()
                            .px_2()
                            .truncate()
                            .text_color(rgb(if on { GREEN } else { TEXT }))
                            .child(value),
                    ),
            );
        }
        div()
            .h(px(240.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .bg(rgb(PANEL))
            .border_t_1()
            .border_color(rgb(LINE))
            .p_3()
            .child(header)
            .child(rows)
            .into_any_element()
    }
}
