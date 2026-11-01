//! Online/offline comparison view: project networks next to the program read from the
//! simulator or PLC, matched by content.
use super::*;
use crate::program_diff::{self, Status};

impl Workbench {
    /// Read the program from the PLC with the active connection profile (read only).
    fn read_physical_program(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.extra.hardware.is_some() {
            return;
        }
        self.sync_profile(cx);
        match self.extra.profiles.active_config() {
            Ok(config) => {
                self.read_origin = "PLC".into();
                self.log(tr("PLC programı okunuyor (yalnız okuma)…"));
                self.dispatch(
                    async move { Done::Read(crate::plc_connection::read_plc_program(config).await) },
                    cx,
                );
            }
            Err(e) => {
                self.log(e);
                cx.notify();
            }
        }
    }

    pub(super) fn diff_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let header = div()
            .px_4()
            .py_3()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr("Online / offline karşılaştırma")),
            )
            .child(div().flex_1())
            .child(
                self.button(
                    "diff-read-sim",
                    tr("Simülatörden oku"),
                    self.bridge.is_none() || self.busy || self.running,
                )
                .on_click(cx.listener(|this, _, _, cx| this.read(cx))),
            )
            .child(
                self.button(
                    "diff-read-plc",
                    tr("PLC'den oku"),
                    self.busy || self.bridge.is_some(),
                )
                .tooltip(tr("Etkin bağlantı profiliyle programı END'e kadar okur; PLC'ye yazılmaz"))
                .on_click(cx.listener(|this, _, _, cx| this.read_physical_program(cx))),
            );
        let Some((plc_source, words)) = &self.read_preview else {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .bg(rgb(BG))
                .child(header)
                .child(
                    div()
                        .p_6()
                        .text_color(rgb(MUTED))
                        .child(tr("Karşılaştırmak için programı simülatörden veya PLC'den okuyun.")),
                )
                .into_any_element();
        };
        let rows = match program_diff::compare(&self.project.layout().il, plc_source) {
            Ok(rows) => rows,
            Err(e) => {
                return div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .bg(rgb(BG))
                    .child(header)
                    .child(div().p_6().text_color(rgb(ERROR)).child(e))
                    .into_any_element();
            }
        };
        let differences = rows.iter().filter(|r| r.status != Status::Same).count();
        let summary = div()
            .mx_3()
            .mt_3()
            .p_3()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(if differences == 0 { 0x86c9a1 } else { 0xf59e0b }))
            .bg(rgb(if differences == 0 { 0xeaf6ef } else { 0xfff7e6 }))
            .child(if differences == 0 {
                tfmt!(
                    "Proje ile {} programı aynı · {} ağ · {} WORD", "The project and the {} program are identical · {} networks · {} WORD",
                    self.read_origin,
                    rows.len(),
                    words
                )
            } else {
                tfmt!(
                    "{differences} ağ farklı · proje ile {} programı karşılaştırıldı ({} WORD)", "{differences} networks differ · project compared with the {} program ({} WORD)",
                    self.read_origin, words
                )
            });
        let column = |lines: &Option<(usize, Vec<String>)>, label: &str| match lines {
            Some((index, code)) => div()
                .flex_1()
                .min_w_0()
                .p_2()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(tfmt!("{label} · Ağ {}", "{label} · Network {}", index + 1)),
                )
                .child(
                    div()
                        .font_family("Consolas")
                        .text_sm()
                        .children(code.iter().map(|l| div().child(l.clone()))),
                ),
            None => div()
                .flex_1()
                .p_2()
                .text_sm()
                .text_color(rgb(MUTED))
                .child(tfmt!("{label}: yok", "{label}: none")),
        };
        let mut list = div()
            .id("diff-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2();
        for (row_index, row) in rows.iter().enumerate() {
            let (label, color, bg) = match row.status {
                Status::Same => (tr("Aynı"), GREEN, PANEL),
                Status::Different => (tr("Farklı"), AMBER, 0xfff7e6),
                Status::OnlyProject => (tr("Yalnız projede"), 0x2563eb, 0xeef4ff),
                Status::OnlyPlc => (tr("Yalnız PLC'de"), 0x7c3aed, 0xf5f0ff),
            };
            let target = row.project.as_ref().map(|(i, _)| *i);
            list = list.child(
                div()
                    .id(("diff-row", row_index))
                    .flex()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(bg))
                    .when_some(target, |row, network| {
                        row.cursor_pointer().on_click(
                            cx.listener(move |this, _, _, cx| this.jump_to_network(network, cx)),
                        )
                    })
                    .child(
                        div()
                            .w(px(120.))
                            .flex_shrink_0()
                            .p_2()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(color))
                            .child(label),
                    )
                    .child(column(&row.project, tr("Proje")))
                    .child(div().w(px(1.)).bg(rgb(LINE)))
                    .child(column(&row.plc, &self.read_origin)),
            );
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(header)
            .child(summary)
            .child(list)
            .into_any_element()
    }
}
