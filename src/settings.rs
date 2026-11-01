//! User preferences kept between runs in `%LOCALAPPDATA%\OpenDVP\settings.txt` (`key=value`
//! lines). The language lives in its own file (`i18n`); everything else is here.
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Keep the previous `.odvp` next to the new one as `.odvp.bak` on every save.
    pub backup_on_save: bool,
    /// Interval between two live-status requests to the simulator, in milliseconds.
    pub poll_ms: u64,
    /// Open the project that was open when the program was closed.
    pub reopen_last: bool,
    /// Path of the last opened or saved project.
    pub last_project: Option<PathBuf>,
    /// Start the MCP HTTP server in the background when the program opens.
    pub mcp_server: bool,
    /// TCP port of the MCP HTTP server.
    pub mcp_port: u16,
    /// Listen on every network interface (0.0.0.0) instead of this computer only (127.0.0.1).
    pub mcp_lan: bool,
    /// Bearer token clients must send; empty = no authentication.
    pub mcp_token: String,
    /// Interface text size in percent of the normal size.
    pub ui_scale: u16,
    /// Minutes between recovery copies while there are unsaved changes (0 = off).
    pub autosave_min: u8,
    /// Most recently opened or saved projects, newest first.
    pub recent: Vec<PathBuf>,
    /// Folder the open/save dialogs start in.
    pub default_dir: Option<PathBuf>,
    /// Model id and language preselected on the new-project screen.
    pub new_model: String,
    pub new_st: bool,
    /// Simulated milliseconds per simulator scan.
    pub scan_ms: u64,
    /// Refuse every action that writes to a real PLC.
    pub plc_read_only: bool,
    /// Dark colour theme.
    pub dark: bool,
}

pub const POLL_MIN: u64 = 20;
pub const POLL_MAX: u64 = 1000;
pub const RECENT_MAX: usize = 8;

impl Default for Settings {
    fn default() -> Self {
        Self { backup_on_save: true, poll_ms: 50, reopen_last: false, last_project: None, mcp_server: true, mcp_port: 8765, mcp_lan: true, mcp_token: String::new(), ui_scale: 100, autosave_min: 5, recent: Vec::new(), default_dir: None, new_model: "ES2".into(), new_st: false, scan_ms: 20, plc_read_only: false, dark: false }
    }
}

impl Settings {
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            match key.trim() {
                "backup_on_save" => s.backup_on_save = value == "1",
                "reopen_last" => s.reopen_last = value == "1",
                "mcp_server" => s.mcp_server = value == "1",
                "mcp_lan" => s.mcp_lan = value == "1",
                "mcp_token" => s.mcp_token = value.to_owned(),
                "mcp_port" => {
                    if let Ok(v) = value.parse::<u16>() {
                        if v >= 1024 {
                            s.mcp_port = v;
                        }
                    }
                }
                "plc_read_only" => s.plc_read_only = value == "1",
                "dark" => s.dark = value == "1",
                "new_st" => s.new_st = value == "1",
                "new_model" if !value.is_empty() => s.new_model = value.to_owned(),
                "ui_scale" => {
                    if let Ok(v) = value.parse::<u16>() {
                        s.ui_scale = v.clamp(80, 160);
                    }
                }
                "autosave_min" => {
                    if let Ok(v) = value.parse::<u8>() {
                        s.autosave_min = v.min(60);
                    }
                }
                "scan_ms" => {
                    if let Ok(v) = value.parse::<u64>() {
                        s.scan_ms = v.clamp(5, 500);
                    }
                }
                "default_dir" if !value.is_empty() => s.default_dir = Some(PathBuf::from(value)),
                "recent" if !value.is_empty() && s.recent.len() < RECENT_MAX => {
                    s.recent.push(PathBuf::from(value))
                }
                "poll_ms" => {
                    if let Ok(v) = value.parse::<u64>() {
                        s.poll_ms = v.clamp(POLL_MIN, POLL_MAX);
                    }
                }
                "last_project" if !value.is_empty() => s.last_project = Some(PathBuf::from(value)),
                _ => {}
            }
        }
        s
    }
    pub fn render(&self) -> String {
        format!(
            "backup_on_save={}\nreopen_last={}\nmcp_server={}\nmcp_lan={}\nmcp_port={}\nmcp_token={}\nplc_read_only={}\ndark={}\nnew_st={}\nnew_model={}\nui_scale={}\nautosave_min={}\nscan_ms={}\ndefault_dir={}\npoll_ms={}\nlast_project={}\n{}",
            u8::from(self.backup_on_save),
            u8::from(self.reopen_last),
            u8::from(self.mcp_server),
            u8::from(self.mcp_lan),
            self.mcp_port,
            self.mcp_token,
            u8::from(self.plc_read_only),
            u8::from(self.dark),
            u8::from(self.new_st),
            self.new_model,
            self.ui_scale,
            self.autosave_min,
            self.scan_ms,
            self.default_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            self.poll_ms,
            self.last_project.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            self.recent.iter().map(|p| format!("recent={}\n", p.display())).collect::<String>()
        )
    }
}

fn file() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("OpenDVP").join("settings.txt"))
}

fn cell() -> &'static Mutex<Settings> {
    static CELL: OnceLock<Mutex<Settings>> = OnceLock::new();
    CELL.get_or_init(|| {
        let loaded = file()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Settings::parse(&t))
            .unwrap_or_default();
        Mutex::new(loaded)
    })
}

/// The current settings.
pub fn get() -> Settings {
    cell().lock().map(|s| s.clone()).unwrap_or_default()
}

/// Change the settings and write them out (a failed write only loses the preference).
pub fn update(change: impl FnOnce(&mut Settings)) {
    let Ok(mut guard) = cell().lock() else { return };
    change(&mut guard);
    if let Some(path) = file() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, guard.render());
    }
}

/// Remember `path` as the most recent project.
pub fn push_recent(path: &std::path::Path) {
    update(|s| {
        s.recent.retain(|p| p != path);
        s.recent.insert(0, path.to_path_buf());
        s.recent.truncate(RECENT_MAX);
    });
}

/// Folder of the autosaved recovery copies.
pub fn recovery_dir() -> Option<PathBuf> {
    file()?.parent().map(|dir| dir.join("recovery"))
}

/// Back to the defaults; the recent-project list and the last project are kept.
pub fn reset_defaults() {
    update(|s| {
        let (recent, last) = (std::mem::take(&mut s.recent), s.last_project.take());
        // The access token is a security setting: a reset must not silently drop it.
        let token = std::mem::take(&mut s.mcp_token);
        *s = Settings::default();
        s.recent = recent;
        s.last_project = last;
        s.mcp_token = token;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = Settings {
            backup_on_save: false,
            poll_ms: 200,
            reopen_last: true,
            last_project: Some(r"C:\a b\p.odvp".into()),
            mcp_server: false,
            mcp_port: 9000,
            mcp_lan: false,
            mcp_token: "abc123".into(),
            ui_scale: 125,
            autosave_min: 10,
            recent: vec![r"C:\x\a.odvp".into(), r"C:\x\b.odvp".into()],
            default_dir: Some(r"C:\work dir".into()),
            new_model: "SS2".into(),
            new_st: true,
            scan_ms: 50,
            plc_read_only: true,
            dark: true,
        };
        assert_eq!(Settings::parse(&s.render()), s);
    }
    #[test]
    fn bad_values_fall_back() {
        let s = Settings::parse("poll_ms=5\nfoo\nbackup_on_save=0\n");
        assert_eq!(s.poll_ms, POLL_MIN);
        assert!(!s.backup_on_save);
        assert_eq!(Settings::parse("poll_ms=x").poll_ms, 50);
    }
}
