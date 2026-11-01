//! Settings page: language and the preferences kept in `opendvp::settings`.
use super::*;
use gpui_component::{
    Theme,
    group_box::{GroupBox, GroupBoxVariants},
    switch::Switch,
};
use opendvp::settings;

impl Workbench {
    pub(super) fn settings_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = settings::get();
        let english = opendvp::i18n::is_en();
        let language = GroupBox::new().title(tr("Dil")).outline().child(
            div()
                .flex()
                .gap_2()
                .child(Button::new("lang-tr").label("Türkçe").small().when(!english, |b| b.primary()).on_click(cx.listener(
                    |this, _, window, cx| {
                        if opendvp::i18n::is_en() {
                            this.toggle_language(window, cx);
                        }
                    },
                )))
                .child(Button::new("lang-en").label("English").small().when(english, |b| b.primary()).on_click(cx.listener(
                    |this, _, window, cx| {
                        if !opendvp::i18n::is_en() {
                            this.toggle_language(window, cx);
                        }
                    },
                ))),
        );
        let files = GroupBox::new()
            .title(tr("Dosyalar"))
            .outline()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        Switch::new("set-backup")
                            .label(tr("Kaydederken önceki sürümü .bak olarak sakla"))
                            .checked(current.backup_on_save)
                            .on_click(cx.listener(|_, value, _, cx| {
                                settings::update(|s| s.backup_on_save = *value);
                                cx.notify();
                            })),
                    )
                    .child(
                        Switch::new("set-reopen")
                            .label(tr("Açılışta son projeyi aç"))
                            .checked(current.reopen_last)
                            .on_click(cx.listener(|_, value, _, cx| {
                                settings::update(|s| s.reopen_last = *value);
                                cx.notify();
                            })),
                    ),
            );
        let mut presets = div().flex().gap_2().items_center();
        for ms in [20_u64, 50, 100, 250, 500] {
            let id: SharedString = format!("poll-{ms}").into();
            presets = presets.child(
                Button::new(id)
                    .label(format!("{ms} ms"))
                    .small()
                    .when(current.poll_ms == ms, |b| b.primary())
                    .on_click(cx.listener(move |_, _, _, cx| {
                        settings::update(|s| s.poll_ms = ms);
                        cx.notify();
                    })),
            );
        }
        let live = GroupBox::new().title(tr("Canlı izleme")).outline().child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(MUTED))
                        .child(tr("İki durum isteği arasındaki süre. Küçük değer akıcıdır, bağlantıyı daha çok yükler.")),
                )
                .child(presets),
        );
        let (mcp_command, mcp_json) = mcp_texts(&current);
        let code_box = |id: &'static str, text: String| {
            div()
                .id(id)
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(rgb(MUTED))
                .text_xs()
                .font_family("Consolas")
                .child(text)
        };
        let copy_button = |id: &'static str, label: &'static str, text: String, cx: &mut Context<Self>| {
            Button::new(id)
                .label(tr(label))
                .small()
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                }))
        };
        let mut ports = div().flex().gap_2().items_center();
        for port in [8765_u16, 8766, 8080, 9000] {
            let id: SharedString = format!("mcp-port-{port}").into();
            ports = ports.child(
                Button::new(id)
                    .label(port.to_string())
                    .small()
                    .when(current.mcp_port == port, |b| b.primary())
                    .on_click(cx.listener(move |_, _, _, cx| {
                        settings::update(|s| s.mcp_port = port);
                        crate::mcp_server::restart_background();
                        cx.notify();
                    })),
            );
        }
        let scope = div()
            .flex()
            .gap_2()
            .child(
                Button::new("mcp-scope-local")
                    .label(tr("Yalnız bu bilgisayar"))
                    .small()
                    .when(!current.mcp_lan, |b| b.primary())
                    .on_click(cx.listener(|_, _, _, cx| {
                        settings::update(|s| s.mcp_lan = false);
                        crate::mcp_server::restart_background();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("mcp-scope-lan")
                    .label(tr("Tüm ağ (0.0.0.0)"))
                    .small()
                    .when(current.mcp_lan, |b| b.primary())
                    .on_click(cx.listener(|_, _, _, cx| {
                        settings::update(|s| s.mcp_lan = true);
                        crate::mcp_server::restart_background();
                        cx.notify();
                    })),
            );
        let has_token = !current.mcp_token.is_empty();
        let mut token_buttons = div().flex().gap_2().child(
            Button::new("mcp-token-new")
                .label(if has_token { tr("Yenile") } else { tr("Oluştur") })
                .small()
                .on_click(cx.listener(|_, _, _, cx| {
                    settings::update(|s| s.mcp_token = crate::mcp_server::new_token());
                    crate::mcp_server::restart_background();
                    cx.notify();
                })),
        );
        if has_token {
            token_buttons = token_buttons
                .child(copy_button("mcp-token-copy", "Anahtarı kopyala", current.mcp_token.clone(), cx))
                .child(
                    Button::new("mcp-token-remove")
                        .label(tr("Kaldır"))
                        .small()
                        .on_click(cx.listener(|_, _, _, cx| {
                            settings::update(|s| s.mcp_token.clear());
                            crate::mcp_server::restart_background();
                            cx.notify();
                        })),
                );
        }
        let token_text = if has_token {
            current.mcp_token.clone()
        } else {
            tr("Yok · kimlik doğrulaması kapalı").to_string()
        };
        let security_note = if has_token {
            tr("İstemciler Authorization: Bearer <anahtar> başlığını göndermelidir (aşağıdaki yapılandırmaya eklendi).")
        } else if current.mcp_lan {
            tr("Sunucu tüm ağ arayüzlerini dinliyor ve erişim anahtarı yok: ağdaki herkes araçları kullanabilir. Bir erişim anahtarı oluşturun veya yalnız bu bilgisayarı seçin.")
        } else {
            tr("Sunucu yalnız bu bilgisayardan erişilebilir.")
        };
        let label = |text: &'static str| div().text_sm().pt_2().child(tr(text));
        let mcp = GroupBox::new().title(tr("Yapay zeka (MCP)")).outline().child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(MUTED))
                        .child(tr("OpenDVP, yapay zeka asistanlarının program yazmasını, derlemesini, simüle etmesini ve (onayınızla) PLC'ye indirmesini sağlar.")),
                )
                .child(
                    Switch::new("set-mcp")
                        .label(tr("Uygulama açıldığında MCP sunucusunu başlat"))
                        .checked(current.mcp_server)
                        .on_click(cx.listener(|_, value, _, cx| {
                            settings::update(|s| s.mcp_server = *value);
                            if *value {
                                crate::mcp_server::start_background();
                            } else {
                                crate::mcp_server::stop_background();
                            }
                            cx.notify();
                        })),
                )
                .child(
                    div().text_sm().child(match crate::mcp_server::status() {
                        crate::mcp_server::Status::Running(port) => {
                            tfmt!("Sunucu çalışıyor · bağlantı noktası {port}", "Server running · port {port}")
                        }
                        crate::mcp_server::Status::Off => tr("Sunucu çalışmıyor").to_string(),
                        crate::mcp_server::Status::Failed(e) => {
                            tfmt!("Sunucu başlatılamadı: {e}", "Server could not start: {e}")
                        }
                    }),
                )
                .child(label("Bağlantı noktası"))
                .child(ports)
                .child(label("Dinleme"))
                .child(scope)
                .child(label("Erişim anahtarı"))
                .child(code_box("mcp-token", token_text))
                .child(token_buttons)
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(security_note),
                )
                .child(label("Pencere olmadan başlatmak için komut:"))
                .child(code_box("mcp-command", mcp_command.clone()))
                .child(div().flex().child(copy_button("mcp-copy-command", "Komutu kopyala", mcp_command, cx)))
                .child(label("MCP istemcinize eklenecek yapılandırma:"))
                .child(code_box("mcp-config", mcp_json.clone()))
                .child(div().flex().child(copy_button("mcp-copy-config", "Yapılandırmayı kopyala", mcp_json, cx)))
                .child(
                    div()
                        .text_xs()
                        .pt_2()
                        .text_color(rgb(MUTED))
                        .child(tr("PLC'ye yazan işlemler yine de onay ister. Yapay zeka penceredeki projeyi window_project_pull ile alabilir, window_project_push ile pencereye geri gönderebilir.")),
                ),
        );
        // ---- appearance
        let mut scales = div().flex().gap_2().items_center();
        for pct in [90_u16, 100, 110, 125, 150] {
            let id: SharedString = format!("scale-{pct}").into();
            scales = scales.child(
                Button::new(id)
                    .label(format!("{pct}%"))
                    .small()
                    .when(current.ui_scale == pct, |b| b.primary())
                    .on_click(cx.listener(move |_, _, _, cx| {
                        settings::update(|s| s.ui_scale = pct);
                        Theme::update(cx, |theme| theme.font_size = px(16.0 * f32::from(pct) / 100.0));
                        cx.notify();
                    })),
            );
        }
        let appearance = GroupBox::new().title(tr("Görünüm")).outline().child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(MUTED))
                        .child(tr("Arayüz yazı boyutu. Sabit boyutlu çizimler (ladder) değişmez.")),
                )
                .child(
                    Switch::new("set-dark")
                        .label(tr("Koyu tema"))
                        .checked(current.dark)
                        .on_click(cx.listener(|_, value, _, cx| {
                            settings::update(|s| s.dark = *value);
                            super::colors::apply(*value, cx);
                            cx.refresh_windows();
                            cx.notify();
                        })),
                )
                .child(scales),
        );

        // ---- saving and recovery
        let mut autosave = div().flex().gap_2().items_center();
        for minutes in [0_u8, 1, 5, 10, 30] {
            let id: SharedString = format!("autosave-{minutes}").into();
            let label = if minutes == 0 { tr("Kapalı").to_string() } else { tfmt!("{minutes} dk", "{minutes} min") };
            autosave = autosave.child(
                Button::new(id)
                    .label(label)
                    .small()
                    .when(current.autosave_min == minutes, |b| b.primary())
                    .on_click(cx.listener(move |_, _, _, cx| {
                        settings::update(|s| s.autosave_min = minutes);
                        cx.notify();
                    })),
            );
        }
        let folder_text = current
            .default_dir
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|| tr("Belirlenmedi").to_string());
        let mut folder_buttons = div().flex().gap_2().child(
            Button::new("dir-choose")
                .label(tr("Seç…"))
                .small()
                .on_click(cx.listener(|_, _, _, cx| {
                    let task = panels::dialog(|| {
                        rfd::FileDialog::new().pick_folder().map(Ok::<std::path::PathBuf, String>)
                    });
                    cx.spawn(async move |this, cx| {
                        if let Some(Ok(dir)) = task.await {
                            settings::update(|s| s.default_dir = Some(dir));
                        }
                        let _ = this.update(cx, |_, cx| cx.notify());
                    })
                    .detach();
                })),
        );
        if current.default_dir.is_some() {
            folder_buttons = folder_buttons.child(
                Button::new("dir-clear")
                    .label(tr("Temizle"))
                    .small()
                    .on_click(cx.listener(|_, _, _, cx| {
                        settings::update(|s| s.default_dir = None);
                        cx.notify();
                    })),
            );
        }
        let mut recent = div().flex().flex_col().gap_1();
        if current.recent.is_empty() {
            recent = recent.child(div().text_sm().text_color(rgb(MUTED)).child(tr("Henüz yok")));
        }
        for (index, path) in current.recent.iter().enumerate() {
            let open = path.clone();
            let id: SharedString = format!("recent-{index}").into();
            recent = recent.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new(id)
                            .label(tr("Aç"))
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| this.open_path(open.clone(), false, cx))),
                    )
                    .child(div().text_sm().child(path.display().to_string())),
            );
        }
        let recovery_files: Vec<std::path::PathBuf> = settings::recovery_dir()
            .and_then(|dir| std::fs::read_dir(dir).ok())
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "odvp"))
                    .collect()
            })
            .unwrap_or_default();
        let mut recovery = div().flex().flex_col().gap_1();
        if recovery_files.is_empty() {
            recovery = recovery.child(div().text_sm().text_color(rgb(MUTED)).child(tr("Henüz yok")));
        }
        for (index, path) in recovery_files.into_iter().enumerate() {
            let (open, delete) = (path.clone(), path.clone());
            let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            recovery = recovery.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new(SharedString::from(format!("recovery-open-{index}")))
                            .label(tr("Aç"))
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| this.open_path(open.clone(), true, cx))),
                    )
                    .child(
                        Button::new(SharedString::from(format!("recovery-delete-{index}")))
                            .label(tr("Sil"))
                            .small()
                            .on_click(cx.listener(move |_, _, _, cx| {
                                let _ = std::fs::remove_file(&delete);
                                cx.notify();
                            })),
                    )
                    .child(div().text_sm().child(name)),
            );
        }
        let muted_note = |text: &'static str| div().text_xs().text_color(rgb(MUTED)).child(tr(text));
        let heading = |text: &'static str| div().text_sm().pt_2().child(tr(text));
        let saving = GroupBox::new().title(tr("Kayıt ve kurtarma")).outline().child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(heading("Otomatik kurtarma kopyası"))
                .child(autosave)
                .child(muted_note("Kaydedilmemiş değişiklik varken bu aralıkla kurtarma kopyası yazılır; Kaydet kopyayı siler."))
                .child(heading("Kurtarma kopyaları"))
                .child(recovery)
                .child(heading("Varsayılan proje klasörü"))
                .child(div().text_sm().text_color(rgb(MUTED)).child(folder_text))
                .child(folder_buttons)
                .child(heading("Son projeler"))
                .child(recent),
        );

        // ---- new project defaults
        let mut models = div().flex().flex_wrap().gap_2();
        for model in opendvp::devices::MODELS {
            let id: SharedString = format!("newdef-model-{}", model.id).into();
            let model_id = model.id;
            models = models.child(
                Button::new(id)
                    .label(model.id)
                    .small()
                    .when(current.new_model.eq_ignore_ascii_case(model_id), |b| b.primary())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        settings::update(|s| s.new_model = model_id.to_string());
                        this.extra.new_model = model_id;
                        cx.notify();
                    })),
            );
        }
        let language_button = |id: &'static str, label: &'static str, st: bool, cx: &mut Context<Self>| {
            Button::new(id)
                .label(label)
                .small()
                .when(current.new_st == st, |b| b.primary())
                .on_click(cx.listener(move |this, _, _, cx| {
                    settings::update(|s| s.new_st = st);
                    this.extra.new_st = st;
                    cx.notify();
                }))
        };
        let mut scans = div().flex().gap_2().items_center();
        for ms in [10_u64, 20, 50, 100] {
            let id: SharedString = format!("scan-{ms}").into();
            scans = scans.child(
                Button::new(id)
                    .label(format!("{ms} ms"))
                    .small()
                    .when(current.scan_ms == ms, |b| b.primary())
                    .on_click(cx.listener(move |_, _, _, cx| {
                        settings::update(|s| s.scan_ms = ms);
                        cx.notify();
                    })),
            );
        }
        let new_project = GroupBox::new().title(tr("Yeni proje varsayılanları")).outline().child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(heading("Varsayılan model"))
                .child(models)
                .child(heading("Varsayılan dil"))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(language_button("newdef-il", tr("Ladder / IL"), false, cx))
                        .child(language_button("newdef-st", "Structured Text", true, cx)),
                )
                .child(heading("Simülatör tarama süresi"))
                .child(scans)
                .child(muted_note("Her tarama simüle edilen süreyi bu kadar ilerletir; zamanlayıcıların çözünürlüğünü belirler.")),
        );

        // ---- PLC safety and reset
        let plc_safety = GroupBox::new().title(tr("PLC güvenliği")).outline().child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    Switch::new("set-plc-readonly")
                        .label(tr("PLC'ye yazmayı kapat (salt okunur)"))
                        .checked(current.plc_read_only)
                        .on_click(cx.listener(|_, value, _, cx| {
                            settings::update(|s| s.plc_read_only = *value);
                            cx.notify();
                        })),
                )
                .child(muted_note("Açıkken program yükleme, RUN/STOP ve zorlama kapalıdır; durum okuma ve program okuma çalışır."))
                .child(
                    div().pt_2().flex().child(
                        Button::new("settings-reset")
                            .label(tr("Varsayılanlara sıfırla"))
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                settings::reset_defaults();
                                let defaults = settings::get();
                                super::colors::apply(false, cx);
                                cx.refresh_windows();
                                this.extra.new_model = opendvp::devices::find(&defaults.new_model)
                                    .map_or(opendvp::devices::DEFAULT, |m| m.id);
                                this.extra.new_st = defaults.new_st;
                                cx.notify();
                            })),
                    ),
                )
                .child(muted_note("Dil dışındaki tüm ayarlar varsayılana döner; son projeler korunur.")),
        );
        div()
            .id("settings-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_4()
            .max_w(px(640.))
            .child(language)
            .child(appearance)
            .child(files)
            .child(saving)
            .child(new_project)
            .child(live)
            .child(plc_safety)
            .child(mcp)
            .into_any_element()
    }
}

/// Command that starts the HTTP server and the matching client configuration (`mcpServers`
/// JSON shared by Claude Desktop, Claude Code and others).
fn mcp_texts(current: &settings::Settings) -> (String, String) {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "opendvp".into());
    let command = if exe.contains(' ') {
        format!("\"{exe}\" --mcp-http")
    } else {
        format!("{exe} --mcp-http")
    };
    let mut server = serde_json::json!({
        "type": "http",
        "url": format!("http://127.0.0.1:{}/mcp", current.mcp_port),
    });
    if !current.mcp_token.is_empty() {
        server["headers"] = serde_json::json!({ "Authorization": format!("Bearer {}", current.mcp_token) });
    }
    let config = serde_json::json!({ "mcpServers": { "opendvp": server } });
    (command, serde_json::to_string_pretty(&config).unwrap_or_default())
}
