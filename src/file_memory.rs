//! Explicit retained file-register backend. Installed simulators only skip these
//! commands; policies here are documented local behavior, not native parity.
use crate::data_processing::DataValue;
use crate::plc::{Area, Device, Memory, Operand};
use crate::wide::WideOperand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const FILE_STORE_FORMAT_VERSION: u32 = 1;
const FORMAT_VERSION: u32 = FILE_STORE_FORMAT_VERSION;
const MAX_IMAGE_BYTES: u64 = 1024 * 1024;
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirmwareVersion {
    pub major: u16,
    pub minor: u8,
}
impl FirmwareVersion {
    pub const fn new(major: u16, minor: u8) -> Self {
        Self { major, minor }
    }
}
impl fmt::Display for FirmwareVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}", self.major, self.minor)
    }
}
impl std::str::FromStr for FirmwareVersion {
    type Err = String;
    fn from_str(raw: &str) -> Result<Self, String> {
        let raw = raw.strip_prefix('V').unwrap_or(raw);
        let (major, minor) = raw
            .split_once('.')
            .ok_or("Firmware x.yy biçiminde olmalı")?;
        if minor.len() != 2 || !minor.bytes().all(|c| c.is_ascii_digit()) {
            return Err("Firmware alt sürümü iki basamaklı olmalı".into());
        }
        Ok(Self {
            major: major.parse().map_err(|_| "Geçersiz firmware sürümü")?,
            minor: minor.parse().map_err(|_| "Geçersiz firmware sürümü")?,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileModel {
    Es2,
    Ex2,
    Sa2,
    Sx2,
    Es2E,
    Ss2,
    Se,
    Se2,
    Es2C,
    Ec5,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileMemoryProfile {
    model: FileModel,
    firmware: FirmwareVersion,
    memory_card_present: bool,
    capacity_words: u16,
    installed_wide_extension: bool,
}
impl FileMemoryProfile {
    pub fn new(
        model: FileModel,
        firmware: FirmwareVersion,
        memory_card_present: bool,
    ) -> Result<Self, String> {
        if firmware.minor > 99 {
            return Err("Geçersiz firmware alt sürümü".into());
        }
        let old = |low: FirmwareVersion, high: FirmwareVersion| firmware >= low && firmware <= high;
        let capacity_words = match model {
            FileModel::Es2 | FileModel::Ex2 => {
                if firmware >= FirmwareVersion::new(3, 46) {
                    8000
                } else if old(FirmwareVersion::new(2, 80), FirmwareVersion::new(3, 44)) {
                    5000
                } else {
                    return Err(
                        "ES2/EX2 dosya register kapasitesi bu firmware için belgelenmedi".into(),
                    );
                }
            }
            FileModel::Sa2 | FileModel::Sx2 => {
                if firmware >= FirmwareVersion::new(3, 2) {
                    8000
                } else if old(FirmwareVersion::new(2, 40), FirmwareVersion::new(3, 0)) {
                    5000
                } else {
                    return Err(
                        "SA2/SX2 dosya register kapasitesi bu firmware için belgelenmedi".into(),
                    );
                }
            }
            FileModel::Es2E => {
                if firmware < FirmwareVersion::new(1, 8) {
                    return Err("ES2-E dosya belleği V1.08 ve sonrası gerektirir".into());
                }
                if !memory_card_present {
                    return Err("ES2-E dosya belleği için açık bellek kartı gerekli".into());
                }
                8000
            }
            FileModel::Se2 => {
                return Err(
                    "SE2 dosya belleği desteği PDF özet ve ayrıntı tablolarında çelişkili".into(),
                );
            }
            _ => return Err("Bu model için seçili PDF dosya register desteği vermiyor".into()),
        };
        Ok(Self {
            model,
            firmware,
            memory_card_present,
            capacity_words,
            installed_wide_extension: false,
        })
    }
    /// Local candidate semantics: m is a FILE WORD offset, n counts width-sized
    /// elements. Wide writes still cannot exceed100 actualWORDS. Not hardware proof.
    pub fn with_installed_wide_extension(mut self) -> Self {
        self.installed_wide_extension = true;
        self
    }
    pub fn model(&self) -> FileModel {
        self.model
    }
    pub fn firmware(&self) -> FirmwareVersion {
        self.firmware
    }
    pub fn memory_card_present(&self) -> bool {
        self.memory_card_present
    }
    pub fn capacity_words(&self) -> u16 {
        self.capacity_words
    }
    pub fn installed_wide_extension(&self) -> bool {
        self.installed_wide_extension
    }
    fn same_hardware(&self, other: &Self) -> bool {
        self.model == other.model
            && self.firmware == other.firmware
            && self.memory_card_present == other.memory_card_present
            && self.capacity_words == other.capacity_words
    }
    fn validate(&self) -> Result<(), String> {
        let mut expected = Self::new(self.model, self.firmware, self.memory_card_present)?;
        expected.installed_wide_extension = self.installed_wide_extension;
        if &expected != self {
            return Err("Dosya belleği profili kapasite tablosuyla uyuşmuyor".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStoreError {
    Missing,
    Io(String),
    Integrity(String),
    UnsupportedVersion(u32),
    IdentityMismatch,
    ProfileMismatch,
    InvalidConfiguration(String),
}
impl fmt::Display for FileStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => write!(f, "Dosya belleği görüntüsü bulunamadı"),
            Self::Io(e) => write!(f, "Dosya belleği G/Ç hatası: {e}"),
            Self::Integrity(e) => write!(f, "Dosya belleği bütünlüğü doğrulanamadı: {e}"),
            Self::UnsupportedVersion(v) => write!(f, "Desteklenmeyen dosya belleği sürümü: {v}"),
            Self::IdentityMismatch => write!(f, "Dosya belleği cihaz kimliği uyuşmuyor"),
            Self::ProfileMismatch => {
                write!(f, "Dosya belleği model/firmware/kart profili uyuşmuyor")
            }
            Self::InvalidConfiguration(e) => {
                write!(f, "Dosya belleği yapılandırması geçersiz: {e}")
            }
        }
    }
}
impl std::error::Error for FileStoreError {}
impl From<std::io::Error> for FileStoreError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            Self::Missing
        } else {
            Self::Io(e.to_string())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStore {
    identity: String,
    profile: FileMemoryProfile,
    words: Vec<u16>,
    generation: u64,
    write_transactions: u64,
    dirty: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImagePayload {
    format_version: u32,
    identity: String,
    profile: FileMemoryProfile,
    words: Vec<u16>,
    generation: u64,
    write_transactions: u64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageEnvelope {
    payload: ImagePayload,
    sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoverySource {
    Primary,
    Previous,
}
#[derive(Debug, Clone)]
pub struct FileLoad {
    pub store: FileStore,
    pub recovery_source: RecoverySource,
}

fn identity_valid(identity: &str) -> bool {
    !identity.trim().is_empty() && identity.len() <= 256 && !identity.chars().any(char::is_control)
}
fn digest(payload: &ImagePayload) -> Result<String, FileStoreError> {
    let bytes =
        serde_json::to_vec(payload).map_err(|e| FileStoreError::Integrity(e.to_string()))?;
    let hash = Sha256::digest(bytes);
    Ok(hash.iter().map(|b| format!("{b:02x}")).collect())
}
fn explicit_path(path: &Path) -> Result<(), FileStoreError> {
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(FileStoreError::InvalidConfiguration(
            "Tam dosya yolu açıkça seçilmeli".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| FileStoreError::InvalidConfiguration("Dosya klasörü yok".into()))?;
    if !fs::metadata(parent)?.is_dir() {
        return Err(FileStoreError::InvalidConfiguration(
            "Hedef klasör değil".into(),
        ));
    }
    if let Ok(meta) = fs::symlink_metadata(path)
        && !meta.file_type().is_file()
    {
        return Err(FileStoreError::InvalidConfiguration(
            "Hedef normal dosya olmalı".into(),
        ));
    }
    Ok(())
}
pub fn previous_image_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".previous");
    PathBuf::from(name)
}
fn read_image(
    path: &Path,
    identity: &str,
    profile: &FileMemoryProfile,
) -> Result<(FileStore, Vec<u8>), FileStoreError> {
    explicit_path(path)?;
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_IMAGE_BYTES {
        return Err(FileStoreError::Integrity(
            "Görüntü 1 MiB sınırını aşıyor".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_IMAGE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(FileStoreError::Integrity("Görüntü boyutu değişti".into()));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| FileStoreError::Integrity(e.to_string()))?;
    if let Some(version) = value
        .get("payload")
        .and_then(|p| p.get("format_version"))
        .and_then(|v| v.as_u64())
        && version != u64::from(FORMAT_VERSION)
    {
        return Err(FileStoreError::UnsupportedVersion(
            version.min(u64::from(u32::MAX)) as u32,
        ));
    }
    let envelope: ImageEnvelope =
        serde_json::from_value(value).map_err(|e| FileStoreError::Integrity(e.to_string()))?;
    let p = envelope.payload;
    if p.format_version != FORMAT_VERSION {
        return Err(FileStoreError::UnsupportedVersion(p.format_version));
    }
    if envelope.sha256 != digest(&p)? {
        return Err(FileStoreError::Integrity("SHA256 özeti uyuşmuyor".into()));
    }
    if p.identity != identity {
        return Err(FileStoreError::IdentityMismatch);
    }
    if !p.profile.same_hardware(profile) {
        return Err(FileStoreError::ProfileMismatch);
    }
    p.profile
        .validate()
        .map_err(FileStoreError::InvalidConfiguration)?;
    if !identity_valid(&p.identity) || p.words.len() != usize::from(p.profile.capacity_words) {
        return Err(FileStoreError::Integrity(
            "Kimlik/kapasite verisi geçersiz".into(),
        ));
    }
    let policy_changed = p.profile.installed_wide_extension != profile.installed_wide_extension;
    // Loading a file never grants an extension: the caller's explicit policy wins.
    Ok((
        FileStore {
            identity: p.identity,
            profile: profile.clone(),
            words: p.words,
            generation: p.generation,
            write_transactions: p.write_transactions,
            dirty: policy_changed,
        },
        bytes,
    ))
}

#[cfg(windows)]
fn replace_file(staging: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let staging: Vec<u16> = staging.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // REPLACE_EXISTING|WRITE_THROUGH, same-directory/same-volume staging.
    if unsafe { MoveFileExW(staging.as_ptr(), target.as_ptr(), 1 | 8) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn replace_file(staging: &Path, target: &Path) -> std::io::Result<()> {
    fs::rename(staging, target)
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), FileStoreError> {
    explicit_path(path)?;
    let parent = path.parent().unwrap();
    let mut chosen = None;
    for _ in 0..16 {
        let serial = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut name = path.file_name().unwrap().to_os_string();
        name.push(format!(".tmp-{}-{serial}", std::process::id()));
        let staging = parent.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
        {
            Ok(file) => {
                chosen = Some((staging, file));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    let (staging, mut file) =
        chosen.ok_or_else(|| FileStoreError::Io("Geçici dosya adı ayrılamadı".into()))?;
    let result = (|| -> Result<(), FileStoreError> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace_file(&staging, path)?;
        #[cfg(not(windows))]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

impl FileStore {
    pub fn new(identity: impl Into<String>, profile: FileMemoryProfile) -> Result<Self, String> {
        profile.validate()?;
        let identity = identity.into();
        if !identity_valid(&identity) {
            return Err("Cihaz kimliği boş/kontrol karakterli veya çok uzun".into());
        }
        Ok(Self {
            identity,
            words: vec![0xffff; usize::from(profile.capacity_words)],
            profile,
            generation: 0,
            write_transactions: 0,
            dirty: true,
        })
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn profile(&self) -> &FileMemoryProfile {
        &self.profile
    }
    pub fn capacity_words(&self) -> u16 {
        self.profile.capacity_words
    }
    pub fn dirty(&self) -> bool {
        self.dirty
    }
    pub fn installed_wide_extension(&self) -> bool {
        self.profile.installed_wide_extension
    }
    pub fn set_installed_wide_extension(&mut self, enabled: bool) {
        if self.profile.installed_wide_extension != enabled {
            self.profile.installed_wide_extension = enabled;
            self.dirty = true;
            self.generation = self.generation.saturating_add(1);
        }
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Diagnostic event count, not a physical endurance/failure simulation.
    pub fn write_transactions(&self) -> u64 {
        self.write_transactions
    }
    pub fn words(&self) -> &[u16] {
        &self.words
    }
    pub fn read_words(&self, start: usize, count: usize) -> Result<&[u16], String> {
        let end = start.checked_add(count).ok_or("Dosya bloğu taştı")?;
        if count == 0 || end > self.words.len() {
            return Err("Dosya register bloğu sınır dışında".into());
        }
        Ok(&self.words[start..end])
    }
    pub fn write_words(&mut self, start: usize, values: &[u16]) -> Result<(), String> {
        if values.is_empty() || values.len() > 100 {
            return Err("Dosya yazma işlemi 1..100 gerçek WORD olmalı".into());
        }
        let end = start.checked_add(values.len()).ok_or("Dosya bloğu taştı")?;
        if end > self.words.len() {
            return Err("Dosya register bloğu sınır dışında".into());
        }
        self.words[start..end].copy_from_slice(values);
        self.generation = self.generation.saturating_add(1);
        self.write_transactions = self.write_transactions.saturating_add(1);
        self.dirty = true;
        Ok(())
    }
    /// Explicit local backend erase; no PLC erase instruction or flash wear is invented.
    pub fn erase(&mut self) {
        self.words.fill(0xffff);
        self.generation = self.generation.saturating_add(1);
        self.dirty = true;
    }
    fn payload(&self) -> ImagePayload {
        ImagePayload {
            format_version: FORMAT_VERSION,
            identity: self.identity.clone(),
            profile: self.profile.clone(),
            words: self.words.clone(),
            generation: self.generation,
            write_transactions: self.write_transactions,
        }
    }
    pub fn save(&mut self, path: &Path) -> Result<(), FileStoreError> {
        self.profile
            .validate()
            .map_err(FileStoreError::InvalidConfiguration)?;
        explicit_path(path)?;
        let payload = self.payload();
        let sha256 = digest(&payload)?;
        let bytes = serde_json::to_vec_pretty(&ImageEnvelope { payload, sha256 })
            .map_err(|e| FileStoreError::Integrity(e.to_string()))?;
        // Preserve the prior validated primary BEFORE committing its replacement.
        match read_image(path, &self.identity, &self.profile) {
            Ok((_, previous)) => atomic_write(&previous_image_path(path), &previous)?,
            Err(FileStoreError::Missing) => {}
            Err(e) => return Err(e),
        }
        atomic_write(path, &bytes)?;
        self.dirty = false;
        Ok(())
    }
    pub fn load_with_recovery(
        path: &Path,
        identity: &str,
        profile: &FileMemoryProfile,
    ) -> Result<FileLoad, FileStoreError> {
        match read_image(path, identity, profile) {
            Ok((store, _)) => Ok(FileLoad {
                store,
                recovery_source: RecoverySource::Primary,
            }),
            Err(primary @ (FileStoreError::Missing | FileStoreError::Integrity(_))) => {
                match read_image(&previous_image_path(path), identity, profile) {
                    Ok((store, _)) => Ok(FileLoad {
                        store,
                        recovery_source: RecoverySource::Previous,
                    }),
                    Err(FileStoreError::Missing) => Err(primary),
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }
    /// Explicitly restore a validated previous image. A valid primary belonging
    /// to another identity/profile/newer version is never silently overwritten.
    pub fn recover_previous(
        path: &Path,
        identity: &str,
        profile: &FileMemoryProfile,
    ) -> Result<Self, FileStoreError> {
        match read_image(path, identity, profile) {
            Ok(_) | Err(FileStoreError::Missing | FileStoreError::Integrity(_)) => {}
            Err(e) => return Err(e),
        }
        let (store, bytes) = read_image(&previous_image_path(path), identity, profile)?;
        atomic_write(path, &bytes)?;
        Ok(store)
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileMemoryState {
    store: Option<FileStore>,
    activation: BTreeMap<usize, bool>,
}
impl FileMemoryState {
    /// Replacing/configuring storage does not synthesize a new contact edge.
    /// Compile/reset explicitly call reset_instruction_state when appropriate.
    pub fn configure(&mut self, store: FileStore) {
        self.store = Some(store);
    }
    pub fn store(&self) -> Option<&FileStore> {
        self.store.as_ref()
    }
    pub fn store_mut(&mut self) -> Option<&mut FileStore> {
        self.store.as_mut()
    }
    pub fn take_store(&mut self) -> Option<FileStore> {
        self.reset_instruction_state();
        self.store.take()
    }
    pub fn reset_instruction_state(&mut self) {
        self.activation.clear();
    }
    pub fn save(&mut self, path: &Path) -> Result<(), FileStoreError> {
        self.store
            .as_mut()
            .ok_or_else(|| {
                FileStoreError::InvalidConfiguration("Dosya belleği yapılandırılmadı".into())
            })?
            .save(path)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileAction {
    Read {
        wide: bool,
        index: DataValue,
        dest: Device,
        count: DataValue,
    },
    Write {
        wide: bool,
        source: Device,
        index: DataValue,
        count: DataValue,
    },
}
fn data_operand(raw: &str) -> Result<Device, String> {
    let d: Device = raw.parse()?;
    if d.area != Area::D || !(2000..=9999).contains(&d.index) {
        return Err("MEMR/MEMW veri bloğu D2000..D9999 içinde olmalı".into());
    }
    Ok(d)
}
fn number(raw: &str, wide: bool) -> Result<DataValue, String> {
    let value = DataValue::parse(raw, wide)?;
    if matches!(&value,DataValue::Word(Operand::Register(d))|DataValue::Double(WideOperand::Register(d)) if d.area!=Area::D)
    {
        return Err("Dosya indeksi ve adet K/H/D olmalı".into());
    }
    Ok(value)
}
fn number_base(value: &DataValue) -> Option<Device> {
    match value {
        DataValue::Word(Operand::Register(d)) | DataValue::Double(WideOperand::Register(d)) => {
            Some(*d)
        }
        _ => None,
    }
}
fn scalar_fits(value: &DataValue, m: &Memory) -> bool {
    number_base(value).is_none_or(|d| {
        d.index
            .checked_add(u16::from(value.wide()))
            .is_some_and(|end| end <= m.data_register_limit.unwrap_or(9999))
    })
}
impl FileAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !matches!(name, "MEMR" | "MEMW" | "DMEMR" | "DMEMW") {
            return Err("Bilinmeyen dosya belleği komutu".into());
        }
        if args.len() != 3 {
            return Err(format!("{name}: üç operand gerekli"));
        }
        let wide = name.starts_with('D');
        if name.ends_with('R') {
            Ok(Self::Read {
                wide,
                index: number(&args[0], wide)?,
                dest: data_operand(&args[1])?,
                count: number(&args[2], wide)?,
            })
        } else {
            Ok(Self::Write {
                wide,
                source: data_operand(&args[0])?,
                index: number(&args[1], wide)?,
                count: number(&args[2], wide)?,
            })
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Read { wide: false, .. } => "MEMR",
            Self::Read { wide: true, .. } => "DMEMR",
            Self::Write { wide: false, .. } => "MEMW",
            Self::Write { wide: true, .. } => "DMEMW",
        }
    }
    pub fn wide(&self) -> bool {
        match self {
            Self::Read { wide, .. } | Self::Write { wide, .. } => *wide,
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Read { wide: false, .. } => 0x4801,
            Self::Read { wide: true, .. } => 0x4901,
            Self::Write { wide: false, .. } => 0x4a01,
            Self::Write { wide: true, .. } => 0x4b01,
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Read { dest, .. } => *dest,
            Self::Write { source, .. } => *source,
        }
    }
    pub fn label(&self) -> String {
        let name = self.name();
        match self {
            Self::Read {
                index, dest, count, ..
            } => format!("{name} {index} {dest} {count}"),
            Self::Write {
                source,
                index,
                count,
                ..
            } => format!("{name} {source} {index} {count}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        let (index, count) = self.scalars();
        // Runtime tracking adds a wide scalar's high WORD only when model-valid.
        out.extend(number_base(index));
        out.extend(number_base(count));
        out.insert(self.dest());
        Memory::error_devices(out);
    }
    fn scalars(&self) -> (&DataValue, &DataValue) {
        match self {
            Self::Read { index, count, .. } | Self::Write { index, count, .. } => (index, count),
        }
    }
    fn word_count(&self, m: &Memory) -> Option<usize> {
        let (_, count) = self.scalars();
        let count = usize::try_from(count.read(m)).ok()?;
        if count == 0 {
            return None;
        }
        count.checked_mul(if self.wide() { 2 } else { 1 })
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        let limit = m.data_register_limit.unwrap_or(9999);
        for value in [self.scalars().0, self.scalars().1] {
            if value.wide()
                && let Some(d) = number_base(value)
                && d.index.checked_add(1).is_some_and(|end| end <= limit)
            {
                out.insert(Device {
                    area: Area::D,
                    index: d.index + 1,
                });
            }
        }
        if let Some(words) = self.word_count(m).filter(|words| *words <= 8000) {
            let base = self.dest();
            for n in 0..words {
                if let Some(index) = base
                    .index
                    .checked_add(n as u16)
                    .filter(|index| *index <= limit)
                {
                    out.insert(Device {
                        area: Area::D,
                        index,
                    });
                }
            }
        }
    }
    /// Local policy: all scalar operands are sampled once; complete transfer
    /// validated before an atomic snapshot commit. Installed bodies are stubs.
    /// A MEMW activation is consumed on its first ATTEMPT, including failures.
    pub fn execute(&self, m: &mut Memory, power: bool, instance: usize) {
        if matches!(self, Self::Write { .. }) {
            let was = m
                .file_memory
                .activation
                .insert(instance, power)
                .unwrap_or(false);
            if !power || was {
                return;
            }
        } else if !power {
            return;
        }
        let Some(store) = m.file_memory.store.as_ref() else {
            m.execution_limitations.insert(
                "MEMR/MEMW: cihaz kimliği ve saklanan dosya belleği profili yapılandırılmadı"
                    .into(),
            );
            return;
        };
        if self.wide() && !store.profile.installed_wide_extension {
            m.execution_limitations.insert("DMEMR/DMEMW: PDF 32 bit biçimi dışlıyor; yerel aday uzantı açıkça etkinleştirilmeli".into());
            return;
        }
        let capacity = store.words.len();
        let (index, count) = self.scalars();
        if !scalar_fits(index, m) || !scalar_fits(count, m) {
            m.operation_error(0x0e1a, 0);
            return;
        }
        let index_value = index.read(m);
        let count_value = count.read(m);
        let Some(start) = usize::try_from(index_value).ok() else {
            m.operation_error(0x0e1a, 0);
            return;
        };
        let Some(count) = usize::try_from(count_value).ok().filter(|v| *v > 0) else {
            m.operation_error(0x0e1a, 0);
            return;
        };
        let Some(words) = count.checked_mul(if self.wide() { 2 } else { 1 }) else {
            m.operation_error(0x0e1a, 0);
            return;
        };
        let base = self.dest();
        let valid_file = start.checked_add(words).is_some_and(|end| end <= capacity);
        let valid_data = base.area == Area::D
            && base.index >= 2000
            && words > 0
            && usize::from(base.index)
                .checked_add(words - 1)
                .is_some_and(|end| end <= usize::from(m.data_register_limit.unwrap_or(9999)));
        let valid_size = words <= capacity && (!matches!(self, Self::Write { .. }) || words <= 100);
        if !valid_file || !valid_data || !valid_size {
            m.operation_error(0x0e1a, 0);
            return;
        }
        match self {
            Self::Read { .. } => {
                let snapshot =
                    m.file_memory.store.as_ref().unwrap().words[start..start + words].to_vec();
                for (n, value) in snapshot.into_iter().enumerate() {
                    m.set_word(
                        Device {
                            area: Area::D,
                            index: base.index + n as u16,
                        },
                        value as i16,
                    );
                }
            }
            Self::Write { .. } => {
                let snapshot: Vec<u16> = (0..words)
                    .map(|n| {
                        m.word(Device {
                            area: Area::D,
                            index: base.index + n as u16,
                        }) as u16
                    })
                    .collect();
                // All backend validation has already succeeded. No partial transaction.
                if m.file_memory
                    .store
                    .as_mut()
                    .unwrap()
                    .write_words(start, &snapshot)
                    .is_err()
                {
                    m.operation_error(0x0e1a, 0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(index: u16) -> Device {
        Device {
            area: Area::D,
            index,
        }
    }
    fn profile() -> FileMemoryProfile {
        FileMemoryProfile::new(FileModel::Es2, FirmwareVersion::new(3, 44), false).unwrap()
    }
    fn store() -> FileStore {
        FileStore::new("fixture-plc", profile()).unwrap()
    }
    fn memory() -> Memory {
        let mut m = Memory::default();
        m.file_memory.configure(store());
        m
    }
    fn action(text: &str) -> FileAction {
        let parts: Vec<String> = text.split_whitespace().map(str::to_string).collect();
        FileAction::parse(&parts[0], &parts[1..]).unwrap()
    }
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            for _ in 0..32 {
                let serial = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
                let dir = std::env::temp_dir().join(format!(
                    "opendvp-file-memory-{}-{serial}",
                    std::process::id()
                ));
                match fs::create_dir(&dir) {
                    Ok(()) => return Self(dir),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("{e}"),
                }
            }
            panic!("test directory unavailable")
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            // No recursive deletion: remove only files in the directory this
            // fixture itself created, then the now-empty directory.
            if let Ok(entries) = fs::read_dir(&self.0) {
                for e in entries.flatten() {
                    let path = e.path();
                    if path.parent() == Some(self.0.as_path())
                        && e.file_type().is_ok_and(|t| t.is_file())
                    {
                        let _ = fs::remove_file(path);
                    }
                }
            }
            let _ = fs::remove_dir(&self.0);
        }
    }
    #[test]
    fn profiles_preserve_firmware_capacity_card_and_model_qualification() {
        assert_eq!(profile().capacity_words(), 5000);
        assert_eq!(
            FileMemoryProfile::new(FileModel::Es2, FirmwareVersion::new(3, 46), false)
                .unwrap()
                .capacity_words(),
            8000
        );
        assert!(
            FileMemoryProfile::new(FileModel::Es2, FirmwareVersion::new(3, 45), false).is_err()
        );
        assert_eq!(
            FileMemoryProfile::new(FileModel::Sa2, FirmwareVersion::new(3, 2), false)
                .unwrap()
                .capacity_words(),
            8000
        );
        assert!(FileMemoryProfile::new(FileModel::Sa2, FirmwareVersion::new(3, 1), false).is_err());
        assert!(
            FileMemoryProfile::new(FileModel::Es2E, FirmwareVersion::new(1, 8), false).is_err()
        );
        assert_eq!(
            FileMemoryProfile::new(FileModel::Es2E, FirmwareVersion::new(1, 8), true)
                .unwrap()
                .capacity_words(),
            8000
        );
        for model in [
            FileModel::Ss2,
            FileModel::Se,
            FileModel::Se2,
            FileModel::Es2C,
            FileModel::Ec5,
        ] {
            assert!(FileMemoryProfile::new(model, FirmwareVersion::new(9, 0), true).is_err());
        }
        assert_eq!(
            "V3.46".parse::<FirmwareVersion>().unwrap(),
            FirmwareVersion::new(3, 46)
        );
        assert!("3.4".parse::<FirmwareVersion>().is_err());
    }
    #[test]
    fn parser_preserves_restricted_matrices_and_real_installed_headers() {
        for (source, header) in [
            ("MEMR K0 D2000 K1", 0x4801),
            ("DMEMR H0 D2000 D10", 0x4901),
            ("MEMW D9999 K0 H1", 0x4a01),
            ("DMEMW D2000 D10 K1", 0x4b01),
        ] {
            assert_eq!(action(source).header(), header);
        }
        for text in [
            "MEMR T0 D2000 K1",
            "MEMR K4M0 D2000 K1",
            "MEMR K0 D1999 K1",
            "MEMR K0 C0 K1",
            "MEMW T0 K0 K1",
            "MEMW D2000 K0 C0",
            "MEMW D2000 E0 K1",
        ] {
            let p: Vec<String> = text.split_whitespace().map(str::to_string).collect();
            assert!(FileAction::parse(&p[0], &p[1..]).is_err(), "{text}");
        }
    }
    #[test]
    fn unconfigured_backend_holds_and_erased_store_reads_ffff_without_flag_changes() {
        let mut m = Memory::default();
        m.set_word(d(2000), 123);
        action("MEMR K0 D2000 K2").execute(&mut m, true, 1);
        assert_eq!(m.word(d(2000)), 123);
        assert!(!m.execution_limitations.is_empty());
        m.file_memory.configure(store());
        m.execution_limitations.clear();
        for bit in [1020, 1021, 1022, 1029, 1101] {
            m.set_system(bit, true);
        }
        action("MEMR K0 D2000 K2").execute(&mut m, true, 1);
        assert_eq!((m.word(d(2000)), m.word(d(2001))), (-1, -1));
        assert!(!m.system_bit(1067));
        for bit in [1020, 1021, 1022, 1029, 1101] {
            assert!(m.system_bit(bit));
        }
    }
    #[test]
    fn ordinary_write_is_once_per_activation_and_failed_edge_does_not_retry_while_held() {
        let mut m = memory();
        let write = action("MEMW D2000 D10 K1");
        m.set_word(d(10), 0);
        m.set_word(d(2000), 123);
        write.execute(&mut m, true, 1);
        m.set_word(d(2000), 456);
        write.execute(&mut m, true, 1);
        assert_eq!(m.file_memory.store().unwrap().words()[0], 123);
        write.execute(&mut m, false, 1);
        write.execute(&mut m, true, 1);
        assert_eq!(m.file_memory.store().unwrap().words()[0], 456);
        m.set_word(d(10), -1);
        write.execute(&mut m, true, 2);
        assert!(m.system_bit(1067));
        m.set_system(1067, false);
        m.set_system(1068, false);
        m.set_word(d(10), 2);
        write.execute(&mut m, true, 2);
        assert_eq!(m.file_memory.store().unwrap().words()[2], 0xffff);
        assert!(!m.system_bit(1067));
        write.execute(&mut m, false, 2);
        write.execute(&mut m, true, 2);
        assert_eq!(m.file_memory.store().unwrap().words()[2], 456);
        assert_eq!(m.file_memory.store().unwrap().write_transactions(), 3);
    }
    #[test]
    fn scalar_aliases_are_sampled_before_atomic_read_and_all_combined_bounds_hold() {
        let mut m = memory();
        m.file_memory
            .store_mut()
            .unwrap()
            .write_words(0, &[2, 100, 101])
            .unwrap();
        m.set_word(d(2000), 0);
        m.set_word(d(2001), 3);
        action("MEMR D2000 D2000 D2001").execute(&mut m, true, 0);
        assert_eq!(
            (0..3).map(|n| m.word(d(2000 + n))).collect::<Vec<_>>(),
            [2, 100, 101]
        );
        m.set_word(d(9999), 123);
        action("MEMR K0 D9999 K2").execute(&mut m, true, 0);
        assert_eq!(m.word(d(9999)), 123);
        assert_eq!(m.word(d(1067)), 0x0e1a);
        let before = m.file_memory.store().unwrap().words().to_vec();
        action("MEMW D2000 K4999 K2").execute(&mut m, true, 9);
        assert_eq!(m.file_memory.store().unwrap().words(), before);
        m.set_word(d(2000), 789);
        action("MEMR K4999 D2000 K2").execute(&mut m, true, 0);
        assert_eq!(m.word(d(2000)), 789);
    }
    #[test]
    fn write_and_scalar_model_spans_are_checked_before_any_transfer() {
        let mut m = memory();
        m.data_register_limit = Some(4999);
        m.set_word(d(4999), 77);
        action("MEMR K0 D4999 K2").execute(&mut m, true, 0);
        assert_eq!(m.word(d(4999)), 77);
        m.file_memory
            .store_mut()
            .unwrap()
            .set_installed_wide_extension(true);
        let wide = action("DMEMR D4999 D2000 K1");
        m.set_word(d(2000), 123);
        wide.execute(&mut m, true, 0);
        assert_eq!(m.word(d(2000)), 123);
        assert!(m.system_bit(1067));
        assert_eq!(m.file_memory.store().unwrap().write_transactions(), 0);
    }
    #[test]
    fn wide_extension_is_explicit_with_word_offset_element_count_and_actual_word_cap() {
        let mut m = memory();
        let write = action("DMEMW D2000 K1 K2");
        for (n, value) in [0x1111, 0x2222, 0x3333, 0x4444].into_iter().enumerate() {
            m.set_word(d(2000 + n as u16), value);
        }
        write.execute(&mut m, true, 0);
        assert!(!m.execution_limitations.is_empty());
        assert_eq!(m.file_memory.store().unwrap().write_transactions(), 0);
        m.file_memory
            .store_mut()
            .unwrap()
            .set_installed_wide_extension(true);
        write.execute(&mut m, false, 0);
        write.execute(&mut m, true, 0);
        assert_eq!(
            &m.file_memory.store().unwrap().words()[..5],
            &[0xffff, 0x1111, 0x2222, 0x3333, 0x4444]
        );
        action("DMEMR K1 D2010 K2").execute(&mut m, true, 1);
        assert_eq!(m.double_word(d(2010)) as u32, 0x22221111);
        action("DMEMW D2000 K10 K51").execute(&mut m, true, 2);
        assert_eq!(m.file_memory.store().unwrap().words()[10], 0xffff);
        assert!(m.system_bit(1067));
    }
    #[test]
    fn instruction_reset_and_take_preserve_separate_retained_store_and_dirty_state() {
        let mut m = memory();
        m.set_word(d(2000), 9);
        let write = action("MEMW D2000 K0 K1");
        write.execute(&mut m, true, 1);
        m.file_memory.reset_instruction_state();
        assert_eq!(m.file_memory.store().unwrap().words()[0], 9);
        assert!(m.file_memory.store().unwrap().dirty());
        m.set_word(d(2000), 10);
        write.execute(&mut m, true, 1);
        assert_eq!(m.file_memory.store().unwrap().words()[0], 10);
        let mut retained = m.file_memory.take_store().unwrap();
        assert!(m.file_memory.store().is_none());
        assert_eq!(retained.identity(), "fixture-plc");
        retained.erase();
        assert!(retained.words().iter().all(|v| *v == 0xffff));
    }
    #[test]
    fn actual_disk_roundtrip_replaces_existing_image_and_restores_a_corrupt_primary() {
        let temp = Temp::new();
        let path = temp.path("explicit-store.json");
        let mut s = store();
        s.write_words(0, &[1]).unwrap();
        s.save(&path).unwrap();
        assert!(!s.dirty());
        s.write_words(1, &[2]).unwrap();
        s.save(&path).unwrap();
        let loaded = FileStore::load_with_recovery(&path, "fixture-plc", &profile()).unwrap();
        assert_eq!(loaded.recovery_source, RecoverySource::Primary);
        assert_eq!(&loaded.store.words()[..2], &[1, 2]);
        fs::write(&path, b"interrupted image").unwrap();
        let recovered = FileStore::load_with_recovery(&path, "fixture-plc", &profile()).unwrap();
        assert_eq!(recovered.recovery_source, RecoverySource::Previous);
        assert_eq!(&recovered.store.words()[..2], &[1, 0xffff]);
        assert_eq!(fs::read(&path).unwrap(), b"interrupted image");
        s.write_words(2, &[3]).unwrap();
        assert!(s.save(&path).is_err());
        assert!(s.dirty());
        FileStore::recover_previous(&path, "fixture-plc", &profile()).unwrap();
        assert_eq!(
            FileStore::load_with_recovery(&path, "fixture-plc", &profile())
                .unwrap()
                .recovery_source,
            RecoverySource::Primary
        );
        s.save(&path).unwrap();
        assert_eq!(
            &FileStore::load_with_recovery(&path, "fixture-plc", &profile())
                .unwrap()
                .store
                .words()[..3],
            &[1, 2, 3]
        );
        assert!(
            fs::read_dir(&temp.0).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-"))
        );
    }
    #[test]
    fn images_enforce_integrity_identity_profile_version_and_do_not_auto_enable_wide() {
        let temp = Temp::new();
        let path = temp.path("store.json");
        let mut s = store();
        s.save(&path).unwrap();
        let original = fs::read(&path).unwrap();
        assert!(matches!(
            FileStore::load_with_recovery(&path, "other-device", &profile()),
            Err(FileStoreError::IdentityMismatch)
        ));
        let newer =
            FileMemoryProfile::new(FileModel::Es2, FirmwareVersion::new(3, 46), false).unwrap();
        assert!(matches!(
            FileStore::load_with_recovery(&path, "fixture-plc", &newer),
            Err(FileStoreError::ProfileMismatch)
        ));
        let mut tampered: serde_json::Value = serde_json::from_slice(&original).unwrap();
        tampered["payload"]["words"][0] = serde_json::json!(7);
        fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(matches!(
            FileStore::load_with_recovery(&path, "fixture-plc", &profile()),
            Err(FileStoreError::Integrity(_))
        ));
        tampered["payload"]["format_version"] = serde_json::json!(2);
        fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(matches!(
            FileStore::load_with_recovery(&path, "fixture-plc", &profile()),
            Err(FileStoreError::UnsupportedVersion(2))
        ));
        fs::write(&path, &original).unwrap();
        s.set_installed_wide_extension(true);
        s.save(&path).unwrap();
        let loaded = FileStore::load_with_recovery(&path, "fixture-plc", &profile()).unwrap();
        assert!(!loaded.store.installed_wide_extension());
        assert!(loaded.store.dirty());
        let wide = profile().with_installed_wide_extension();
        assert!(
            FileStore::load_with_recovery(&path, "fixture-plc", &wide)
                .unwrap()
                .store
                .installed_wide_extension()
        );
    }
    #[test]
    fn a_valid_foreign_primary_is_not_masked_by_an_identity_matching_previous_image() {
        let temp = Temp::new();
        let path = temp.path("selected.json");
        let other = temp.path("other.json");
        let mut s = store();
        s.save(&path).unwrap();
        s.write_words(0, &[1]).unwrap();
        s.save(&path).unwrap();
        let mut foreign = FileStore::new("another-plc", profile()).unwrap();
        foreign.save(&other).unwrap();
        fs::copy(&other, &path).unwrap();
        assert!(matches!(
            FileStore::load_with_recovery(&path, "fixture-plc", &profile()),
            Err(FileStoreError::IdentityMismatch)
        ));
        assert!(matches!(
            FileStore::recover_previous(&path, "fixture-plc", &profile()),
            Err(FileStoreError::IdentityMismatch)
        ));
        assert_eq!(fs::read(&path).unwrap(), fs::read(&other).unwrap());
    }
}
