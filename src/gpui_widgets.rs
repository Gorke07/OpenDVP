use super::*;
use gpui_component::{
    Icon, IconName,
    list::ListItem,
    tab::{Tab as KitTab, TabBar},
    table::{Column, DataTable, TableDelegate, TableState},
    tree::{TreeItem, TreeState, tree},
};

pub(super) struct WorkspaceUi {
    pub tree: Entity<TreeState>,
    pub registers: Entity<TableState<Registers>>,
    pub watch_mode: bool,
    pub focus_mode: bool,
    pub inspector_open: bool,
    /// Bottom inspector tab: 0 properties, 1 info/log.
    pub bottom_tab: usize,
    project_name: String,
    unit_labels: Vec<String>,
    tree_english: bool,
    register_rows: Vec<[String; 4]>,
}
pub(super) struct Registers {
    columns: Vec<Column>,
    rows: Vec<[String; 4]>,
    owner: WeakEntity<Workbench>,
}
impl TableDelegate for Registers {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }
    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }
    fn column(&self, index: usize, _: &App) -> Column {
        self.columns[index].clone()
    }
    fn render_td(
        &mut self,
        row: usize,
        col: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        if col == 3 {
            let owner = self.owner.clone();
            let device = self.rows[row][0].clone();
            return Button::new(("watch-remove", row))
                .icon(IconName::Close)
                .tooltip(tr("İzlemeden kaldır"))
                .ghost()
                .xsmall()
                .on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.project.watch.retain(|d| d != &device);
                        this.dirty = true;
                        cx.notify();
                    });
                })
                .into_any_element();
        }
        div()
            .font_family(if col < 2 { "Consolas" } else { "Segoe UI" })
            .text_sm()
            .text_color(rgb(if col == 2 { MUTED } else { TEXT }))
            .child(self.rows[row][col].clone())
            .into_any_element()
    }
    fn render_empty(
        &mut self,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .p_4()
            .text_sm()
            .child(tr("İzlemek için bir cihaz adresi ekleyin."))
    }
}
/// Tree labels of the extra programs, in order (`name · pointer`).
fn unit_labels(project: &Project) -> Vec<String> {
    project
        .units
        .iter()
        .map(|u| format!("{}|{}", u.kind.label(), u.label()))
        .collect()
}
/// Tree nodes of the units of one kind: id `unit-<program index>`, label from the unit.
fn unit_nodes(units: &[String], kind: &str) -> Vec<TreeItem> {
    units
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            let (tag, label) = entry.split_once('|')?;
            (tag == kind).then(|| TreeItem::new(format!("unit-{}", i + 1), label.to_owned()))
        })
        .collect()
}
fn items(name: &str, units: &[String]) -> Vec<TreeItem> {
    let group = |id: &'static str, label: &'static str, kind: UnitKind| {
        let nodes = unit_nodes(units, kind.label());
        (!nodes.is_empty()).then(|| TreeItem::new(id, tr(label)).expanded(true).children(nodes))
    };
    let task_groups = [
        group("interrupts", "Kesmeler", UnitKind::Interrupt),
        group("subroutines", "Alt programlar", UnitKind::Subroutine),
    ];
    vec![
        TreeItem::new("project", name.to_owned())
            .expanded(true)
            .child(
                TreeItem::new("program", tr("Program"))
                    .expanded(true)
                    .child(
                        TreeItem::new("cyclic", tr("Döngüsel görev"))
                            .expanded(true)
                            .child(TreeItem::new("ladder", tr("Main · Ladder")))
                            .child(TreeItem::new("source", tr("Main · Kaynak")))
                            .children(unit_nodes(units, UnitKind::Cyclic.label())),
                    )
                    .children(task_groups.into_iter().flatten())
                    .child(TreeItem::new("comments", tr("Cihaz açıklamaları")))
                    .child(TreeItem::new("symbols", tr("Semboller")))
                    .child(TreeItem::new("xref", tr("Çapraz referans")))
                    .child(TreeItem::new("search", tr("Programda ara"))),
            )
            .child(
                TreeItem::new("controller", tr("Kontrolör"))
                    .expanded(true)
                    .child(TreeItem::new("hardware", tr("Cihaz yapılandırması")))
                    .child(TreeItem::new("connection", tr("Bağlantı profilleri")))
                    .child(TreeItem::new("io", tr("Giriş / çıkış")))
                    .child(TreeItem::new("watch", tr("İzleme tablosu")))
                    .child(TreeItem::new("recipes", tr("Parametreler / reçeteler")))
                    .child(TreeItem::new("diff", tr("Online / offline karşılaştırma")))
                    .child(TreeItem::new("trend", "Trend"))
                    .child(TreeItem::new("comparison", tr("Karşılaştırma"))),
            )
            .child(
                TreeItem::new("resources", tr("Kaynaklar"))
                    .expanded(true)
                    .child(TreeItem::new("modules", tr("Genişleme modülleri")))
                    .child(TreeItem::new("memory", tr("Dosya belleği")))
                    .child(TreeItem::new("help", tr("Komut kütüphanesi")))
                    .child(TreeItem::new("log", tr("Tanılama"))),
            ),
    ]
}
impl WorkspaceUi {
    pub fn new(project: &Project, window: &mut Window, cx: &mut Context<Workbench>) -> Self {
        let tree = cx.new(|cx| TreeState::new(cx).items(items(&project.name, &unit_labels(project))));
        cx.observe(&tree, |this, state, cx| {
            let id = state
                .read(cx)
                .selected_entry()
                .map(|e| e.item().id.to_string());
            let tab = match id.as_deref() {
                Some("ladder") => {
                    this.select_program(0, cx);
                    Some(Tab::Ladder)
                }
                Some("source") => {
                    this.select_program(0, cx);
                    Some(Tab::Source)
                }
                Some(unit) if unit.starts_with("unit-") => {
                    let index = unit["unit-".len()..].parse().unwrap_or(0);
                    this.select_program(index, cx);
                    Some(Tab::Ladder)
                }
                Some("comments") => Some(Tab::Comments),
                Some("symbols") => Some(Tab::Symbols),
                Some("xref") => Some(Tab::CrossRef),
                Some("connection") => Some(Tab::Connection),
                Some("hardware") => Some(Tab::Hardware),
                Some("io") => Some(Tab::Io),
                Some("watch") => Some(Tab::Watch),
                Some("recipes") => Some(Tab::Recipes),
                Some("diff") => Some(Tab::Diff),
                Some("trend") => Some(Tab::Trend),
                Some("comparison") => Some(Tab::Comparison),
                Some("modules") => Some(Tab::Modules),
                Some("memory") => Some(Tab::FileMemory),
                Some("search") => Some(Tab::Search),
                Some("help") => Some(Tab::Help),
                Some("log") => Some(Tab::Log),
                Some("settings") => Some(Tab::Settings),
                _ => None,
            };
            if let Some(tab) = tab {
                this.tab = tab;
                cx.notify();
            }
        })
        .detach();
        let owner = cx.weak_entity();
        let registers = cx.new(|cx| {
            TableState::new(
                Registers {
                    columns: vec![
                        Column::new("device", tr("Cihaz")).width(px(68.)).movable(false),
                        Column::new("value", tr("Değer")).width(px(110.)).movable(false),
                        Column::new("comment", tr("Açıklama"))
                            .width(px(130.))
                            .movable(false),
                        Column::new("remove", "")
                            .width(px(30.))
                            .movable(false)
                            .resizable(false),
                    ],
                    rows: vec![],
                    owner,
                },
                window,
                cx,
            )
            .col_selectable(false)
        });
        Self {
            tree,
            registers,
            watch_mode: false,
            focus_mode: opendvp::settings::get().focus_mode,
            inspector_open: opendvp::settings::get().inspector_open,
            bottom_tab: 0,
            project_name: project.name.clone(),
            unit_labels: unit_labels(project),
            tree_english: opendvp::i18n::is_en(),
            register_rows: vec![],
        }
    }
}
impl Workbench {
    pub(super) fn sync_widgets(&mut self, cx: &mut Context<Self>) {
        let labels = unit_labels(&self.project);
        if self.ui.project_name != self.project.name
            || self.ui.unit_labels != labels
            || self.ui.tree_english != opendvp::i18n::is_en()
        {
            self.ui.project_name = self.project.name.clone();
            self.ui.tree_english = opendvp::i18n::is_en();
            let nodes = items(&self.project.name, &labels);
            self.ui.unit_labels = labels;
            self.ui
                .tree
                .update(cx, |state, cx| state.set_items(nodes, cx));
        }
        let rows = self
            .project
            .watch
            .iter()
            .filter_map(|name| {
                name.parse::<Device>().ok().map(|d| {
                    let value = if self.extra.watch_hex
                        && self.bridge.is_none()
                        && self.extra.hardware.is_none()
                    {
                        format!("{:04X}", self.simulator.memory.word(d) as u16)
                    } else {
                        self.device_value(d)
                            .replace("bit=", "")
                            .replace("word=", "")
                            .replace("dword=", "")
                    };
                    let comment = self.project.comment(d).unwrap_or_default().to_owned();
                    [name.clone(), value, comment, String::new()]
                })
            })
            .collect::<Vec<_>>();
        if rows != self.ui.register_rows {
            let count_changed = rows.len() != self.ui.register_rows.len();
            self.ui.register_rows = rows.clone();
            self.ui.registers.update(cx, |table, cx| {
                table.delegate_mut().rows = rows;
                if count_changed {
                    table.refresh(cx);
                }
                cx.notify();
            });
        }
    }
    pub(super) fn project_tree(&self) -> AnyElement {
        div()
            .size_full()
            .bg(rgb(0xf5f7fa))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(34.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_3()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child(tr("PROJE GEZGİNİ")),
            )
            .child(div().flex_1().min_h_0().py_2().child(tree(
                &self.ui.tree,
                |index, entry, _, _, _| {
                    let folder = entry.is_folder();
                    let icon = if folder {
                        if entry.is_expanded() {
                            IconName::FolderOpen
                        } else {
                            IconName::FolderClosed
                        }
                    } else {
                        match entry.item().id.as_ref() {
                            "connection" => IconName::Settings,
                            "memory" => IconName::File,
                            "source" => IconName::SquareTerminal,
                            _ => IconName::File,
                        }
                    };
                    ListItem::new(index)
                        .pl(px(8. + entry.depth() as f32 * 14.))
                        .h(px(28.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .h_full()
                                .child(
                                    Icon::new(icon)
                                        .size(px(14.))
                                        .flex_shrink_0()
                                        .text_color(rgb(if folder { 0x8491a3 } else { ACCENT })),
                                )
                                .child(div().text_sm().child(entry.item().label.clone())),
                        )
                },
            )))
            .into_any_element()
    }
    pub(super) fn inspector_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        TabBar::new("inspector-tabs")
            .underline()
            .small()
            .selected_index(usize::from(self.ui.watch_mode))
            .child(KitTab::new().label(tr("Düzenle")))
            .child(KitTab::new().label(tr("Cihaz izleme")))
            .on_click(cx.listener(|this, index, _, cx| {
                this.ui.watch_mode = *index == 1;
                cx.notify();
            }))
            .into_any_element()
    }
    pub(super) fn register_table(&self) -> AnyElement {
        div()
            .relative()
            .w_full()
            .h(px(260.))
            .min_h(px(100.))
            .flex_shrink_0()
            .child(
                div().absolute().inset_0().child(
                    DataTable::new(&self.ui.registers)
                        .small()
                        .stripe(true)
                        .bordered(false),
                ),
            )
            .into_any_element()
    }
}
