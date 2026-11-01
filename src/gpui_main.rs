#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
// App-owned locale files: `gpui_component:` entries add Turkish to the component library.
rust_i18n::i18n!("locales", fallback = "en");
mod connection_profiles;
mod device_search;
mod editor_search;
mod gpui_model;
mod gpui_workbench;
mod instruction_help;
mod ladder_plot;
mod ladder_source;
mod mcp_server;
mod palette;
mod plc_connection;
mod program_diff;
mod report;
mod xref;
use gpui::*;
use gpui_component::Root;
fn main() {
    // `opendvp --mcp` (stdio) or `opendvp --mcp-http[=PORT]` (streamable HTTP, scope and token
    // from the settings): serve MCP for AI assistants instead of opening the window.
    let flag = std::env::args().skip(1).find(|a| a.starts_with("--mcp"));
    if let Some(flag) = flag {
        let result = match flag.split_once('=') {
            None if flag == "--mcp" => mcp_server::serve_stdio(),
            None if flag == "--mcp-http" => mcp_server::serve_http(opendvp::settings::get().mcp_port),
            Some(("--mcp-http", port)) => match port.parse() {
                Ok(port) => mcp_server::serve_http(port),
                Err(_) => {
                    eprintln!("opendvp: invalid port '{port}'");
                    std::process::exit(2);
                }
            },
            _ => {
                eprintln!("opendvp: unknown option '{flag}' (use --mcp or --mcp-http[=PORT])");
                std::process::exit(2);
            }
        };
        if let Err(error) = result {
            eprintln!("opendvp {flag}: {error}");
            std::process::exit(1);
        }
        return;
    }
    gpui_platform::application()
        .with_assets(gpui_kit_assets::Assets)
        .run(|cx| {
            if opendvp::settings::get().mcp_server {
                mcp_server::start_background();
            }
            rust_i18n::extend!(gpui_component);
            gpui_component::init(cx);
            let lang = opendvp::i18n::saved().unwrap_or(opendvp::i18n::Lang::Tr);
            opendvp::i18n::set_lang(lang);
            gpui_component::set_locale(lang.code());
            gpui_workbench::bind_keys(cx);
            gpui_workbench::colors::apply(opendvp::settings::get().dark, cx);
            let bounds = Bounds::centered(None, size(px(1440.), px(920.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(1000.), px(650.))),
                    // The application bar draws the title and the window controls.
                    titlebar: Some(TitlebarOptions {
                        title: Some("OpenDVP".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| gpui_workbench::Workbench::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("GPUI window could not be created");
            cx.activate(true);
        });
}
