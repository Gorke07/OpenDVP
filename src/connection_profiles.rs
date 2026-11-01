//! User-managed connection profiles. Device I/O remains owned by the connection worker.
use crate::plc_connection::{Config, Parity, PortInfo, Protocol, Transport};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    UsbSerial,
    Serial,
    Ethernet,
    Simulator,
}
impl Kind {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::UsbSerial => "USB / seri",
            Self::Serial => "Seri / COM",
            Self::Ethernet => "Ethernet",
            Self::Simulator => "ES2 / SS2 simülatörü",
        }
    }
}
// Tested connection-profile event model; the window drives the profile form directly.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Port,
    Baud,
    DataBits,
    Parity,
    StopBits,
    Protocol,
    Host,
    TcpPort,
    Station,
    Timeout,
    Poll,
    XStart,
    XCount,
    YStart,
    YCount,
    SimulatorPath,
}
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Event {
    Select(usize),
    Add,
    Remove,
    UndoRemove,
    Kind(Kind),
    Field(Field, String),
    UsePort(String),
    RefreshPorts,
    Connect,
    Disconnect,
    OpenIo,
    OpenSimulator,
    BrowseSimulator,
    SaveProfiles,
}
/// Status is owned by the caller; all values are display-only.
#[allow(dead_code)]
pub struct Context<'a> {
    pub status: &'a str,
    pub error: Option<&'a str>,
    pub busy: bool,
    pub connected: bool,
    pub endpoint: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) port: String,
    pub(crate) baud: String,
    pub(crate) data_bits: String,
    pub(crate) parity: String,
    pub(crate) stop_bits: String,
    pub(crate) protocol: String,
    pub(crate) host: String,
    pub(crate) tcp_port: String,
    pub(crate) station: String,
    pub(crate) timeout: String,
    pub(crate) poll: String,
    pub(crate) x_start: String,
    pub(crate) x_count: String,
    pub(crate) y_start: String,
    pub(crate) y_count: String,
    pub(crate) simulator_path: String,
}
impl Profile {
    fn new(name: &str, kind: Kind) -> Self {
        Self {
            name: name.into(),
            kind,
            port: String::new(),
            baud: "9600".into(),
            data_bits: "7".into(),
            parity: "Even".into(),
            stop_bits: "1".into(),
            protocol: if kind == Kind::Ethernet {
                "Tcp"
            } else {
                "Ascii"
            }
            .into(),
            host: String::new(),
            tcp_port: "502".into(),
            station: "1".into(),
            timeout: "1000".into(),
            poll: "500".into(),
            x_start: "0".into(),
            x_count: "16".into(),
            y_start: "0".into(),
            y_count: "16".into(),
            simulator_path: opendvp::es2_bridge::DEFAULT_SIMULATOR.into(),
        }
    }
    fn check_name(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.chars().count() > 100 {
            return Err("Profil adı 1–100 karakter olmalı.".into());
        }
        Ok(())
    }
    fn check_lengths(&self) -> Result<(), String> {
        let fields = [
            &self.name,
            &self.port,
            &self.baud,
            &self.data_bits,
            &self.parity,
            &self.stop_bits,
            &self.protocol,
            &self.host,
            &self.tcp_port,
            &self.station,
            &self.timeout,
            &self.poll,
            &self.x_start,
            &self.x_count,
            &self.y_start,
            &self.y_count,
            &self.simulator_path,
        ];
        if fields.iter().any(|value| value.len() > 2048) {
            return Err("Profil alanı çok uzun.".into());
        }
        Ok(())
    }
    fn config(&self) -> Result<Config, String> {
        self.check_name()?;
        let transport = match self.kind {
            Kind::UsbSerial => Transport::UsbSerial,
            Kind::Serial => Transport::Serial,
            Kind::Ethernet => Transport::Tcp,
            Kind::Simulator => {
                return Err("Simülatör profili fiziksel bağlantı yapılandırması değildir.".into());
            }
        };
        let number = |value: &str, name: &str| -> Result<u64, String> {
            value
                .trim()
                .parse()
                .map_err(|_| format!("{name} geçerli bir sayı olmalı."))
        };
        let narrow = |value: &str, name: &str| -> Result<u16, String> {
            u16::try_from(number(value, name)?).map_err(|_| format!("{name} aralık dışında."))
        };
        let small = |value: &str, name: &str| -> Result<u8, String> {
            u8::try_from(number(value, name)?).map_err(|_| format!("{name} aralık dışında."))
        };
        let start = |value: &str, prefix: char| -> Result<u16, String> {
            let upper = value.trim().to_ascii_uppercase();
            let digits = upper.strip_prefix(prefix).unwrap_or(&upper);
            u16::from_str_radix(digits, 8)
                .map_err(|_| format!("{prefix} başlangıcı sekizlik adres olmalı."))
        };
        let mut config = Config {
            transport,
            station: small(&self.station, "İstasyon")?,
            timeout_ms: number(&self.timeout, "Zaman aşımı")?,
            poll_ms: number(&self.poll, "Okuma aralığı")?,
            x_start: start(&self.x_start, 'X')?,
            x_count: narrow(&self.x_count, "X adedi")?,
            y_start: start(&self.y_start, 'Y')?,
            y_count: narrow(&self.y_count, "Y adedi")?,
            ..Config::default()
        };
        if self.kind == Kind::Ethernet {
            config.protocol = Protocol::Tcp;
            config.host = self.host.trim().into();
            config.tcp_port = narrow(&self.tcp_port, "TCP portu")?;
        } else {
            config.port_name = self.port.trim().into();
            config.baud = u32::try_from(number(&self.baud, "Baud")?)
                .map_err(|_| "Baud aralık dışında.".to_string())?;
            config.data_bits = small(&self.data_bits, "Veri biti")?;
            config.stop_bits = small(&self.stop_bits, "Dur biti")?;
            config.parity = match self.parity.as_str() {
                "None" => Parity::None,
                "Even" => Parity::Even,
                "Odd" => Parity::Odd,
                _ => return Err("Geçersiz parite.".into()),
            };
            config.protocol = match self.protocol.as_str() {
                "Ascii" => Protocol::Ascii,
                "Rtu" => Protocol::Rtu,
                _ => return Err("Geçersiz seri protokol.".into()),
            };
        }
        config.validate()?;
        Ok(config)
    }
    fn valid_for_save(&self) -> Result<(), String> {
        self.check_name()?;
        if self.kind == Kind::Simulator {
            if self.simulator_path.trim().is_empty() {
                return Err("Simülatör yolu boş olamaz.".into());
            }
            Ok(())
        } else {
            self.config().map(|_| ())
        }
    }
    pub(crate) fn field_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Name => &mut self.name,
            Field::Port => &mut self.port,
            Field::Baud => &mut self.baud,
            Field::DataBits => &mut self.data_bits,
            Field::Parity => &mut self.parity,
            Field::StopBits => &mut self.stop_bits,
            Field::Protocol => &mut self.protocol,
            Field::Host => &mut self.host,
            Field::TcpPort => &mut self.tcp_port,
            Field::Station => &mut self.station,
            Field::Timeout => &mut self.timeout,
            Field::Poll => &mut self.poll,
            Field::XStart => &mut self.x_start,
            Field::XCount => &mut self.x_count,
            Field::YStart => &mut self.y_start,
            Field::YCount => &mut self.y_count,
            Field::SimulatorPath => &mut self.simulator_path,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u8,
    pub(crate) selected: usize,
    pub(crate) profiles: Vec<Profile>,
}
#[derive(Debug)]
pub struct Manager {
    pub(crate) profiles: Vec<Profile>,
    pub(crate) selected: usize,
    pub(crate) removed: Option<(usize, Profile)>,
    pub(crate) ports: Vec<PortInfo>,
    pub(crate) dirty: bool,
    pub feedback: Option<String>,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            profiles: vec![
                Profile::new("Delta DVP · USB", Kind::UsbSerial),
                Profile::new("Delta DVP · Ethernet", Kind::Ethernet),
                Profile::new("ES2 / SS2 simülatörü", Kind::Simulator),
            ],
            selected: 0,
            removed: None,
            ports: Vec::new(),
            dirty: false,
            feedback: None,
        }
    }
}
impl Manager {
    pub fn active_kind(&self) -> Kind {
        self.profiles[self.selected].kind
    }
    #[allow(dead_code)]
    pub fn active_name(&self) -> &str {
        &self.profiles[self.selected].name
    }
    #[allow(dead_code)]
    pub fn simulator_path(&self) -> &str {
        &self.profiles[self.selected].simulator_path
    }
    pub fn active_config(&self) -> Result<Config, String> {
        self.profiles[self.selected].config()
    }
    #[cfg(test)]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub fn set_ports(&mut self, ports: Vec<PortInfo>) {
        self.ports = ports;
    }
    /// The caller supplies whether configuration is editable; connection operations
    /// are emitted separately and never performed here.
    pub fn update(&mut self, event: Event, editable: bool) {
        match event {
            Event::Select(i) if editable && i < self.profiles.len() => {
                self.dirty |= self.selected != i;
                self.selected = i;
                self.feedback = None;
            }
            Event::Add if editable && self.profiles.len() < 32 => {
                let mut p = self.profiles[self.selected].clone();
                p.name = format!("{} kopyası", p.name.chars().take(90).collect::<String>());
                self.profiles.push(p);
                self.selected = self.profiles.len() - 1;
                self.dirty = true;
            }
            Event::Remove if editable && self.profiles.len() > 1 => {
                let index = self.selected;
                self.removed = Some((index, self.profiles.remove(index)));
                self.selected = index.min(self.profiles.len() - 1);
                self.dirty = true;
            }
            Event::UndoRemove if editable && self.profiles.len() < 32 => {
                if let Some((i, p)) = self.removed.take() {
                    let i = i.min(self.profiles.len());
                    self.profiles.insert(i, p);
                    self.selected = i;
                    self.dirty = true;
                }
            }
            Event::Kind(k) if editable => {
                let p = &mut self.profiles[self.selected];
                p.kind = k;
                p.protocol = if k == Kind::Ethernet { "Tcp" } else { "Ascii" }.into();
                self.dirty = true;
            }
            Event::Field(field, value) if editable && value.len() <= 2048 => {
                *self.profiles[self.selected].field_mut(field) = value;
                self.dirty = true;
                self.feedback = None;
            }
            Event::UsePort(port) if editable && self.ports.iter().any(|p| p.name == port) => {
                self.profiles[self.selected].port = port;
                self.dirty = true;
            }
            _ => {}
        }
    }
    /// Save explicitly, with atomic replacement in the same directory.
    /// Validate the active profile; inactive drafts are revalidated on connect.
    pub fn save_to(&mut self, path: &Path) -> Result<(), String> {
        for profile in &self.profiles {
            profile.check_name()?;
            profile.check_lengths()?;
        }
        self.profiles[self.selected].valid_for_save()?;
        let stored = Stored {
            version: 1,
            selected: self.selected,
            profiles: self.profiles.clone(),
        };
        let json = serde_json::to_vec_pretty(&stored).map_err(|e| e.to_string())?;
        let parent = path.parent().ok_or("Geçersiz profil dosyası yolu.")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let tmp = parent.join(format!(
            ".opendvp-profiles-{}-{nonce}.tmp",
            std::process::id()
        ));
        let result = (|| -> Result<(), String> {
            use std::io::Write;
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .map_err(|e| e.to_string())?;
            f.write_all(&json).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            drop(f);
            fs::rename(&tmp, path).map_err(|e| e.to_string())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result?;
        self.dirty = false;
        self.removed = None;
        Ok(())
    }
    /// Explicit load, replacing state only after the entire file is checked.
    pub fn load_from(&mut self, path: &Path) -> Result<(), String> {
        if fs::metadata(path).map_err(|e| e.to_string())?.len() > 256 * 1024 {
            return Err("Profil dosyası çok büyük.".into());
        }
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() > 256 * 1024 {
            return Err("Profil dosyası çok büyük.".into());
        }
        let saved: Stored =
            serde_json::from_slice(&bytes).map_err(|e| format!("Profil dosyası okunamadı: {e}"))?;
        if saved.version != 1
            || saved.profiles.is_empty()
            || saved.profiles.len() > 32
            || saved.selected >= saved.profiles.len()
        {
            return Err("Profil dosyası sürümü veya seçimi geçersiz.".into());
        }
        for p in &saved.profiles {
            p.check_name()?;
            p.check_lengths()?;
            if serde_json::to_vec(p).map_err(|e| e.to_string())?.len() > 32 * 1024 {
                return Err("Profil çok büyük.".into());
            }
        }
        self.profiles = saved.profiles;
        self.selected = saved.selected;
        self.removed = None;
        self.dirty = false;
        self.feedback = None;
        Ok(())
    }
    #[cfg(test)]
    pub fn selected_kind(&self) -> Kind {
        self.active_kind()
    }
    pub fn set_simulator_path(&mut self, path: String) {
        if path.len() <= 2048 {
            self.profiles[self.selected].simulator_path = path;
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "opendvp-connection-test-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn blank_port_and_invalid_values_are_rejected_without_opening_a_port() {
        let mut m = Manager::default();
        assert_eq!(m.selected_kind(), Kind::UsbSerial);
        assert!(m.active_config().is_err());
        m.update(Event::Field(Field::Port, "COM9".into()), true);
        m.update(Event::Field(Field::XStart, "X10".into()), true);
        let c = m.active_config().unwrap();
        assert_eq!(c.x_start, 8);
        assert_eq!(c.protocol, Protocol::Ascii);
        m.update(Event::Field(Field::XStart, "X8".into()), true);
        assert!(m.active_config().unwrap_err().contains("sekizlik"));
        m.update(Event::Field(Field::XStart, "0".into()), true);
        m.update(Event::Field(Field::Timeout, "garbage".into()), true);
        assert!(m.active_config().is_err());
    }
    #[test]
    fn locked_manager_ignores_all_profile_mutations() {
        let mut m = Manager::default();
        m.set_ports(vec![PortInfo {
            name: "COM9".into(),
            label: "test".into(),
            usb: true,
        }]);
        for e in [
            Event::Select(1),
            Event::Add,
            Event::Remove,
            Event::Kind(Kind::Ethernet),
            Event::UsePort("COM9".into()),
            Event::Field(Field::Name, "mutated".into()),
        ] {
            m.update(e, false);
        }
        assert_eq!(m.selected_kind(), Kind::UsbSerial);
        assert_eq!(m.active_name(), "Delta DVP · USB");
        assert!(!m.is_dirty());
        assert_eq!(m.profiles.len(), 3);
    }
    #[test]
    fn all_discovered_ports_are_selectable_and_selection_is_persistable() {
        let mut m = Manager::default();
        m.set_ports(
            (1..=8)
                .map(|n| PortInfo {
                    name: format!("COM{n}"),
                    label: format!("COM{n} · adapter"),
                    usb: true,
                })
                .collect(),
        );
        m.update(Event::UsePort("COM8".into()), true);
        assert_eq!(m.active_config().unwrap().port_name, "COM8");
        assert!(m.is_dirty());
        let mut m = Manager::default();
        m.update(Event::Select(1), true);
        assert!(m.is_dirty());
    }
    #[test]
    fn save_reload_and_invalid_file_keep_previous_state() {
        let path = temp_path();
        let mut m = Manager::default();
        assert!(m.save_to(&path).is_err());
        assert!(!path.exists());
        m.update(Event::Field(Field::Port, "COM7".into()), true);
        m.update(Event::Field(Field::Name, "Test hattı".into()), true);
        m.save_to(&path).unwrap();
        assert!(!m.is_dirty());
        let mut loaded = Manager::default();
        loaded.load_from(&path).unwrap();
        assert_eq!(loaded.active_name(), "Test hattı");
        assert_eq!(loaded.active_config().unwrap().port_name, "COM7");
        fs::write(&path, b"{ invalid").unwrap();
        assert!(loaded.load_from(&path).is_err());
        assert_eq!(loaded.active_name(), "Test hattı");
        let _ = fs::remove_file(path);
    }
    #[test]
    fn profile_removal_can_be_undone_and_simulator_is_not_physical_config() {
        let mut m = Manager::default();
        m.update(Event::Select(2), true);
        assert_eq!(m.selected_kind(), Kind::Simulator);
        assert!(m.active_config().is_err());
        m.update(Event::Remove, true);
        assert_eq!(m.profiles.len(), 2);
        m.update(Event::UndoRemove, true);
        assert_eq!(m.profiles.len(), 3);
        assert_eq!(m.selected_kind(), Kind::Simulator);
        m.set_simulator_path("C:\\Simulator.exe".into());
        assert_eq!(m.simulator_path(), "C:\\Simulator.exe");
    }
    #[test]
    fn hidden_transport_drafts_do_not_block_selected_transport() {
        let mut m = Manager::default();
        m.update(Event::Field(Field::Baud, "invalid".into()), true);
        m.update(Event::Kind(Kind::Ethernet), true);
        assert!(m.active_config().is_err()); // IP must be supplied explicitly
        m.update(Event::Field(Field::Host, "192.168.1.10".into()), true);
        assert_eq!(m.active_config().unwrap().protocol, Protocol::Tcp);
        m.update(Event::Field(Field::TcpPort, "invalid".into()), true);
        m.update(Event::Kind(Kind::Serial), true);
        m.update(Event::Field(Field::Port, "COM9".into()), true);
        m.update(Event::Field(Field::Baud, "9600".into()), true);
        assert_eq!(m.active_config().unwrap().protocol, Protocol::Ascii);
    }
    #[test]
    fn overwrite_and_reject_oversize_fields_without_mutating_state() {
        let path = temp_path();
        let mut m = Manager::default();
        m.update(Event::Field(Field::Port, "COM7".into()), true);
        m.save_to(&path).unwrap();
        m.update(Event::Field(Field::Port, "COM8".into()), true);
        m.save_to(&path).unwrap();
        let mut loaded = Manager::default();
        loaded.load_from(&path).unwrap();
        assert_eq!(loaded.active_config().unwrap().port_name, "COM8");
        let mut bad = serde_json::json!({
            "version": 1,
            "selected": 0,
            "profiles": m.profiles.clone(),
        });
        bad["profiles"][0]["host"] = serde_json::json!("x".repeat(2049));
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(loaded.load_from(&path).is_err());
        assert_eq!(loaded.active_config().unwrap().port_name, "COM8");
        let _ = fs::remove_file(path);
    }
}
