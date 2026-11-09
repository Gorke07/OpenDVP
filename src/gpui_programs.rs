//! Program units: Main plus subroutines and interrupt routines (see `project::Unit`).
//! One program is edited at a time; the PLC always runs the joined program.
use super::*;
use gpui_component::{
    IconName,
    menu::{DropdownMenu, PopupMenuItem},
};
use opendvp::project::UnitKind;
use std::rc::Rc;

impl Workbench {
    /// Source text of the program being edited (Main until another is selected).
    pub(super) fn active_source(&self) -> &str {
        self.project.program_source(self.active_program)
    }
    /// The networks the ladder shows: those of the selected program.
    pub(super) fn view_program(&self) -> Option<&Program> {
        self.view.as_ref().or(self.program.as_ref())
    }
    /// Recompute the selected program's view after a compile or a selection change.
    pub(super) fn refresh_view(&mut self) {
        self.view = match (&self.program, self.project.units.is_empty()) {
            (Some(program), false) => Some(self.project.program_view(program, self.active_program)),
            _ => None,
        };
    }
    /// The compiler that validates graphical edits of the selected program inside the
    /// joined program (a unit alone is not a valid program); `None` for a single program.
    /// Owned, so the edit closure may still borrow the workbench.
    pub(super) fn model_compiler(&self) -> Option<crate::gpui_model::Compiler> {
        if self.project.units.is_empty() {
            return None;
        }
        let project = self.project.clone();
        let index = self.active_program;
        Some(Rc::new(move |text| project.compile_program_text(index, text)))
    }
    pub(super) fn select_program(&mut self, index: usize, cx: &mut Context<Self>) {
        let index = index.min(self.project.units.len());
        if index != self.active_program {
            self.active_program = index;
            // Undo history belongs to one program's text.
            self.undo.clear();
            self.redo.clear();
            self.ladder_ui.selection = None;
            self.ladder_ui.placement = None;
            self.selected = 0;
            self.selected_sync = true;
            self.editor_sync = true;
            self.unit_sync = true;
            self.refresh_view();
            // The error mark belongs to the program it was found in.
            self.shown_mark = None;
            self.compile_mark = None;
            if self.compile_error.is_some() {
                self.compile();
            }
        }
        cx.notify();
    }
    pub(super) fn add_program(&mut self, kind: UnitKind, cx: &mut Context<Self>) {
        if self.editing_locked() {
            return;
        }
        match self.project.add_unit(kind) {
            Ok(index) => {
                self.dirty = true;
                self.uploaded = false;
                self.compile();
                self.select_program(index, cx);
                self.log(tfmt!(
                    "{} eklendi: {}",
                    "{} added: {}",
                    tr(kind.label()),
                    self.project.program_name(index)
                ));
            }
            Err(e) => self.log(e),
        }
        cx.notify();
    }
    pub(super) fn remove_program(&mut self, cx: &mut Context<Self>) {
        let index = self.active_program;
        if index == 0 || self.editing_locked() {
            return;
        }
        let name = self.project.program_name(index).to_owned();
        self.project.remove_unit(index);
        self.active_program = 0;
        self.undo.clear();
        self.redo.clear();
        self.ladder_ui.selection = None;
        self.selected = 0;
        self.selected_sync = true;
        self.editor_sync = true;
        self.unit_sync = true;
        self.dirty = true;
        self.uploaded = false;
        self.compile();
        self.log(tfmt!("{name} silindi.", "{name} deleted."));
        cx.notify();
    }
    /// Apply the name and pointer number typed for the selected unit.
    pub(super) fn apply_program_settings(&mut self, cx: &mut Context<Self>) {
        let index = self.active_program;
        if index == 0 || self.editing_locked() {
            return;
        }
        let name = self.extra.unit_name.read(cx).value().to_string();
        let number = self.extra.unit_number.read(cx).value().trim().parse::<u16>();
        let has_pointer = self.project.units[index - 1].kind.has_pointer();
        let result = self.project.rename_unit(index, &name).and_then(|()| match number {
            _ if !has_pointer => Ok(()),
            Ok(n) => self.project.set_unit_number(index, n),
            Err(_) => Err(tr("İşaretçi numarası bir sayı olmalı.").into()),
        });
        match result {
            Ok(()) => {
                self.dirty = true;
                self.uploaded = false;
                self.compile();
                self.log(tr("Program ayarları uygulandı."));
            }
            Err(e) => self.log(e),
        }
        self.unit_sync = true;
        cx.notify();
    }
    /// Write the selected program (and the symbols it uses) as a package file.
    pub(super) fn export_package(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let json = match opendvp::program_pack::export(&self.project, &[self.active_program]) {
            Ok(json) => json,
            Err(e) => {
                self.log(e);
                return cx.notify();
            }
        };
        let name = format!("{}.odprog", self.project.program_name(self.active_program));
        let task = panels::dialog(move || {
            rfd::FileDialog::new()
                .add_filter(tr("Program paketi"), &["odprog"])
                .set_file_name(name)
                .save_file()
                .map(|path| {
                    std::fs::write(&path, json)
                        .map(|_| path.display().to_string())
                        .map_err(|e| e.to_string())
                })
        });
        self.dispatch(async move { Done::PackSaved(task.await) }, cx);
    }
    /// Pick a package file; the import itself runs when the text arrives.
    pub(super) fn import_package_dialog(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.editing_locked() {
            return;
        }
        let task = panels::dialog(|| {
            rfd::FileDialog::new()
                .add_filter(tr("Program paketi"), &["odprog"])
                .pick_file()
                .map(|path| opendvp::program_pack::read_file(&path))
        });
        self.dispatch(async move { Done::PackRead(task.await) }, cx);
    }
    pub(super) fn import_package_text(&mut self, text: Result<String, String>, cx: &mut Context<Self>) {
        let outcome = text.and_then(|json| opendvp::program_pack::import(&mut self.project, &json));
        match outcome {
            Ok(done) => {
                self.dirty = true;
                self.uploaded = false;
                self.unit_sync = true;
                self.compile();
                for note in &done.notes {
                    self.log(note.clone());
                }
                self.log(tfmt!(
                    "{} program içe aktarıldı (devre dışı): {}",
                    "{} programs imported (disabled): {}",
                    done.programs.len(),
                    done.programs.join(", ")
                ));
                if let Some(last) = self.project.program_count().checked_sub(1) {
                    self.select_program(last, cx);
                }
            }
            Err(e) => self.log(e),
        }
        cx.notify();
    }
    /// Keep the selected program in the user library folder.
    pub(super) fn save_to_library(&mut self, cx: &mut Context<Self>) {
        let name = self.project.program_name(self.active_program).to_owned();
        let result = opendvp::program_pack::export(&self.project, &[self.active_program]).and_then(|json| {
            let dir = opendvp::program_pack::library_dir().ok_or("Kütüphane klasörü bulunamadı")?;
            opendvp::program_pack::library_save_in(&dir, &name, &json)
        });
        match result {
            Ok(path) => self.log(tfmt!(
                "Kütüphaneye eklendi: {}",
                "Added to the library: {}",
                path.display()
            )),
            Err(e) => self.log(e),
        }
        cx.notify();
    }
    /// Split a flat Main (FEND, `P…SRET`, `I…IRET` blocks) into Main plus programs.
    pub(super) fn split_program(&mut self, cx: &mut Context<Self>) {
        if self.editing_locked() {
            return;
        }
        match self.project.split_main() {
            Ok(count) => {
                self.active_program = 0;
                self.undo.clear();
                self.redo.clear();
                self.editor_sync = true;
                self.program_changed(cx);
                self.log(tfmt!(
                    "Main {count} ek programa ayrıldı.",
                    "Main was split into {count} more programs."
                ));
            }
            Err(e) => {
                self.log(e);
                cx.notify();
            }
        }
    }
    /// Take the selected unit out of / back into the compiled program.
    pub(super) fn toggle_program_enabled(&mut self, cx: &mut Context<Self>) {
        let index = self.active_program;
        if index == 0 || self.editing_locked() {
            return;
        }
        let enabled = !self.project.units[index - 1].enabled;
        self.project.set_unit_enabled(index, enabled);
        self.program_changed(cx);
        self.log(if enabled {
            tr("Program etkinleştirildi.")
        } else {
            tr("Program devre dışı bırakıldı; derlemeye girmez.")
        });
    }
    /// Move the selected unit one place up or down the list (execution order of cyclic parts).
    pub(super) fn move_program(&mut self, up: bool, cx: &mut Context<Self>) {
        if self.editing_locked() {
            return;
        }
        if let Some(index) = self.project.move_unit(self.active_program, up) {
            self.active_program = index;
            self.program_changed(cx);
        }
    }
    /// Pick the trigger of the selected interrupt routine from the documented labels.
    pub(super) fn set_interrupt_source(&mut self, number: u16, cx: &mut Context<Self>) {
        let index = self.active_program;
        if index == 0 || self.editing_locked() {
            return;
        }
        match self.project.set_unit_number(index, number) {
            Ok(()) => self.program_changed(cx),
            Err(e) => {
                self.log(e);
                cx.notify();
            }
        }
    }
    /// The program list, an order or a pointer changed: recompile and refill the inputs.
    fn program_changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.uploaded = false;
        self.unit_sync = true;
        self.compile();
        cx.notify();
    }
    /// Program and network (numbered inside that program) of network `network` of the
    /// joined program.
    pub(super) fn network_home(&self, network: usize) -> Option<(usize, usize)> {
        let whole = self.program.as_ref()?;
        let line = whole.networks.get(network)?.line;
        let (unit, _) = self.project.locate(line)?;
        let first = whole
            .networks
            .iter()
            .position(|n| self.project.locate(n.line).is_some_and(|(u, _)| u == unit))?;
        Some((unit, network - first))
    }
    /// Label of a document tab; the ladder and source tabs name the selected program.
    pub(super) fn tab_text(&self, tab: Tab) -> String {
        let name = self.project.program_name(self.active_program);
        match tab {
            Tab::Ladder => format!("{name} · Ladder"),
            Tab::Source => format!("{name} · {}", tr("Kaynak")),
            other => other.tab_label(),
        }
    }
    /// Chips for Main and every unit, plus the add buttons and the unit's own settings.
    pub(super) fn program_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let locked = self.editing_locked();
        let chip = |index: usize| {
            let active = index == self.active_program;
            let disabled = index > 0 && !self.project.units[index - 1].enabled;
            let label = match index {
                0 => "Main".to_owned(),
                i => self.project.units[i - 1].label(),
            };
            div()
                .id(SharedString::from(format!("program-chip-{index}")))
                .px_3()
                .py(px(3.))
                .rounded(px(4.))
                .border_1()
                .border_color(rgb(if active { ACCENT } else { LINE }))
                .bg(rgb(if active { 0xe3f2f4 } else { PANEL }))
                .text_color(rgb(if active {
                    ACCENT
                } else if disabled {
                    MUTED
                } else {
                    TEXT
                }))
                .font_weight(if active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .cursor_pointer()
                .hover(|style| style.bg(rgb(0xeef5f6)))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.select_program(index, cx)))
        };
        let mut row = div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py(px(5.))
            .flex_wrap()
            .border_b_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .text_xs();
        for index in 0..self.project.program_count() {
            row = row.child(chip(index));
        }
        let view = cx.entity();
        let can_split = self.project.can_split();
        row = row.child(div().flex_1()).child(
            self.button("program-actions", tr("Program işlemleri"), false)
                .ghost()
                .dropdown_menu(move |menu, window, cx| {
                    let action = |label: &'static str, disabled, f: fn(&mut Workbench, &mut Context<Workbench>)| {
                        let view = view.clone();
                        PopupMenuItem::new(label).disabled(disabled).on_click(move |_, _, cx| {
                            view.update(cx, f);
                        })
                    };
                    let library_view = view.clone();
                    menu.label(tr("Yeni program"))
                        .item(action(tr("Program parçası"), locked, |this, cx| this.add_program(UnitKind::Cyclic, cx)))
                        .item(action(tr("Alt program"), locked, |this, cx| this.add_program(UnitKind::Subroutine, cx)))
                        .item(action(tr("Kesme"), locked, |this, cx| this.add_program(UnitKind::Interrupt, cx)))
                        .separator()
                        .item(action(tr("Dışa aktar…"), false, Workbench::export_package))
                        .item(action(tr("İçe aktar…"), locked, Workbench::import_package_dialog))
                        .submenu(tr("Kütüphane"), window, cx, move |menu, _, _| library_menu(menu, library_view.clone()))
                        .when(can_split, |menu| menu.item(action(tr("Alt programlara ayır"), locked, Workbench::split_program)))
                }),
        );
        if self.active_program > 0 {
            let unit = &self.project.units[self.active_program - 1];
            let (kind, enabled) = (unit.kind, unit.enabled);
            row = row
                .child(div().w(px(1.)).h(px(18.)).bg(rgb(LINE)))
                .child(div().text_color(rgb(MUTED)).child(tr("Ad")))
                .child(div().w(px(130.)).child(Input::new(&self.extra.unit_name).small()))
                .when(kind.has_pointer(), |row| {
                    row.child(div().text_color(rgb(MUTED)).child(tr("No")))
                        .child(div().w(px(56.)).child(Input::new(&self.extra.unit_number).small()))
                })
                .child(
                    self.button("unit-apply", tr("Uygula"), locked)
                        .on_click(cx.listener(|this, _, _, cx| this.apply_program_settings(cx))),
                )
                .when(kind == UnitKind::Interrupt, |row| {
                    let view = cx.entity();
                    row.child(
                        self.button("unit-source", tr("Kaynak seç"), locked)
                            .ghost()
                            .tooltip(tr("Kesmenin tetikleyicisini belgelenmiş etiketlerden seç"))
                            .dropdown_menu(move |menu, _, _| interrupt_menu(menu, view.clone())),
                    )
                })
                .child(
                    self.button(
                        "unit-enabled",
                        if enabled { tr("Devre dışı bırak") } else { tr("Etkinleştir") },
                        locked,
                    )
                    .ghost()
                    .tooltip(tr("Devre dışı program projede kalır ama derlemeye girmez"))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_program_enabled(cx))),
                )
                .child(
                    Button::new("unit-up")
                        .icon(IconName::ArrowUp)
                        .ghost()
                        .small()
                        .disabled(locked)
                        .tooltip(tr("Yukarı taşı"))
                        .on_click(cx.listener(|this, _, _, cx| this.move_program(true, cx))),
                )
                .child(
                    Button::new("unit-down")
                        .icon(IconName::ArrowDown)
                        .ghost()
                        .small()
                        .disabled(locked)
                        .tooltip(tr("Aşağı taşı"))
                        .on_click(cx.listener(|this, _, _, cx| this.move_program(false, cx))),
                )
                .child(
                    self.button("unit-remove", tr("Programı sil"), locked)
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| this.remove_program(cx))),
                );
        }
        row.into_any_element()
    }
}

/// The documented interrupt labels, grouped by trigger: external X0..X7 (rising / falling
/// edge), high-speed counter and communication. Timer labels carry their period (I6xx..I8xx),
/// so they are typed in.
fn interrupt_menu(
    mut menu: gpui_component::menu::PopupMenu,
    view: Entity<Workbench>,
) -> gpui_component::menu::PopupMenu {
    let item = |menu: gpui_component::menu::PopupMenu, number: u16| {
        let view = view.clone();
        let label = format!(
            "{} · {}",
            UnitKind::Interrupt.pointer(number),
            UnitKind::Interrupt.pointer_note(number).unwrap_or_default()
        );
        menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
            view.update(cx, |this, cx| this.set_interrupt_source(number, cx));
        }))
    };
    menu = menu.label(tr("Dış kesme"));
    for input in 0..8 {
        for rising in [1, 0] {
            menu = item(menu, 100 * input + rising);
        }
    }
    menu = menu.label(tr("Hızlı sayıcı"));
    for slot in 1..=8 {
        menu = item(menu, 10 * slot);
    }
    menu = menu.label(tr("Haberleşme"));
    for number in [140, 150, 160] {
        menu = item(menu, number);
    }
    menu.label(tr("Zaman kesmesi: numarayı yazın (I602…I899)"))
}

/// The user library: add the selected program, then every package found in the folder.
fn library_menu(
    mut menu: gpui_component::menu::PopupMenu,
    view: Entity<Workbench>,
) -> gpui_component::menu::PopupMenu {
    {
        let view = view.clone();
        menu = menu.item(
            PopupMenuItem::new(tr("Seçili programı kütüphaneye ekle")).on_click(move |_, _, cx| {
                view.update(cx, |this, cx| this.save_to_library(cx));
            }),
        );
    }
    let packages = opendvp::program_pack::library_dir()
        .map(|dir| opendvp::program_pack::library_list_in(&dir))
        .unwrap_or_default();
    if packages.is_empty() {
        return menu.label(tr("Kütüphane boş"));
    }
    menu = menu.label(tr("Projeye ekle"));
    for (name, path) in packages {
        let view = view.clone();
        menu = menu.item(PopupMenuItem::new(name).on_click(move |_, _, cx| {
            let text = opendvp::program_pack::read_file(&path);
            view.update(cx, |this, cx| this.import_package_text(text, cx));
        }));
    }
    menu
}
