use super::*;
use gpui_component::IconName;

const LIVE: u32 = 0x22c55e;

impl Workbench {
    /// Whether a ladder symbol conducts/is energized; `None` when not observable.
    fn symbol_lit(&self, symbol: &crate::ladder_plot::Symbol) -> Option<bool> {
        use crate::ladder_plot::Kind;
        // Like the energized conductors, symbol state is shown only while monitoring.
        if !(self.running || self.local_running || self.extra.hardware.is_some()) {
            return None;
        }
        let bit = self.observed_bit(symbol.device?)?;
        match symbol.kind {
            Kind::Contact(inverted) => Some(bit != inverted),
            // An edge contact conducts for one scan only; the level is not its state.
            Kind::Edge(_) => None,
            Kind::Coil | Kind::Block => Some(bit),
        }
    }
}

/// Three-valued power of a condition term; `None` when a value is not observable.
fn power_of(e: &opendvp::plc::Expr, bit: &dyn Fn(Device) -> Option<bool>) -> Option<bool> {
    use opendvp::plc::Expr;
    match e {
        Expr::Contact(d, inverted) => bit(*d).map(|v| v != *inverted),
        Expr::Invert(inner) => power_of(inner, bit).map(|v| !v),
        Expr::And(a, b) => match (power_of(a, bit), power_of(b, bit)) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        },
        Expr::Or(a, b) => match (power_of(a, bit), power_of(b, bit)) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        },
        _ => None,
    }
}

impl Workbench {
    pub(super) fn ladder(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::ladder_plot::Kind;
        let mut list = div()
            .id("ladder-scroll")
            .track_scroll(&self.ladder_scroll)
            .flex_1()
            .min_h_0()
            .overflow_scroll()
            .bg(rgb(PANEL))
            .flex()
            .flex_col();
        if let Some(program) = self.view_program() {
            if program.networks.is_empty() {
                list = list.child(
                    div()
                        .p_8()
                        .child(tr("Henüz ağ yok. Komut paletinden yeni ağ ekleyin.")),
                );
            }
            for (index, network) in program.networks.iter().enumerate() {
                let plot = crate::ladder_plot::network(network);
                let height = plot.height;
                let width = plot.width;
                let lines = plot.lines.clone();
                // Energized conductors while monitoring: green and thicker, as in common
                // PLC IDEs. Unknown values (edges, compares, stale data) stay neutral.
                let monitoring =
                    self.running || self.local_running || self.extra.hardware.is_some();
                let powered = plot
                    .feeds
                    .iter()
                    .map(|feed| {
                        monitoring
                            && feed.as_ref().is_some_and(|terms| {
                                terms
                                    .iter()
                                    .all(|t| power_of(t, &|d| self.observed_bit(d)) == Some(true))
                            })
                    })
                    .collect::<Vec<_>>();
                let symbols = plot.symbols.clone();
                // A symbol is drawn green when it conducts: a NO contact with bit 1, an NC
                // contact with bit 0, an energized coil.
                let states = symbols
                    .iter()
                    .map(|s| self.symbol_lit(s))
                    .collect::<Vec<_>>();
                let drawing = canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let at =
                            |x: f32, y: f32| point(bounds.left() + px(x), bounds.top() + px(y));
                        let mut grid = PathBuilder::stroke(px(1.));
                        for column in 1..((width / 108.) as usize) {
                            let x = 24. + column as f32 * 108.;
                            grid.move_to(at(x, 6.));
                            grid.line_to(at(x, height - 6.));
                        }
                        if let Ok(path) = grid.build() {
                            window.paint_path(path, rgb(0xedf1f6));
                        }
                        let mut wires = PathBuilder::stroke(px(1.3));
                        for ((x, y), (xx, yy)) in &lines {
                            wires.move_to(at(*x, *y));
                            wires.line_to(at(*xx, *yy));
                        }
                        if let Ok(path) = wires.build() {
                            window.paint_path(path, rgb(0x65758b));
                        }
                        let mut live = PathBuilder::stroke(px(3.));
                        let mut any_live = false;
                        for (((x, y), (xx, yy)), on) in lines.iter().zip(&powered) {
                            if *on {
                                live.move_to(at(*x, *y));
                                live.line_to(at(*xx, *yy));
                                any_live = true;
                            }
                        }
                        if any_live && let Ok(path) = live.build() {
                            window.paint_path(path, rgb(LIVE));
                        }
                        for (symbol, state) in symbols.iter().zip(&states) {
                            let color = if *state == Some(true) {
                                GREEN
                            } else {
                                0x24364c
                            };
                            let mut path = PathBuilder::stroke(px(1.8));
                            match symbol.kind {
                                Kind::Contact(inverted) | Kind::Edge(inverted) => {
                                    for x in [symbol.x - 8., symbol.x + 8.] {
                                        path.move_to(at(x, symbol.y - 11.));
                                        path.line_to(at(x, symbol.y + 11.));
                                    }
                                    if matches!(symbol.kind, Kind::Contact(true)) {
                                        path.move_to(at(symbol.x - 7., symbol.y + 11.));
                                        path.line_to(at(symbol.x + 7., symbol.y - 11.));
                                    }
                                    if matches!(symbol.kind, Kind::Edge(_)) {
                                        let dir = if inverted { -1. } else { 1. };
                                        path.move_to(at(symbol.x, symbol.y + 7. * dir));
                                        path.line_to(at(symbol.x, symbol.y - 7. * dir));
                                        path.move_to(at(symbol.x - 3., symbol.y - 3. * dir));
                                        path.line_to(at(symbol.x, symbol.y - 7. * dir));
                                        path.line_to(at(symbol.x + 3., symbol.y - 3. * dir));
                                    }
                                }
                                Kind::Coil => {
                                    for i in 0..=40 {
                                        let angle = i as f32 * std::f32::consts::TAU / 40.;
                                        let p = at(
                                            symbol.x + 15. * angle.cos(),
                                            symbol.y + 14. * angle.sin(),
                                        );
                                        if i == 0 {
                                            path.move_to(p);
                                        } else {
                                            path.line_to(p);
                                        }
                                    }
                                }
                                Kind::Block => continue,
                            }
                            if let Ok(path) = path.build() {
                                window.paint_path(path, rgb(color));
                            }
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full();
                let mut field = div()
                    .relative()
                    .w(px(width))
                    .h(px(height))
                    .flex_shrink_0()
                    .child(drawing);
                for symbol in &plot.symbols {
                    let state = symbol.device.and_then(|d| self.observed_bit(d));
                    let color = if self.symbol_lit(symbol) == Some(true) {
                        GREEN
                    } else {
                        TEXT
                    };
                    if matches!(symbol.kind, Kind::Block) {
                        field = field.child(
                            div()
                                .absolute()
                                .left(px(symbol.x - symbol.width / 2.))
                                .top(px(symbol.y - symbol.height / 2.))
                                .w(px(symbol.width))
                                .h(px(symbol.height))
                                .border_1()
                                .border_color(rgb(0x9aa9bb))
                                .bg(rgb(0xf8fafc))
                                .flex()
                                .items_center()
                                .justify_center()
                                .font_family("Consolas")
                                .text_xs()
                                .text_color(rgb(color))
                                .child(symbol.label.clone()),
                        );
                    } else {
                        field = field.child(
                            div()
                                .absolute()
                                .left(px(symbol.x - 50.))
                                .top(px(symbol.y - 32.))
                                .w(px(100.))
                                .text_center()
                                .font_family("Consolas")
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(color))
                                .truncate()
                                .child(match symbol.device {
                                    Some(d) if symbol.label == d.to_string() => self.device_name(d),
                                    _ => symbol.label.clone(),
                                }),
                        );
                    }
                    // Device comment above the address, as the symbol line in PLC IDEs.
                    if let Some(text) = symbol.device.and_then(|d| self.project.comment(d)) {
                        let (width, top) = if matches!(symbol.kind, Kind::Block) {
                            (symbol.width, symbol.y - symbol.height / 2. - 17.)
                        } else {
                            (104., symbol.y - 50.)
                        };
                        field = field.child(
                            div()
                                .absolute()
                                .left(px(symbol.x - width / 2.))
                                .top(px(top))
                                .w(px(width))
                                .text_center()
                                .text_xs()
                                .text_color(rgb(ACCENT))
                                .truncate()
                                .child(text.to_owned()),
                        );
                    }
                    if let Some(device) = symbol.device {
                        field = field.child(
                            div()
                                .absolute()
                                .left(px(symbol.x - 68.))
                                .top(px(symbol.y + symbol.height / 2. + 4.))
                                .w(px(136.))
                                .text_center()
                                .font_family("Consolas")
                                .text_xs()
                                .text_color(rgb(if self.bridge.is_some() && state.is_none() {
                                    0x946113
                                } else {
                                    MUTED
                                }))
                                .child(self.device_value(device)),
                        );
                    }
                }
                for (symbol_index, symbol) in plot.symbols.iter().enumerate() {
                    let target = symbol.target;
                    let active = self.ladder_ui.selection == Some((index, target));
                    let hit_width = (symbol.width + 12.).max(68.);
                    // Hit area: outline only, so the symbol and its state stay visible.
                    field = field.child(
                        div()
                            .id(SharedString::from(format!(
                                "ladder-element-{index}-{symbol_index}"
                            )))
                            .absolute()
                            .left(px(symbol.x - hit_width / 2.))
                            .top(px(symbol.y - 35.))
                            .w(px(hit_width))
                            .h(px(57.))
                            .rounded(px(4.))
                            .cursor_pointer()
                            .border_1()
                            .border_color(gpui::transparent_black())
                            .hover(|s| s.border_color(rgb(0x9fd3d8)))
                            .when(active, |e| e.border_2().border_color(rgb(ACCENT)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_element(index, target, None, window, cx)
                            })),
                    );
                    if self.ladder_editable()
                        && index == self.selected
                        && (active || self.ladder_ui.tool.is_some())
                    {
                        for (side, place) in [
                            (-1., crate::gpui_model::Placement::Before),
                            (1., crate::gpui_model::Placement::After),
                        ] {
                            let chosen = active && self.ladder_ui.placement == Some(place);
                            let mut button = Button::new(SharedString::from(format!(
                                "insert-{index}-{symbol_index}-{side}"
                            )))
                            .label("+")
                            .xsmall()
                            .absolute()
                            .left(px(symbol.x + side * (symbol.width / 2. + 34.) - 10.))
                            .top(px(symbol.y - 10.))
                            .w(px(20.))
                            .h(px(20.))
                            .tooltip(if side < 0. {
                                tr("Bu elemandan önce ekle")
                            } else {
                                tr("Bu elemandan sonra ekle")
                            });
                            if chosen {
                                button = button.primary();
                            }
                            field = field.child(button.on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.select_element(index, target, Some(place), window, cx)
                                },
                            )));
                        }
                    }
                }
                let selected = index == self.selected;
                // Network block in the style of common PLC IDEs: a header band with the
                // network number and title, then the rung on a white canvas.
                list = list.child(
                    div()
                        .id(("network", index))
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .mx_3()
                        .mt_3()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(rgb(if selected { ACCENT } else { LINE }))
                        .bg(rgb(PANEL))
                        .overflow_hidden()
                        .child(
                            div()
                                .h(px(32.))
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_2()
                                .bg(rgb(if selected { 0xe3f3f4 } else { 0xf3f5f8 }))
                                .border_b_1()
                                .border_color(rgb(LINE))
                                .child(
                                    Button::new(("rung-select", index))
                                        .label(tfmt!("Ağ {}", "Network {}", index + 1))
                                        .ghost()
                                        .xsmall()
                                        .tooltip(tr("Ağı seç"))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.selected = index;
                                            this.ladder_ui.selection = None;
                                            this.ladder_ui.placement = None;
                                            this.selected_sync = true;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(if network.comment.is_empty() {
                                            MUTED
                                        } else {
                                            TEXT
                                        }))
                                        .child(if network.comment.is_empty() {
                                            tr("Başlık yok").to_string()
                                        } else {
                                            network
                                                .comment
                                                .lines()
                                                .next()
                                                .unwrap_or_default()
                                                .to_owned()
                                        }),
                                )
                                .when(selected && self.ladder_editable(), |row| {
                                    // Network commands for the selected network.
                                    let tool =
                                        |id: &'static str, icon: IconName, tip: &'static str| {
                                            Button::new(id).icon(icon).xsmall().ghost().tooltip(tip)
                                        };
                                    row.child(
                                        tool("net-copy", IconName::Copy, tr("Ağı kopyala")).on_click(
                                            cx.listener(|t, _, _, cx| {
                                                t.network_command("copy", cx)
                                            }),
                                        ),
                                    )
                                    .child(
                                        tool("net-duplicate", IconName::Plus, tr("Ağı çoğalt"))
                                            .on_click(cx.listener(|t, _, _, cx| {
                                                t.network_command("duplicate", cx)
                                            })),
                                    )
                                    .child(
                                        tool("net-up", IconName::ArrowUp, tr("Yukarı taşı")).on_click(
                                            cx.listener(|t, _, _, cx| t.network_command("up", cx)),
                                        ),
                                    )
                                    .child(
                                        tool("net-down", IconName::ArrowDown, tr("Aşağı taşı"))
                                            .on_click(cx.listener(|t, _, _, cx| {
                                                t.network_command("down", cx)
                                            })),
                                    )
                                })
                                .when(selected, |row| {
                                    row.child(
                                        div()
                                            .px_2()
                                            .py(px(1.))
                                            .rounded(px(4.))
                                            .bg(rgb(ACCENT))
                                            .text_xs()
                                            .text_color(rgb(0xffffff))
                                            .child(tr("SEÇİLİ")),
                                    )
                                })
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .child(tfmt!("IL satır {}", "IL line {}", network.line)),
                                ),
                        )
                        .when(network.comment.lines().count() > 1, |card| {
                            card.child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(0xfafbfc))
                                    .border_b_1()
                                    .border_color(rgb(0xedf0f4))
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .children(
                                        network
                                            .comment
                                            .lines()
                                            .skip(1)
                                            .map(|line| div().child(line.to_owned())),
                                    ),
                            )
                        })
                        .child(div().min_w(px(width)).py_2().child(field)),
                );
            }
            if !program.graphically_editable() {
                list =
                    list.child(div().px_5().py_3().text_xs().text_color(rgb(MUTED)).child(
                        tr("ST / akış kontrolü: kaynak kanonik, grafik düzenleme salt okunur."),
                    ));
            } else if program.has_branches() {
                list =
                    list.child(div().px_5().py_3().text_xs().text_color(rgb(MUTED)).child(
                        tr("MPS/MRD/MPP dalları düzenlenebilir; KAYITLI ortak kısım ve dal yapısı kaynak editöründe değişir."),
                    ));
            }
        } else {
            list = list.child(
                div()
                    .p_6()
                    .text_color(rgb(0xad3b35))
                    .child(self.compile_error.clone().unwrap_or_default()),
            );
        }
        list.into_any_element()
    }
    pub(super) fn comparison_view(&self) -> AnyElement {
        let mut body=div().id("comparison-scroll").flex_1().overflow_y_scroll().p_5().flex().flex_col().gap_2()
            .child(Self::heading(tr("Tek tarama karşılaştırması · sürekli online RUN sonrasında yeniden program yazmak gerekir.")));
        if let Some((scan, mismatches)) = self.comparison {
            body = body.child(tfmt!("Tarama {scan} · {mismatches} fark", "Scan {scan} · {mismatches} differences"));
        } else {
            body = body
                .child(tr("Simülatöre bağlanıp programı yazın; doğrulanan değerler burada görünür."));
        }
        body = body.child(
            div()
                .flex()
                .gap_4()
                .py_2()
                .font_weight(FontWeight::SEMIBOLD)
                .child(div().w(px(100.)).child(tr("Cihaz")))
                .child(div().w(px(100.)).child(tr("Tip")))
                .child(div().w(px(120.)).child(tr("Yerel")))
                .child(tr("Simülatör")),
        );
        for (device, kind, local, native) in &self.extra.comparison_rows {
            body = body.child(
                div()
                    .flex()
                    .gap_4()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(if local == native { PANEL } else { 0xfff0e9 }))
                    .font_family("Consolas")
                    .child(div().w(px(100.)).child(device.clone()))
                    .child(div().w(px(100.)).child(kind.clone()))
                    .child(div().w(px(120.)).child(local.to_string()))
                    .child(native.to_string()),
            );
        }
        body.into_any_element()
    }
    pub(super) fn trend_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let points = self
            .extra
            .trend
            .iter()
            .map(|(value, _)| *value)
            .collect::<Vec<_>>();
        let device = self
            .extra
            .trend_device
            .read(cx)
            .value()
            .parse::<Device>()
            .ok();
        // Bit devices are drawn as a step trace on a fixed 0..1 scale.
        let digital = device.is_some_and(|d| d.bit() && !matches!(d.area, Area::T | Area::C));
        let (min, max) = if digital {
            (0., 1.)
        } else {
            (
                points.iter().copied().reduce(f32::min).unwrap_or(0.),
                points.iter().copied().reduce(f32::max).unwrap_or(1.),
            )
        };
        // Statistics use the real extremes; the axis is padded so a flat trace sits in
        // the middle instead of on the bottom edge.
        let (low, high) = (min, max);
        let (min, max) = if digital || max - min >= 1. {
            (min, max)
        } else {
            (min - 1., max + 1.)
        };
        let span = (max - min).max(1.);
        let current = points.last().copied();
        let average =
            (!points.is_empty()).then(|| points.iter().sum::<f32>() / points.len() as f32);
        let has_points = !points.is_empty();
        let chart = canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                for step in 0..=4 {
                    let y = bounds.top() + bounds.size.height * step as f32 / 4.;
                    window.paint_quad(fill(
                        Bounds::new(point(bounds.left(), y), size(bounds.size.width, px(1.))),
                        rgb(0xe8ecf1),
                    ));
                }
                if points.len() < 2 {
                    return;
                }
                let x_of = |i: usize| {
                    bounds.left() + bounds.size.width * i as f32 / (points.len() - 1) as f32
                };
                let y_of = |v: f32| {
                    bounds.bottom()
                        - bounds.size.height * 0.92 * ((v - min) / span)
                        - bounds.size.height * 0.04
                };
                let mut trace = Vec::with_capacity(points.len() * 2);
                for (i, value) in points.iter().enumerate() {
                    let p = point(x_of(i), y_of(*value));
                    if digital && let Some(&last) = trace.last() {
                        let last: Point<Pixels> = last;
                        trace.push(point(p.x, last.y));
                    }
                    trace.push(p);
                }
                let mut area = PathBuilder::fill();
                area.move_to(point(trace[0].x, bounds.bottom()));
                for p in &trace {
                    area.line_to(*p);
                }
                area.line_to(point(trace[trace.len() - 1].x, bounds.bottom()));
                area.close();
                if let Ok(path) = area.build() {
                    window.paint_path(path, rgba(0x00838f1f));
                }
                let mut line = PathBuilder::stroke(px(2.));
                line.move_to(trace[0]);
                for p in &trace[1..] {
                    line.line_to(*p);
                }
                if let Ok(path) = line.build() {
                    window.paint_path(path, rgb(ACCENT));
                }
            },
        )
        .flex_1()
        .h(px(280.));
        let format = |v: Option<f32>| match v {
            Some(v) if digital => (if v >= 0.5 { "1" } else { "0" }).to_string(),
            Some(v) if (v - v.round()).abs() < 0.005 => format!("{v:.0}"),
            Some(v) => format!("{v:.2}"),
            None => "—".into(),
        };
        let stat = |label: &'static str, value: String, accent: bool| {
            div()
                .flex_1()
                .p_3()
                .rounded(px(6.))
                .border_1()
                .border_color(rgb(LINE))
                .bg(rgb(PANEL))
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_xs().text_color(rgb(MUTED)).child(label))
                .child(
                    div()
                        .text_xl()
                        .font_family("Consolas")
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(if accent { ACCENT } else { TEXT }))
                        .child(value),
                )
        };
        let axis_label = |v: f32| {
            div()
                .text_xs()
                .font_family("Consolas")
                .text_color(rgb(MUTED))
                .child(format(Some(v)))
        };
        let names = self
            .history
            .back()
            .map(|(_, values)| values.iter().map(|(d, _)| d.clone()).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut history = div()
            .id("trend-history")
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
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
                    .child(div().w(px(80.)).px_3().child(tr("Sıra")))
                    .children(
                        names
                            .iter()
                            .map(|n| div().w(px(90.)).px_2().child(n.clone())),
                    ),
            );
        if self.history.is_empty() {
            history = history.child(
                div()
                    .p_4()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(tr("Gözlem kaydı SS2 simülatörü veya PLC oturumunda tutulur.")),
            );
        }
        for (seq, values) in self.history.iter().rev().take(120) {
            history = history.child(
                div()
                    .flex()
                    .h(px(26.))
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(0xedf0f4))
                    .font_family("Consolas")
                    .text_xs()
                    .child(
                        div()
                            .w(px(80.))
                            .px_3()
                            .text_color(rgb(MUTED))
                            .child(format!("#{seq:05}")),
                    )
                    .children(
                        values
                            .iter()
                            .map(|(_, v)| div().w(px(90.)).px_2().child(v.to_string())),
                    ),
            );
        }
        let title = match device {
            Some(d) => match self.project.comment(d) {
                Some(comment) => tfmt!("Trend · {d} · {comment}", "Trend · {d} · {comment}"),
                None => format!("Trend · {d}"),
            },
            None => tr("Trend · geçersiz cihaz adresi").into(),
        };
        div()
            .p_4()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .bg(rgb(BG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(div().flex_1())
                    .child(Self::heading(tr("Cihaz")))
                    .child(Input::new(&self.extra.trend_device).small().w(px(120.)))
                    .child(
                        self.button("trend-clear", tr("Geçmişi temizle"), false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.extra.trend.clear();
                                this.history.clear();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(stat(tr("Güncel"), format(current), true))
                    .child(stat(tr("En düşük"), format(has_points.then_some(low)), false))
                    .child(stat(tr("En yüksek"), format(has_points.then_some(high)), false))
                    .child(stat(tr("Ortalama"), format(average), false))
                    .child(stat(tr("Örnek"), self.extra.trend.len().to_string(), false)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(PANEL))
                    .child(
                        div()
                            .w(px(56.))
                            .h(px(280.))
                            .flex()
                            .flex_col()
                            .justify_between()
                            .items_end()
                            .child(axis_label(max))
                            .child(axis_label((max + min) / 2.))
                            .child(axis_label(min)),
                    )
                    .child(chart),
            )
            .child(Self::heading(
                tr("Son gözlemler · sıra numarası PLC tarama sayısı değildir"),
            ))
            .child(history)
            .into_any_element()
    }
}
