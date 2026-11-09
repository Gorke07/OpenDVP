//! Parameter sets join the existing project/workbench, with a local-memory preview.
use super::*;
use opendvp::{
    plc_monitor,
    recipes::{Parameter, Recipe},
    st::DataType,
};

const TYPES: [DataType; 6] = [
    DataType::Bool,
    DataType::Int,
    DataType::Dint,
    DataType::Word,
    DataType::Dword,
    DataType::Real,
];

pub(super) struct RecipeUi {
    selected: usize,
    row: Option<usize>,
    shown: Option<Recipe>,
    name: Entity<InputState>,
    label: Entity<InputState>,
    target: Entity<InputState>,
    value: Entity<InputState>,
    unit: Entity<InputState>,
    min: Entity<InputState>,
    max: Entity<InputState>,
    ty: DataType,
    feedback: Option<(bool, String)>,
    undo: Option<(Vec<Recipe>, Vec<Recipe>)>,
}

impl RecipeUi {
    pub(super) fn reset(&mut self) {
        self.selected = 0;
        self.row = None;
        self.shown = None;
        self.feedback = None;
        self.undo = None;
    }

    pub(super) fn new(window: &mut Window, cx: &mut Context<Workbench>) -> Self {
        let mut input = |value: &'static str| {
            let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
            cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify())
                .detach();
            input
        };
        Self {
            selected: 0,
            row: None,
            shown: None,
            name: input(""),
            label: input(""),
            target: input("D0"),
            value: input("0"),
            unit: input(""),
            min: input(""),
            max: input(""),
            ty: DataType::Int,
            feedback: None,
            undo: None,
        }
    }
}

impl Workbench {
    fn recipe_index(&self) -> Option<usize> {
        (!self.project.recipes.is_empty()).then(|| {
            self.extra
                .recipes
                .selected
                .min(self.project.recipes.len() - 1)
        })
    }

    pub(super) fn sync_recipes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.recipe_index().map(|i| &self.project.recipes[i]);
        if selected == self.extra.recipes.shown.as_ref() {
            return;
        }
        let recipe = selected.cloned();
        let name = recipe.as_ref().map_or("", |r| r.name.as_str()).to_owned();
        self.extra.recipes.shown = recipe;
        self.extra.recipes.feedback = None;
        self.extra
            .recipes
            .name
            .update(cx, |s, cx| s.set_value(name, window, cx));
        self.recipe_select_row(None, window, cx);
    }

    fn recipe_select_row(
        &mut self,
        row: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let p = self
            .recipe_index()
            .and_then(|i| row.and_then(|row| self.project.recipes[i].parameters.get(row)))
            .cloned();
        let ui = &mut self.extra.recipes;
        ui.row = row;
        ui.ty = p.as_ref().map_or(DataType::Int, |p| p.ty);
        for (input, value) in [
            (&ui.label, p.as_ref().map_or("", |p| p.name.as_str())),
            (&ui.target, p.as_ref().map_or("D0", |p| p.target.as_str())),
            (&ui.value, p.as_ref().map_or("0", |p| p.value.as_str())),
            (&ui.unit, p.as_ref().map_or("", |p| p.unit.as_str())),
            (
                &ui.min,
                p.as_ref().and_then(|p| p.min.as_deref()).unwrap_or(""),
            ),
            (
                &ui.max,
                p.as_ref().and_then(|p| p.max.as_deref()).unwrap_or(""),
            ),
        ] {
            input.update(cx, |s, cx| s.set_value(value.to_owned(), window, cx));
        }
        cx.notify();
    }

    fn recipe_message(&mut self, good: bool, message: String, cx: &mut Context<Self>) {
        self.extra.recipes.feedback = Some((good, message));
        cx.notify();
    }

    fn recipe_commit(&mut self, recipes: Vec<Recipe>, selected: usize, cx: &mut Context<Self>) {
        if self.editing_locked() {
            return;
        }
        if let Err(e) = opendvp::recipes::validate(&recipes) {
            return self.recipe_message(false, e, cx);
        }
        let previous = std::mem::replace(&mut self.project.recipes, recipes);
        self.extra.recipes.undo = Some((previous, self.project.recipes.clone()));
        self.extra.recipes.selected = selected;
        self.dirty = true;
        cx.notify();
    }

    fn recipe_unique_name(&self, base: &str) -> String {
        if !self
            .project
            .recipes
            .iter()
            .any(|r| r.name.to_lowercase() == base.to_lowercase())
        {
            return base.into();
        }
        let base: String = base.chars().take(54).collect();
        (2..)
            .map(|n| format!("{base} ({n})"))
            .find(|name| {
                !self
                    .project
                    .recipes
                    .iter()
                    .any(|r| r.name.to_lowercase() == name.to_lowercase())
            })
            .unwrap()
    }

    fn recipe_create(&mut self, duplicate: bool, cx: &mut Context<Self>) {
        let mut recipe = if duplicate {
            let Some(i) = self.recipe_index() else {
                return;
            };
            self.project.recipes[i].clone()
        } else {
            Recipe {
                name: tr("Yeni reçete").into(),
                parameters: Vec::new(),
            }
        };
        recipe.name = self.recipe_unique_name(&recipe.name);
        let mut next = self.project.recipes.clone();
        let index = next.len();
        next.push(recipe);
        self.recipe_commit(next, index, cx);
    }

    fn recipe_save_parameter(&mut self, cx: &mut Context<Self>) {
        let Some(i) = self.recipe_index() else {
            return;
        };
        let ui = &self.extra.recipes;
        let value = |input: &Entity<InputState>| input.read(cx).value().trim().to_owned();
        let optional = |input: &Entity<InputState>| {
            let v = value(input);
            (!v.is_empty()).then_some(v)
        };
        let p = Parameter {
            name: value(&ui.label),
            target: value(&ui.target),
            ty: ui.ty,
            value: value(&ui.value),
            unit: value(&ui.unit),
            min: optional(&ui.min),
            max: optional(&ui.max),
        };
        let mut next = self.project.recipes.clone();
        if let Some(row) = ui.row.filter(|r| *r < next[i].parameters.len()) {
            next[i].parameters[row] = p;
        } else {
            next[i].parameters.push(p);
        }
        if let Err(e) = next[i].prepare(&self.project) {
            return self.recipe_message(false, e, cx);
        }
        self.recipe_commit(next, i, cx);
    }

    fn recipe_apply(&mut self, capture: bool, cx: &mut Context<Self>) {
        if self.editing_locked() || self.bridge.is_some() {
            return;
        }
        let Some(i) = self.recipe_index() else {
            return;
        };
        let recipe = &self.project.recipes[i];
        if capture {
            match recipe.capture(&self.project, &self.simulator.memory) {
                Ok(recipe) => {
                    let mut next = self.project.recipes.clone();
                    next[i] = recipe;
                    self.recipe_commit(next, i, cx);
                }
                Err(e) => self.recipe_message(false, e, cx),
            }
        } else {
            let prepared = match recipe.prepare(&self.project) {
                Ok(p) => p,
                Err(e) => return self.recipe_message(false, e, cx),
            };
            if prepared.iter().any(|p| {
                self.forced.keys().any(|d| {
                    d.area == p.device.area
                        && (p.device.index..p.device.index + p.words.len() as u16)
                            .contains(&d.index)
                })
            }) {
                return self.recipe_message(
                    false,
                    tr("Reçete hedeflerinde zorlama var; önce zorlamayı kaldırın.").into(),
                    cx,
                );
            }
            match recipe.apply(&self.project, &mut self.simulator.memory) {
                Ok(n) => self.recipe_message(
                    true,
                    tfmt!(
                        "{n} parametre yerel simülatöre uygulandı.",
                        "Applied {n} parameters to the local simulator."
                    ),
                    cx,
                ),
                Err(e) => self.recipe_message(false, e, cx),
            }
        }
    }

    pub(super) fn import_recipe_result(&mut self, recipe: Recipe, cx: &mut Context<Self>) {
        let mut recipe = recipe;
        recipe.name = self.recipe_unique_name(&recipe.name);
        let mut next = self.project.recipes.clone();
        let i = next.len();
        next.push(recipe);
        self.recipe_commit(next, i, cx);
    }

    fn recipe_file(&mut self, import: bool, cx: &mut Context<Self>) {
        if self.editing_locked() {
            return;
        }
        if import {
            self.dispatch(
                async move {
                    Done::RecipeImported(
                        panels::dialog(|| {
                            let path = panels::start_dir(rfd::FileDialog::new())
                                .add_filter(tr("Reçete"), &["odrecipe", "json"])
                                .pick_file()?;
                            Some((|| {
                                if std::fs::metadata(&path).map_err(|e| e.to_string())?.len()
                                    > 256 * 1024
                                {
                                    return Err(
                                        tr("Reçete dosyası 256 KiB sınırını aşıyor.").into()
                                    );
                                }
                                Recipe::import(
                                    &std::fs::read_to_string(path).map_err(|e| e.to_string())?,
                                )
                            })())
                        })
                        .await,
                    )
                },
                cx,
            );
        } else {
            let Some(i) = self.recipe_index() else {
                return;
            };
            let recipe = &self.project.recipes[i];
            let json = match recipe.export() {
                Ok(json) => json,
                Err(e) => return self.recipe_message(false, e, cx),
            };
            let name: String = recipe
                .name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            self.dispatch(
                async move {
                    Done::RecipeExported(
                        panels::dialog(move || {
                            let path = panels::start_dir(rfd::FileDialog::new())
                                .add_filter(tr("Reçete"), &["odrecipe"])
                                .set_file_name(format!("{name}.odrecipe"))
                                .save_file()?;
                            Some(
                                std::fs::write(&path, json)
                                    .map(|_| path)
                                    .map_err(|e| e.to_string()),
                            )
                        })
                        .await,
                    )
                },
                cx,
            );
        }
    }

    pub(super) fn recipes_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let locked = self.editing_locked();
        let local = self.bridge.is_none() && self.extra.hardware.is_none();
        let selected = self.recipe_index();
        let ui = &self.extra.recipes;
        let heading = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .p_3()
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr("Parametreler / reçeteler")),
            )
            .child(
                self.button("recipe-new", tr("Yeni reçete"), locked)
                    .on_click(cx.listener(|this, _, _, cx| this.recipe_create(false, cx))),
            )
            .child(
                self.button("recipe-import", tr("İçe aktar…"), locked)
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.recipe_file(true, cx))),
            )
            .child(
                self.button(
                    "recipe-undo",
                    tr("Geri al"),
                    locked
                        || !ui
                            .undo
                            .as_ref()
                            .is_some_and(|(_, after)| *after == self.project.recipes),
                )
                .ghost()
                .tooltip(tr(
                    "Son reçete düzenlemesini geri alır; simülatör değerlerini değiştirmez.",
                ))
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.editing_locked() {
                        return;
                    }
                    if let Some((before, after)) = this.extra.recipes.undo.take()
                        && after == this.project.recipes
                    {
                        this.project.recipes = before;
                        this.dirty = true;
                        cx.notify();
                    }
                })),
            );
        let mut list = div()
            .id("recipe-list")
            .w(px(180.))
            .flex_shrink_0()
            .overflow_y_scroll()
            .p_2();
        for (i, recipe) in self.project.recipes.iter().enumerate() {
            list = list.child(
                Button::new(("recipe-select", i))
                    .label(format!("{} · {}", recipe.name, recipe.parameters.len()))
                    .w_full()
                    .small()
                    .ghost()
                    .when(selected == Some(i), |b| {
                        b.bg(rgb(0xe3f2f4)).text_color(rgb(ACCENT))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.extra.recipes.selected = i;
                        cx.notify();
                    })),
            );
        }
        let Some(i) = selected else {
            return div().size_full().flex().flex_col().bg(rgb(PANEL)).child(heading)
                .child(div().p_6().flex().flex_col().gap_3()
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(tr("İlk parametre setinizi oluşturun")))
                    .child(tr("Hız, zaman ve proses ayarlarını bir reçetede saklayın. Yeni reçete ile başlayın veya .odrecipe dosyası içe aktarın.")))
                .into_any_element();
        };
        let recipe = &self.project.recipes[i];
        let validation = recipe.prepare(&self.project).err();
        let mut table = div()
            .id("recipe-parameters")
            .min_h(px(110.))
            .flex_1()
            .overflow_y_scroll()
            .overflow_x_scroll()
            .flex()
            .flex_col();
        let cells = |name: String,
                     target: String,
                     ty: String,
                     saved: String,
                     current: String,
                     changed: bool| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .py_2()
                .px_3()
                .child(div().w(px(130.)).truncate().child(name))
                .child(
                    div()
                        .w(px(90.))
                        .truncate()
                        .font_family("Consolas")
                        .child(target),
                )
                .child(div().w(px(50.)).child(ty))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(70.))
                        .truncate()
                        .font_family("Consolas")
                        .text_color(rgb(if changed { ACCENT } else { TEXT }))
                        .child(saved),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(70.))
                        .truncate()
                        .font_family("Consolas")
                        .child(current),
                )
        };
        table = table.child(
            cells(
                tr("Parametre").into(),
                tr("Hedef").into(),
                tr("Tip").into(),
                tr("Reçete değeri").into(),
                tr("Yerel değer").into(),
                false,
            )
            .min_w(px(680.))
            .font_weight(FontWeight::SEMIBOLD)
            .bg(rgb(0xf5f7fa))
            .text_xs(),
        );
        for (row, p) in recipe.parameters.iter().enumerate() {
            let current = if local {
                p.resolve(&self.project).ok().map(|r| {
                    plc_monitor::read_memory(&self.simulator.memory, &[(r.device, p.ty)])[0]
                        .to_string()
                })
            } else {
                None
            };
            let desired = opendvp::project::parse_initial(p.ty, &p.value)
                .ok()
                .map(|w| plc_monitor::decode(p.ty, &w).to_string());
            let changed = current.is_some() && current != desired;
            let saved = if p.unit.is_empty() {
                p.value.clone()
            } else {
                format!("{} {}", p.value, p.unit)
            };
            table = table.child(
                div()
                    .id(("recipe-row", row))
                    .flex()
                    .min_w(px(680.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(if ui.row == Some(row) { 0xe3f2f4 } else { PANEL }))
                    .child(
                        div()
                            .id(("recipe-row-select", row))
                            .flex_1()
                            .min_w_0()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0xf1f4f7)))
                            .child(cells(
                                p.name.clone(),
                                p.target.clone(),
                                p.ty.name().into(),
                                saved,
                                current.unwrap_or_else(|| "—".into()),
                                changed,
                            ))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.recipe_select_row(Some(row), window, cx)
                            })),
                    )
                    .child(
                        Button::new(("recipe-row-edit", row))
                            .label(tr("Düzenle"))
                            .ghost()
                            .xsmall()
                            .disabled(locked)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.recipe_select_row(Some(row), window, cx)
                            })),
                    )
                    .child(
                        Button::new(("recipe-row-remove", row))
                            .icon(gpui_component::IconName::Close)
                            .ghost()
                            .small()
                            .disabled(locked)
                            .tooltip(tr("Parametreyi kaldır"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let mut next = this.project.recipes.clone();
                                next[i].parameters.remove(row);
                                this.recipe_commit(next, i, cx);
                            })),
                    ),
            );
        }
        if recipe.parameters.is_empty() {
            table = table.child(
                div()
                    .p_4()
                    .text_color(rgb(MUTED))
                    .child(tr("Aşağıdaki formdan ilk parametreyi ekleyin.")),
            );
        }
        let field = |label: &'static str, input: &Entity<InputState>, width| {
            div()
                .w(px(width))
                .flex()
                .flex_col()
                .gap_1()
                .child(Self::heading(label))
                .child(Input::new(input).aria_label(label).small().disabled(locked))
        };
        let mut types = div().flex().flex_wrap().gap_1();
        for ty in TYPES {
            types = types.child(
                Button::new(SharedString::from(format!("recipe-type-{}", ty.name())))
                    .label(ty.name())
                    .small()
                    .ghost()
                    .disabled(locked)
                    .when(ui.ty == ty, |b| b.primary())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.extra.recipes.ty = ty;
                        cx.notify();
                    })),
            );
        }
        let form = div().flex_shrink_0().p_3().border_t_1().border_color(rgb(LINE)).flex().flex_col().gap_2()
            .child(div().flex().flex_wrap().gap_3().child(field(tr("Parametre"), &ui.label, 155.)).child(field(tr("Adres / sembol"), &ui.target, 130.))
                .child(field(tr("Değer"), &ui.value, 110.)).child(field(tr("Birim"), &ui.unit, 75.)).child(field(tr("Alt sınır"), &ui.min, 90.)).child(field(tr("Üst sınır"), &ui.max, 90.)))
            .child(div().flex().flex_wrap().items_center().gap_3().child(types)
                .child(self.button("recipe-parameter-save", if ui.row.is_some() { tr("Parametreyi güncelle") } else { tr("Parametre ekle") }, locked).primary()
                    .on_click(cx.listener(|this, _, _, cx| this.recipe_save_parameter(cx))))
                .child(self.button("recipe-parameter-clear", tr("Yeni parametre"), locked).ghost().on_click(cx.listener(|this, _, window, cx| this.recipe_select_row(None, window, cx)))))
            .child(div().text_xs().text_color(rgb(MUTED)).child(tr("BOOL için M, diğer tipler için D adresi veya aynı tipte proje sembolü kullanın. Sınırlar isteğe bağlıdır.")));
        let detail = div().id("recipe-detail-scroll").flex_1().min_w_0().min_h_0().overflow_y_scroll().flex().flex_col().border_l_1().border_color(rgb(LINE))
            .child(div().p_3().flex().flex_wrap().items_center().gap_2()
                .child(div().w(px(210.)).child(Input::new(&ui.name).small().disabled(locked)))
                .child(self.button("recipe-rename", tr("Adı uygula"), locked).ghost().on_click(cx.listener(move |this, _, _, cx| {
                    let mut next = this.project.recipes.clone(); next[i].name = this.extra.recipes.name.read(cx).value().trim().to_owned(); this.recipe_commit(next, i, cx);
                })))
                .child(self.button("recipe-copy", tr("Çoğalt"), locked).ghost().on_click(cx.listener(|this, _, _, cx| this.recipe_create(true, cx))))
                .child(self.button("recipe-export", tr("Dışa aktar…"), locked).ghost().on_click(cx.listener(|this, _, _, cx| this.recipe_file(false, cx))))
                .child(self.button("recipe-delete", tr("Reçeteyi sil"), locked).ghost().on_click(cx.listener(move |this, _, _, cx| { let mut next = this.project.recipes.clone(); next.remove(i); this.recipe_commit(next, i.saturating_sub(1), cx); }))))
            .child(div().px_3().pb_2().flex().flex_wrap().items_center().gap_2()
                .child(self.button("recipe-apply", tr("Yerel simülatöre uygula"), locked || !local || validation.is_some() || recipe.parameters.is_empty()).primary().on_click(cx.listener(|this, _, _, cx| this.recipe_apply(false, cx))))
                .child(self.button("recipe-capture", tr("Yerel değerleri al"), locked || !local || validation.is_some() || recipe.parameters.is_empty()).ghost().on_click(cx.listener(|this, _, _, cx| this.recipe_apply(true, cx))))
                .child(div().text_xs().text_color(rgb(MUTED)).child(tr("Uygulamak için yerel simülasyonu durdurun; değerler tek işlemde değiştirilir."))))
            .children(validation.map(|e| div().px_3().py_2().text_color(rgb(ERROR)).child(e)))
            .children(ui.feedback.as_ref().map(|(good, message)| div().px_3().py_2().text_color(rgb(if *good { GREEN } else { ERROR })).child(message.clone())))
            .child(table).child(form);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .child(heading)
            .child(div().flex_1().min_h_0().flex().child(list).child(detail))
            .into_any_element()
    }
}
