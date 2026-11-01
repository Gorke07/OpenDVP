use super::*;
use gpui_component::{
    Icon, IconName,
    alert::Alert,
    description_list::DescriptionList,
    group_box::{GroupBox, GroupBoxVariants},
    list::ListItem,
    radio::{Radio, RadioGroup},
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
    tab::{Tab as KitTab, TabBar},
    table::{Column, DataTable, TableDelegate, TableState},
};

struct DataRows {
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
}
impl TableDelegate for DataRows {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }
    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }
    fn column(&self, i: usize, _: &App) -> Column {
        self.columns[i].clone()
    }
    fn render_td(
        &mut self,
        r: usize,
        c: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .font_family("Consolas")
            .text_sm()
            .child(self.rows[r][c].clone())
    }
    fn render_empty(
        &mut self,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .p_4()
            .text_color(rgb(MUTED))
            .child(tr("Görüntülenecek veri yok."))
    }
}
type Choice = Entity<SelectState<Vec<SharedString>>>;
pub(super) struct ConfigUi {
    choices: Vec<(Field, Choice, Vec<SharedString>, Option<SharedString>)>,
    module_table: Entity<TableState<DataRows>>,
    memory_table: Entity<TableState<DataRows>>,
    module_rows: Vec<Vec<String>>,
    memory_rows: Vec<Vec<String>>,
    module_tab: usize,
    memory_tab: usize,
}
impl ConfigUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Workbench>) -> Self {
        let mut choices = vec![];
        for (field, values) in [
            (
                Field::Baud,
                vec![
                    "1200", "2400", "4800", "9600", "19200", "38400", "57600", "115200",
                ],
            ),
            (Field::DataBits, vec!["7", "8"]),
            (Field::Parity, vec!["None", "Even", "Odd"]),
            (Field::StopBits, vec!["1", "2"]),
            (Field::Protocol, vec!["Ascii", "Rtu", "Tcp"]),
        ] {
            let values = values
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>();
            let state = cx.new(|cx| SelectState::new(values.clone(), None, window, cx));
            cx.subscribe_in(
                &state,
                window,
                move |this, _, event: &SelectEvent<Vec<SharedString>>, window, cx| {
                    if !this.topology_editable() {
                        return;
                    }
                    if let SelectEvent::Confirm(Some(value)) = event {
                        if let Some((_, _, input)) =
                            this.extra.fields.iter().find(|(f, _, _)| *f == field)
                        {
                            input.update(cx, |s, cx| s.set_value(value.clone(), window, cx));
                        }
                        this.sync_profile(cx);
                        cx.notify();
                    }
                },
            )
            .detach();
            choices.push((field, state, values, None));
        }
        fn table(
            labels: &[(&'static str, &'static str, f32)],
            window: &mut Window,
            cx: &mut Context<Workbench>,
        ) -> Entity<TableState<DataRows>> {
            let columns = labels
                .iter()
                .map(|(id, label, width)| Column::new(*id, *label).width(px(*width)).movable(false))
                .collect();
            cx.new(|cx| {
                TableState::new(
                    DataRows {
                        columns,
                        rows: vec![],
                    },
                    window,
                    cx,
                )
                .col_selectable(false)
            })
        }
        Self {
            choices,
            module_table: table(
                &[
                    ("cr", "Register", 130.),
                    ("signed", tr("İşaretli"), 160.),
                    ("hex", "HEX", 160.),
                    ("state", tr("Durum"), 260.),
                ],
                window,
                cx,
            ),
            memory_table: table(
                &[
                    ("index", tr("Dosya adresi"), 160.),
                    ("signed", tr("İşaretli"), 160.),
                    ("unsigned", tr("İşaretsiz"), 160.),
                    ("hex", "HEX", 160.),
                ],
                window,
                cx,
            ),
            module_rows: vec![],
            memory_rows: vec![],
            module_tab: 0,
            memory_tab: 0,
        }
    }
}
fn data_table(state: &Entity<TableState<DataRows>>, height: f32) -> AnyElement {
    div()
        .relative()
        .w_full()
        .h(px(height))
        .flex_shrink_0()
        .border_1()
        .border_color(rgb(LINE))
        .rounded(px(3.))
        .child(
            div()
                .absolute()
                .inset_0()
                .child(DataTable::new(state).small().stripe(true).bordered(false)),
        )
        .into_any_element()
}
fn section(title: &str) -> Div {
    div().flex().flex_col().gap_3().child(
        div()
            .font_weight(FontWeight::SEMIBOLD)
            .child(title.to_owned()),
    )
}
fn page(title: &str, description: &str) -> Stateful<Div> {
    div()
        .id(SharedString::from(title.to_owned()))
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap_5()
        .p_5()
        .bg(rgb(PANEL))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_owned()),
                )
                .child(Workbench::heading(description.to_owned())),
        )
}
impl Workbench {
    pub(crate) fn sync_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.tab, Tab::Connection | Tab::Modules | Tab::FileMemory) {
            return;
        }
        for (field, select, options, synced) in &mut self.extra.config.choices {
            if let Some((_, _, input)) = self.extra.fields.iter().find(|(f, _, _)| f == field) {
                let value = input.read(cx).value();
                if synced.as_ref() != Some(&value) {
                    let mut items = options.clone();
                    if !items.contains(&value) {
                        items.push(value.clone());
                    }
                    select.update(cx, |s, cx| {
                        s.set_items(items, window, cx);
                        s.set_selected_value(&value, window, cx);
                    });
                    *synced = Some(value);
                }
            }
        }
        let rows: Vec<Vec<String>> = self
            .simulator
            .memory
            .module_io
            .read_slot(self.extra.slot)
            .map(|m| {
                m.registers()
                    .into_iter()
                    .map(|(cr, v)| match v {
                        Ok(v) => vec![
                            format!("CR {cr:03}"),
                            (v as i16).to_string(),
                            format!("{v:04X}"),
                            tr("Okundu").into(),
                        ],
                        Err(e) => vec![format!("CR {cr:03}"), "—".into(), "—".into(), e],
                    })
                    .collect()
            })
            .unwrap_or_default();
        if rows != self.extra.config.module_rows {
            let count_changed = rows.len() != self.extra.config.module_rows.len();
            self.extra.config.module_rows = rows;
            let rows = self.extra.config.module_rows.clone();
            self.extra.config.module_table.update(cx, |t, cx| {
                t.delegate_mut().rows = rows;
                if count_changed {
                    t.refresh(cx);
                }
                cx.notify();
            });
        }
        let rows: Vec<Vec<String>> = self
            .simulator
            .memory
            .file_memory
            .store()
            .and_then(|s| {
                self.extra
                    .memory_index
                    .read(cx)
                    .value()
                    .parse::<usize>()
                    .ok()
                    .filter(|i| *i < s.words().len())
                    .map(|start| {
                        s.words()
                            .iter()
                            .enumerate()
                            .skip(start)
                            .take(32)
                            .map(|(i, w)| {
                                vec![
                                    format!("F[{i:04}]"),
                                    (*w as i16).to_string(),
                                    w.to_string(),
                                    format!("{w:04X}"),
                                ]
                            })
                            .collect()
                    })
            })
            .unwrap_or_default();
        if rows != self.extra.config.memory_rows {
            let count_changed = rows.len() != self.extra.config.memory_rows.len();
            self.extra.config.memory_rows = rows;
            let rows = self.extra.config.memory_rows.clone();
            self.extra.config.memory_table.update(cx, |t, cx| {
                t.delegate_mut().rows = rows;
                if count_changed {
                    t.refresh(cx);
                }
                cx.notify();
            });
        }
    }
    fn config_field(&self, field: Field, locked: bool) -> AnyElement {
        let (_, label, input) = self
            .extra
            .fields
            .iter()
            .find(|(f, _, _)| *f == field)
            .unwrap();
        if let Some((_, state, _, _)) = self
            .extra
            .config
            .choices
            .iter()
            .find(|(f, _, _, _)| *f == field)
        {
            div()
                .flex_1()
                .min_w(px(165.))
                .flex()
                .flex_col()
                .gap_1()
                .child(Self::heading(label.to_string()))
                .child(Select::new(state).w_full().disabled(locked))
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .child(Self::form(label, input, locked))
                .into_any_element()
        }
    }
    pub(crate) fn physical_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let locked = !self.topology_editable();
        let kind = self.extra.profiles.active_kind();
        let mut list = div()
            .id("profiles-scroll")
            .overflow_y_scroll()
            .min_h_0()
            .w(px(220.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .bg(rgb(0xf5f7fa))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(Self::heading(tr("BAĞLANTI PROFİLLERİ")));
        for (i, p) in self.extra.profiles.profiles.iter().enumerate() {
            list = list.child(
                ListItem::new(("connection-profile", i))
                    .selected(self.extra.profiles.selected == i)
                    .disabled(locked)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .py_1()
                            .child(Icon::new(IconName::Settings2).size(px(15.)))
                            .child(div().flex().flex_col().child(p.name.clone()).child(
                                div().text_xs().text_color(rgb(MUTED)).child(p.kind.title()),
                            )),
                    )
                    .on_click(cx.listener(move |this, _, w, cx| {
                        this.select_profile(Event::Select(i), w, cx)
                    })),
            );
        }
        list = list
            .child(div().flex_1())
            .child(
                self.button("profile-add", tr("Profili çoğalt"), locked)
                    .icon(IconName::Plus)
                    .on_click(cx.listener(|t, _, w, cx| t.select_profile(Event::Add, w, cx))),
            )
            .child(
                self.button("profile-remove", tr("Profili kaldır"), locked)
                    .on_click(cx.listener(|t, _, w, cx| t.select_profile(Event::Remove, w, cx))),
            )
            .child(
                self.button("profile-undo", tr("Kaldırmayı geri al"), locked)
                    .icon(IconName::Undo2)
                    .on_click(
                        cx.listener(|t, _, w, cx| t.select_profile(Event::UndoRemove, w, cx)),
                    ),
            );
        let kinds = [
            Kind::UsbSerial,
            Kind::Serial,
            Kind::Ethernet,
            Kind::Simulator,
        ];
        let tabs = TabBar::new("connection-kind")
            .segmented()
            .small()
            .selected_index(kinds.iter().position(|k| *k == kind).unwrap_or(0))
            .children(
                [tr("USB / seri"), tr("Seri / COM"), "Ethernet", tr("Simülatör")]
                    .map(|l| KitTab::new().label(l).disabled(locked)),
            )
            .on_click(cx.listener(move |t, index, w, cx| {
                t.select_profile(Event::Kind(kinds[*index]), w, cx)
            }));
        let mut detail = page(
            tr("Bağlantı ayarları"),
            tr("Profil, aktarım ve okuma ayarlarını yapılandırın."),
        )
        .child(tabs)
        .child(self.config_field(Field::Name, locked));
        if locked {
            detail = detail.child(
                Alert::info(
                    "profile-lock",
                    tr("Ayarları değiştirmek için çalışan oturumu durdurup bağlantıyı kesin."),
                )
                .small(),
            );
        }
        if kind == Kind::Simulator {
            detail=detail.child(GroupBox::new().title(tr("COMMGR simülatörü")).outline().child(Self::form(tr("Çalıştırılabilir dosya"),&self.simulator_path,locked)))
                .child(Alert::info("sim-scope",tr("Program aktarımı ve sürekli online izleme, uygulamanın başlattığı SS2 oturumunda çalışır.")).small())
                .child(div().flex().gap_2().child(self.button("sim-connect",tr("Simülatöre bağlan"),locked).primary().on_click(cx.listener(|t,_,_,cx|t.connect(cx))))
                .child(self.button("sim-disconnect",tr("Bağlantıyı kes"),self.bridge.is_none()||self.running||self.busy).on_click(cx.listener(|t,_,_,cx|t.disconnect(cx)))));
        } else {
            let mut transport = section(if kind == Kind::Ethernet {
                tr("Ağ adresi")
            } else {
                tr("Seri port")
            });
            let fields = if kind == Kind::Ethernet {
                vec![Field::Host, Field::TcpPort]
            } else {
                vec![
                    Field::Port,
                    Field::Baud,
                    Field::DataBits,
                    Field::Parity,
                    Field::StopBits,
                    Field::Protocol,
                ]
            };
            for chunk in fields.chunks(2) {
                transport = transport.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_4()
                        .children(chunk.iter().map(|f| self.config_field(*f, locked))),
                );
            }
            if matches!(kind, Kind::UsbSerial | Kind::Serial) {
                transport = transport.child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(
                            self.button("refresh-ports", tr("Portları tara"), self.busy)
                                .icon(IconName::Search)
                                .on_click(cx.listener(|t, _, _, cx| t.refresh_ports(cx))),
                        )
                        .child(Self::heading(tfmt!(
                            "{} port bulundu", "{} ports found",
                            self.extra.profiles.ports.len()
                        ))),
                );
                for port in &self.extra.profiles.ports {
                    let name = port.name.clone();
                    transport = transport.child(
                        Button::new(SharedString::from(format!("port-{name}")))
                            .label(port.label.clone())
                            .small()
                            .disabled(locked)
                            .on_click(cx.listener(move |t, _, w, cx| {
                                t.select_profile(Event::UsePort(name.clone()), w, cx)
                            })),
                    );
                }
            }
            detail=detail.child(GroupBox::new().title(tr("Bağlantı parametreleri")).outline().child(transport))
                .child(GroupBox::new().title(tr("Okuma oturumu")).outline().child(div().flex().flex_wrap().gap_4().child(self.config_field(Field::Station,locked)).child(self.config_field(Field::Timeout,locked)).child(self.config_field(Field::Poll,locked))))
                .child(GroupBox::new().title(tr("Cihaz aralığı")).outline().child(div().flex().flex_wrap().gap_4().child(self.config_field(Field::XStart,locked)).child(self.config_field(Field::XCount,locked))).child(div().flex().flex_wrap().gap_4().child(self.config_field(Field::YStart,locked)).child(self.config_field(Field::YCount,locked))))
                .child(Alert::info("physical-scope",tr("İzleme X/Y için yalnız FC01/FC02 okur. Program yükleme ayrı bir bağlantıda PLC’yi STOP eder, programı yazar ve geri okuyarak doğrular; kaynak/yorum, register ve PLC kimliği yazılmaz.")).small())
                .child(div().flex().gap_2().child(self.button("hardware-connect",tr("PLC’ye bağlan"),locked).primary().on_click(cx.listener(|t,_,_,cx|t.connect_physical(cx))))
                    .child(self.button("hardware-disconnect",tr("Bağlantıyı kes"),self.extra.hardware.is_none()||self.busy).on_click(cx.listener(|t,_,_,cx|{t.disconnect_hardware();t.log(tr("Fiziksel bağlantı kapatıldı."));cx.notify();})))
                    .child(self.button("hardware-probe",tr("Bağlantıyı test et"),locked).tooltip(tr("Yalnız okuma: model, kapasite, RUN, parola ve PLC kimliği durumu")).on_click(cx.listener(|t,_,_,cx|t.probe_physical(cx)))))
                .child(self.probe_card());
            detail = detail.child(
                GroupBox::new().title(tr("PLC kontrolü")).outline().child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            self.button("plc-status", tr("Durumu oku"), locked)
                                .tooltip(tr("Yalnız okuma: RUN/STOP, hata kodu, tarama süresi, PLC saati"))
                                .on_click(cx.listener(|t, _, _, cx| t.status_physical(cx))),
                        )
                        .children(if self.extra.control_armed {
                            vec![
                                self.button("plc-run", tr("RUN yap"), locked)
                                    .warning()
                                    .on_click(cx.listener(|t, _, _, cx| t.run_stop_physical(true, cx))),
                                self.button("plc-stop", tr("STOP yap"), locked)
                                    .danger()
                                    .on_click(cx.listener(|t, _, _, cx| t.run_stop_physical(false, cx))),
                                self.button("plc-control-cancel", tr("Vazgeç"), false).on_click(
                                    cx.listener(|t, _, _, cx| {
                                        t.extra.control_armed = false;
                                        cx.notify();
                                    }),
                                ),
                            ]
                        } else {
                            vec![self.button("plc-control-arm", "RUN/STOP…", locked || opendvp::settings::get().plc_read_only).on_click(
                                cx.listener(|t, _, _, cx| {
                                    t.extra.control_armed = true;
                                    t.log(tr("Onay bekleniyor: RUN/STOP makineyi etkiler."));
                                    cx.notify();
                                }),
                            )]
                        }),
                ),
            );
            let download_blocked = locked || self.program.is_none();
            detail = detail.child(
                GroupBox::new().title(tr("Program yükleme")).outline().child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_4()
                        .child(Self::form(
                            tr("PLC parolası (yalnız kilitliyse, 4 karakter)"),
                            &self.extra.download_password,
                            locked,
                        ))
                        .child(Self::form(
                            tr("PLC kimliği (yalnız tanımlıysa)"),
                            &self.extra.download_plc_id,
                            locked,
                        ))
                        .child(
                            self.button(
                                "plc-write-initial",
                                if self.extra.write_initial {
                                    tr("Başlangıç değerlerini yaz: Açık")
                                } else {
                                    tr("Başlangıç değerlerini yaz: Kapalı")
                                },
                                locked,
                            )
                            .ghost()
                            .tooltip(tr("Sembollerin başlangıç değerleri program yazıldıktan sonra, PLC STOP'tayken yazılır"))
                            .on_click(cx.listener(|t, _, _, cx| {
                                t.extra.write_initial = !t.extra.write_initial;
                                cx.notify();
                            })),
                        ),
                ),
            );
            detail = detail.child(if self.extra.download_armed {
                div()
                    .flex()
                    .gap_2()
                    .child(
                        self.button("plc-download-stop", tr("Yaz, STOP’ta bırak"), download_blocked)
                            .warning()
                            .on_click(cx.listener(|t, _, _, cx| t.download_physical(false, cx))),
                    )
                    .child(
                        self.button("plc-download-run", tr("Yaz ve RUN"), download_blocked)
                            .danger()
                            .on_click(cx.listener(|t, _, _, cx| t.download_physical(true, cx))),
                    )
                    .child(
                        self.button("plc-download-cancel", tr("Vazgeç"), false)
                            .on_click(cx.listener(|t, _, _, cx| {
                                t.extra.download_armed = false;
                                cx.notify();
                            })),
                    )
            } else {
                div().flex().gap_2().child(
                    self.button(
                        "plc-download-arm",
                        tr("Programı PLC’ye yükle…"),
                        download_blocked || opendvp::settings::get().plc_read_only,
                    )
                    .on_click(cx.listener(|t, _, _, cx| {
                        t.extra.download_armed = true;
                        t.log(tr("Onay bekleniyor: PLC STOP edilecek ve program üzerine yazılacak."));
                        cx.notify();
                    })),
                )
            });
        }
        if let Some(feedback) = &self.extra.profiles.feedback {
            detail = detail.child(Alert::warning("profile-feedback", feedback.clone()).small());
        }
        detail = detail.child(
            div().border_t_1().border_color(rgb(LINE)).pt_4().child(
                self.button("profiles-save", tr("Profilleri kaydet"), locked)
                    .on_click(cx.listener(|t, _, _, cx| {
                        t.sync_profile(cx);
                        if let Some(path) = Panels::profile_path() {
                            let result = t.extra.profiles.save_to(&path);
                            t.log(
                                result
                                    .map(|_| tr("Bağlantı profilleri kaydedildi.").into())
                                    .unwrap_or_else(|e| e),
                            );
                        }
                        cx.notify();
                    })),
            ),
        );
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(list)
            .child(detail)
            .into_any_element()
    }
    pub(crate) fn modules_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let bus = &self.simulator.memory.module_io;
        let slot = self.extra.slot;
        let locked = !self.topology_editable();
        let samples_locked = !self.local_editable();
        let mut slots = div()
            .id("slots-scroll")
            .overflow_y_scroll()
            .min_h_0()
            .w(px(200.))
            .flex_shrink_0()
            .p_3()
            .flex()
            .flex_col()
            .gap_1()
            .bg(rgb(0xf5f7fa))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(Self::heading(tr("GENİŞLEME YOLU")));
        for i in 0..8u8 {
            let installed = bus.read_slot(i).is_some();
            slots = slots.child(
                ListItem::new(("module-slot", i as usize))
                    .selected(slot == i)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .py_2()
                            .child(
                                Icon::new(if installed {
                                    IconName::Settings2
                                } else {
                                    IconName::Plus
                                })
                                .size(px(15.)),
                            )
                            .child(div().flex().flex_col().child(tfmt!("Slot {i:02}", "Slot {i:02}")).child(
                                div().text_xs().text_color(rgb(MUTED)).child(if installed {
                                    "DVP04AD"
                                } else {
                                    tr("Boş slot")
                                }),
                            )),
                    )
                    .on_click(cx.listener(move |t, _, w, cx| {
                        t.extra.slot = i;
                        let values = t
                            .simulator
                            .memory
                            .module_io
                            .read_slot(i)
                            .and_then(VirtualModule::samples)
                            .unwrap_or([0; 4]);
                        for (input, value) in t.extra.samples.iter().zip(values) {
                            input.update(cx, |s, cx| s.set_value(value.to_string(), w, cx));
                        }
                        cx.notify();
                    })),
            );
        }
        let tabs = TabBar::new("module-tabs")
            .underline()
            .small()
            .selected_index(self.extra.config.module_tab)
            .children(
                [tr("Kanallar"), tr("Kontrol register’ları"), tr("Besleme")].map(|l| KitTab::new().label(l)),
            )
            .on_click(cx.listener(|t, i, _, cx| {
                t.extra.config.module_tab = *i;
                cx.notify();
            }));
        let mut body = page(
            &tfmt!("Slot {slot:02} · modül yapılandırması", "Slot {slot:02} · module configuration"),
            tr("Yerel sanal genişleme yolu · FROM / TO aynı modül belleğine erişir."),
        )
        .child(
            RadioGroup::horizontal("module-cpu")
                .flex_none()
                .h(px(28.))
                .flex_shrink_0()
                .selected_index(Some(usize::from(bus.cpu().model == ModuleCpu::Ss2)))
                .disabled(locked)
                .child(Radio::new("cpu-es2").label("ES2 · 3.46"))
                .child(Radio::new("cpu-ss2").label("SS2 · 2.80"))
                .on_click(
                    cx.listener(|t, i, _, cx| t.module_action(if *i == 0 { 0 } else { 1 }, cx)),
                ),
        )
        .child(tabs);
        if self.bridge.is_some() || self.extra.hardware.is_some() {
            body=body.child(Alert::info("module-local",tr("Bu ekran yerel sanal modül modelidir. Bağlı PLC’nin modülleri burada okunmaz veya değiştirilmez.")).small());
        }
        if self.extra.config.module_tab == 2 {
            body = body.child(
                DescriptionList::new()
                    .columns(2)
                    .small()
                    .item("Model", tr("24 V besleme · M1019"), 1)
                    .item(
                        "ERROR LED",
                        if self.bridge.is_some() || self.extra.hardware.is_some() {
                            tr("Kullanılamıyor").into()
                        } else {
                            format!("{:?}", self.simulator.memory.power_monitor.led())
                        },
                        1,
                    ),
            );
            let mut supply = div().flex().gap_2();
            for (id, label, input) in [
                ("supply-unknown", tr("Bilinmiyor"), SupplyInput::Unknown),
                ("supply-normal", "Normal", SupplyInput::Normal),
                ("supply-low", "<17,8 V", SupplyInput::Below17800Mv),
                ("supply-unstable", tr("Kararsız"), SupplyInput::Unstable),
            ] {
                supply = supply.child(self.button(id, label, samples_locked).on_click(
                    cx.listener(move |t, _, _, cx| {
                        PowerState::set_backend(
                            &mut t.simulator.memory,
                            PowerBackend::DocumentedVirtual,
                        );
                        PowerState::set_input(&mut t.simulator.memory, input);
                        cx.notify();
                    }),
                ));
            }
            body = body.child(
                GroupBox::new()
                    .title(tr("Besleme senaryosu"))
                    .outline()
                    .child(supply)
                    .child(
                        self.button("power-time", tr("Sanal zamanı +1 s ilerlet"), locked)
                            .on_click(cx.listener(|t, _, _, cx| {
                                opendvp::timing_io::advance_by(&mut t.simulator.memory, 1_000_000);
                                cx.notify();
                            })),
                    ),
            );
        } else if let Some(module) = bus.read_slot(slot) {
            body = body.child(
                DescriptionList::new()
                    .columns(2)
                    .small()
                    .item(tr("Modül"), module.kind().name(), 1)
                    .item(
                        "Firmware",
                        format!(
                            "V{:X}.{:02X}",
                            module.firmware() >> 8,
                            module.firmware() & 255
                        ),
                        1,
                    ),
            );
            if self.extra.config.module_tab == 0 {
                let mut channels = div().flex().flex_col().gap_4();
                for chunk in self
                    .extra
                    .samples
                    .iter()
                    .enumerate()
                    .collect::<Vec<_>>()
                    .chunks(2)
                {
                    channels = channels.child(div().flex().gap_4().children(chunk.iter().map(
                        |(i, input)| {
                            div().flex_1().min_w_0().child(Self::form(
                                &tfmt!("Kanal {} · ham kod", "Channel {} · raw code", i + 1),
                                input,
                                samples_locked,
                            ))
                        },
                    )));
                }
                body = body
                    .child(
                        GroupBox::new()
                            .title(tr("Analog giriş örnekleri"))
                            .outline()
                            .child(channels),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                self.button("samples-apply", tr("Kanalları uygula"), samples_locked)
                                    .primary()
                                    .on_click(cx.listener(|t, _, _, cx| t.module_action(3, cx))),
                            )
                            .child(
                                self.button("module-cycle", tr("Güç çevrimi"), locked)
                                    .on_click(cx.listener(|t, _, _, cx| t.module_action(4, cx))),
                            )
                            .child(
                                self.button("module-remove", tr("Modülü çıkar"), locked)
                                    .on_click(cx.listener(|t, _, _, cx| t.module_action(5, cx))),
                            ),
                    );
            } else {
                body=body.child(Alert::info("cr-readonly",tr("Register’lar salt okunur. Yazma işlemleri programdaki TO / DTO komutlarıyla yapılır.")).small()).child(data_table(&self.extra.config.module_table,380.));
            }
        } else {
            body=body.child(Alert::new("empty-slot",tr("Seçili slotta modül yok. DVP04AD ekleyerek analog kanal değerlerini ve kontrol register’larını inceleyin.")).title(tr("Boş genişleme slotu"))).child(div().flex().child(self.button("module-install",tr("DVP04AD ekle"),locked).primary().icon(IconName::Plus).on_click(cx.listener(|t,_,_,cx|t.module_action(2,cx)))));
        }
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(slots)
            .child(body)
            .into_any_element()
    }
    pub(crate) fn memory_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let locked = !self.topology_editable();
        let store = self.simulator.memory.file_memory.store();
        let tabs = TabBar::new("memory-tabs")
            .underline()
            .small()
            .selected_index(self.extra.config.memory_tab)
            .child(KitTab::new().label(tr("Register görüntüsü")))
            .child(KitTab::new().label(tr("Görüntü ayarları")))
            .on_click(cx.listener(|t, i, _, cx| {
                t.extra.config.memory_tab = *i;
                cx.notify();
            }));
        let mut body = page(
            tr("Dosya belleği"),
            tr("MEMR / MEMW için yerel kalıcı görüntü · proje dosyasından ayrı kaydedilir."),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    self.button("memory-open", tr("Görüntü aç"), locked)
                        .icon(IconName::FolderOpen)
                        .on_click(cx.listener(|t, _, _, cx| t.memory_open(cx))),
                )
                .child(
                    self.button(
                        "memory-save",
                        tr("Kaydet"),
                        !self.local_editable() || store.is_none(),
                    )
                    .primary()
                    .on_click(cx.listener(|t, _, _, cx| t.memory_save(false, cx))),
                )
                .child(
                    self.button(
                        "memory-save-as",
                        tr("Farklı kaydet"),
                        !self.local_editable() || store.is_none(),
                    )
                    .on_click(cx.listener(|t, _, _, cx| t.memory_save(true, cx))),
                ),
        )
        .child(tabs);
        if self.extra.config.memory_tab == 1 {
            body = body
                .child(
                    GroupBox::new()
                        .title(tr("Yeni görüntü"))
                        .outline()
                        .child(Self::form(
                            tr("Cihaz kimliği"),
                            &self.extra.memory_identity,
                            locked,
                        ))
                        .child(
                            RadioGroup::horizontal("memory-capacity")
                                .flex_none()
                                .h(px(28.))
                                .flex_shrink_0()
                                .selected_index(Some(usize::from(
                                    self.extra.memory_capacity == 8000,
                                )))
                                .disabled(locked)
                                .child(Radio::new("cap5000").label("ES2 · 5.000 WORD"))
                                .child(Radio::new("cap8000").label("ES2 · 8.000 WORD"))
                                .on_click(cx.listener(|t, i, _, cx| {
                                    t.extra.memory_capacity = if *i == 0 { 5000 } else { 8000 };
                                    cx.notify();
                                })),
                        )
                        .child(
                            div().child(
                                self.button("memory-new", tr("Yeni silinmiş bellek oluştur"), locked)
                                    .on_click(cx.listener(|t, _, _, cx| t.memory_new(cx))),
                            ),
                        ),
                )
                .child(
                    GroupBox::new()
                        .title(tr("Geniş komut uzantısı"))
                        .outline()
                        .child(
                            Switch::new("memory-wide")
                                .label(tr("DMEMR / DMEMW aday uzantısı"))
                                .checked(self.extra.memory_wide)
                                .disabled(locked)
                                .on_click(cx.listener(|t, value, _, cx| {
                                    t.extra.memory_wide = *value;
                                    if let Some(s) = t.simulator.memory.file_memory.store_mut() {
                                        s.set_installed_wide_extension(*value);
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(Self::heading(
                            tr("Bu seçenek mevcut görüntüdeki geniş komut kabulünü değiştirir."),
                        )),
                );
        } else if let Some(store) = store {
            body = body
                .child(
                    div().flex_shrink_0().child(
                        DescriptionList::new()
                            .columns(2)
                            .small()
                            .item(tr("Cihaz kimliği"), store.identity().to_string(), 1)
                            .item(tr("Kapasite"), format!("{} WORD", store.capacity_words()), 1)
                            .item(tr("Nesil"), store.generation().to_string(), 1)
                            .item(tr("Yazma işlemi"), store.write_transactions().to_string(), 1),
                    ),
                )
                .child(if store.dirty() {
                    Alert::warning("memory-state", tr("Kaydedilmemiş bellek değişiklikleri var."))
                        .small()
                } else {
                    Alert::success("memory-state", tr("Bellekte kaydedilmemiş değişiklik yok.")).small()
                })
                .child(
                    div().text_xs().text_color(rgb(MUTED)).child(
                        self.extra
                            .memory_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| tr("Dosya yolu henüz seçilmedi").into()),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .items_end()
                        .gap_3()
                        .child(div().w(px(210.)).child(Self::form(
                            tr("Başlangıç dosya indeksi"),
                            &self.extra.memory_index,
                            false,
                        )))
                        .child(
                            self.button(
                                "memory-prev",
                                tr("Önceki 32"),
                                self.extra
                                    .memory_index
                                    .read(cx)
                                    .value()
                                    .parse::<usize>()
                                    .unwrap_or(0)
                                    == 0,
                            )
                            .on_click(cx.listener(|t, _, w, cx| t.memory_page(false, w, cx))),
                        )
                        .child(
                            self.button(
                                "memory-next",
                                tr("Sonraki 32"),
                                self.extra
                                    .memory_index
                                    .read(cx)
                                    .value()
                                    .parse::<usize>()
                                    .unwrap_or(0)
                                    .saturating_add(32)
                                    >= store.words().len(),
                            )
                            .on_click(cx.listener(|t, _, w, cx| t.memory_page(true, w, cx))),
                        ),
                );
            let valid = self
                .extra
                .memory_index
                .read(cx)
                .value()
                .parse::<usize>()
                .is_ok_and(|i| i < store.words().len());
            if valid {
                body = body.child(data_table(&self.extra.config.memory_table, 390.));
            } else {
                body = body.child(
                    Alert::error(
                        "memory-index",
                        tfmt!(
                            "İndeks 0..{} aralığında olmalı.", "Index must be in the range 0..{}.",
                            store.capacity_words() - 1
                        ),
                    )
                    .small(),
                );
            }
            if self.extra.memory_recovery {
                body = body
                    .child(
                        Alert::warning(
                            "memory-recovery",
                            tr("Önceki doğrulanmış görüntü kurtarılabilir."),
                        )
                        .small(),
                    )
                    .child(
                        div().child(
                            self.button("memory-recover", tr("Doğrulanmış görüntüyü onar"), locked)
                                .on_click(cx.listener(|t, _, _, cx| t.memory_recover(cx))),
                        ),
                    );
            }
        } else {
            body=body.child(Alert::new("memory-empty",tr("Bir .odmem dosyası açın veya Görüntü ayarları sekmesinden yeni bellek oluşturun.")).title(tr("Bellek görüntüsü yok"))).child(div().flex().child(self.button("memory-setup",tr("Yeni görüntü ayarla"),locked).on_click(cx.listener(|t,_,_,cx|{t.extra.config.memory_tab=1;cx.notify();}))));
        }
        body.into_any_element()
    }
    fn memory_page(&mut self, next: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.simulator.memory.file_memory.store() {
            let i = self
                .extra
                .memory_index
                .read(cx)
                .value()
                .parse::<usize>()
                .unwrap_or(0);
            let index = if next {
                i.saturating_add(32).min(s.words().len().saturating_sub(1))
            } else {
                i.saturating_sub(32)
            };
            self.extra
                .memory_index
                .update(cx, |s, cx| s.set_value(index.to_string(), window, cx));
            cx.notify();
        }
    }
}
