//! Program download to a Delta DVP PLC.
//!
//! The request order mirrors Delta's DVPtool/EasyDownload (Delta editor `PLC_CARD`), which was
//! decompiled and then captured end-to-end against the Delta editor ES2 simulator; see
//! Only the executable program image is
//! written: no source/comment area, registers, file registers, PLC ID, password or timestamp.
use std::time::Duration;

pub const PROGRAM_END: u16 = 0x0F00;
/// M1072: written 0 (STOP) before the transfer and FF00 (RUN) on request.
const RUN_COIL: u16 = 0x0C30;
/// M1073: DVPtool sets it after the image is written, then waits about one second.
const COMMIT_COIL: u16 = 0x0C31;
/// D1002: program capacity in steps.
const CAPACITY_REGISTER: u16 = 0x13EA;
/// Two words: executable size and source size; `FFFF FFFF` when no source is sent.
const SOURCE_HEADER: u16 = 0x1580;
/// Above this capacity DVPtool switches from FC03/FC10 banks to FC74/FC75.
const BANKED_CAPACITY_LIMIT: u16 = 0x3E00;
const BANK_LOW: u16 = 0x2000;
const BANK_HIGH: u16 = 0x5000;
const BANK_SPLIT: usize = 0x2000;
const BANK_CHUNK: usize = 16;
const LINEAR_CHUNK: usize = 99;
const COMMIT_SETTLE: Duration = Duration::from_millis(1000);

/// FC67 word 3 (when word 2 is 1) as mapped by DVPtool's model table.
const MODEL_CODES: &[(u16, &str)] = &[
    (0x00, "ES/EC"),
    (0x01, "EX"),
    (0x02, "SS"),
    (0x03, "ES/EC"),
    (0x04, "EH"),
    (0x05, "EP"),
    (0x06, "SA"),
    (0x07, "SC"),
    (0x08, "SX"),
    (0x09, "SV"),
    (0x0A, "EH2-L"),
    (0x0B, "EH2"),
    (0x0C, "ES2"),
    (0x0D, "EX2"),
    (0x0E, "SS2"),
    (0x0F, "SX2"),
    (0x10, "SV2"),
    (0x11, "EH3-L"),
    (0x12, "EH3"),
    (0x13, "SA2"),
    (0x14, "MC"),
    (0x15, "SE"),
    (0x17, "ES2-E"),
    (0x18, "EC3-8K"),
    (0x19, "EC5"),
    (0x1A, "SE2"),
    (0x1C, "ES2-64K"),
    (0x1D, "EX2-64K"),
];

/// Modbus exception code 03 (illegal data value), used by the simulators for FC64
/// sub-commands they do not implement.
const UNSUPPORTED: u8 = 0x03;

pub trait PduLink {
    /// Send one request (`function` + `data`, station and framing added by the link) and
    /// return the reply data after the function byte, or the Modbus exception code.
    fn transact(&mut self, function: u8, data: &[u8]) -> Result<Result<Vec<u8>, u8>, String>;
    fn wait(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

impl PduLink for crate::es2_protocol::Client {
    fn transact(&mut self, function: u8, data: &[u8]) -> Result<Result<Vec<u8>, u8>, String> {
        self.request_reply(function, data)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    /// Delta simulators: FC64 password/ID queries are not implemented there.
    Simulator,
    /// A real PLC: every safety query must be answered.
    #[default]
    Plc,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Options {
    pub target: Target,
    pub run_after: bool,
    /// 4-character PLC password, sent once with FC64 1/0D when the PLC is locked (older
    /// ES/EX/SS: compared with D1013 before M1069 is cleared). Never retried automatically:
    /// the PLC counts failed attempts and locks.
    pub password: Option<String>,
    /// PLC ID the project expects; required when the PLC reports an ID.
    pub plc_id: Option<String>,
    /// Values written after the program is verified and before RUN (Delta editor's "Initial
    /// Value" option). The PLC does not reload them on its own at the next STOP→RUN.
    pub initial: Vec<Initial>,
}
/// One initial value, already mapped to its Modbus address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Initial {
    Bit { address: u16, on: bool },
    Words { address: u16, words: Vec<u16> },
}

/// Password status byte (FC64 1/0C): 01 = none, 5A = set. Anything else is refused.
const PASSWORD_NONE: u8 = 0x01;
const PASSWORD_SET: u8 = 0x5A;
/// FC03 block holding the PLC ID: word 0 = length, words 1.. = one character each.
const PLC_ID_REGISTER: u16 = 0x6E0A;
const PLC_ID_WORDS: usize = 9;
/// Older ES/EX/SS/EC series (DVPtool series group 0) have no FC64 commands. DVPtool reads
/// M1069 for the password state, reads the password from D1013 (2 words), compares it with
/// the entered one locally and clears M1069 to unlock (`routine`, `routine`).
const LEGACY_MODELS: [&str; 3] = ["ES/EC", "EX", "SS"];
const LEGACY_PASSWORD_COIL: u16 = 0x0C2D;
const LEGACY_PASSWORD_REGISTER: u16 = 0x13F5;
/// DVPtool polls FC64 1/1A after M1073 only on the 64K-step models (flag `+0x620`, set
/// for 0x1F400-byte capacity in `routine`).
const COMPLETION_POLL_CAPACITY: u16 = 0xFA00;
const COMPLETION_POLL_INTERVAL: Duration = Duration::from_millis(100);
const COMPLETION_POLL_LIMIT: usize = 600;
/// M1004: syntax check error, read by DVPtool when 1/1A reports a failure.
const SYNTAX_ERROR_COIL: u16 = 0x0BEC;

fn is_legacy(identity: &Identity) -> bool {
    match identity.model {
        Some(model) => LEGACY_MODELS.contains(&model),
        // The old series predate FC67.
        None => identity.info.is_none(),
    }
}

pub(crate) fn read_coil(link: &mut dyn PduLink, address: u16, what: &str) -> Result<bool, String> {
    let mut request = address.to_be_bytes().to_vec();
    request.extend(1u16.to_be_bytes());
    let reply = data(link, 0x01, &request, what)?;
    match reply.as_slice() {
        [1, bits] => Ok(bits & 1 != 0),
        _ => Err(format!("{what}: beklenmeyen yanıt uzunluğu")),
    }
}

fn unlock_legacy(link: &mut dyn PduLink, password: Option<[u8; 4]>) -> Result<(), String> {
    if !read_coil(link, LEGACY_PASSWORD_COIL, "Parola durumu (M1069)")? {
        return Ok(());
    }
    let password =
        password.ok_or("PLC parola korumalı; indirmek için 4 karakterlik PLC parolasını girin")?;
    let stored = read_words(
        link,
        0x03,
        LEGACY_PASSWORD_REGISTER,
        2,
        "PLC parolası (D1013)",
    )?;
    // DVPtool compares the reply data bytes in order (memcmp at 0x42cb9d): high byte first.
    let stored: Vec<u8> = stored.iter().flat_map(|w| w.to_be_bytes()).collect();
    if stored != password {
        return Err("PLC parolası eşleşmedi; PLC'ye kilit açma komutu gönderilmedi".into());
    }
    write_coil(
        link,
        LEGACY_PASSWORD_COIL,
        false,
        "Parola kilidini açma (M1069)",
    )
}

/// FC64 1/1A after M1073: a 5-byte payload (two big-endian progress counters in bytes 1..5)
/// while the PLC works, `00 5A` when done; any other 2-byte payload is a failure.
fn wait_for_completion(link: &mut dyn PduLink) -> Result<(), String> {
    for _ in 0..COMPLETION_POLL_LIMIT {
        match fc64(link, 0x1A, &[0x00], "Program tamamlama (FC64 1/1A)")? {
            Ok(reply) if reply.len() == 5 => link.wait(COMPLETION_POLL_INTERVAL),
            Ok(reply) if reply == [0x00, 0x5A] => return Ok(()),
            Ok(reply) if reply.len() == 2 => {
                let syntax = read_coil(link, SYNTAX_ERROR_COIL, "Sözdizimi hatası (M1004)")?;
                return Err(if syntax {
                    "PLC programı reddetti: sözdizimi denetimi hatası (M1004)".into()
                } else {
                    format!("PLC programı kabul etmedi (1/1A durumu {reply:02x?})")
                });
            }
            Ok(reply) => return Err(format!("1/1A yanıtı tanınmadı: {reply:02x?}")),
            Err(code) => {
                return Err(format!(
                    "Program tamamlama (FC64 1/1A): PLC Modbus istisnası {code:#04x}"
                ));
            }
        }
    }
    Err("PLC program kabulünü zaman aşımında bitirmedi (FC64 1/1A)".into())
}

pub fn validate_password(password: &str) -> Result<[u8; 4], String> {
    let bytes: [u8; 4] = password
        .as_bytes()
        .try_into()
        .map_err(|_| "PLC parolası tam 4 karakter olmalı".to_owned())?;
    if !bytes.iter().all(u8::is_ascii_graphic) {
        return Err("PLC parolası yalnız ASCII harf, rakam veya işaret içerebilir".into());
    }
    Ok(bytes)
}

/// Decode the FC03 0x6E0A block: length word, then one character per word (low byte).
fn decode_plc_id(words: &[u16]) -> Result<String, String> {
    let length = usize::from(*words.first().ok_or("PLC kimliği okunamadı")?);
    if length > words.len() - 1 {
        return Err("PLC kimliği uzunluğu geçersiz".into());
    }
    Ok(words[1..=length]
        .iter()
        .map(|w| char::from(*w as u8))
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// FC67 words from 0x0010 (absent when the device rejects FC67).
    pub info: Option<[u16; 5]>,
    pub model: Option<&'static str>,
    pub capacity_steps: u16,
    pub slave_id: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub identity: Identity,
    pub words_written: usize,
    pub running: bool,
    /// Initial values written after the program.
    pub initial_written: usize,
    /// Safety queries the target rejected as unsupported (simulator only).
    pub unsupported_checks: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Identify,
    Stop,
    Checks,
    Program { done: usize, total: usize },
    Commit,
    Verify { done: usize, total: usize },
    Initial { done: usize, total: usize },
    Run,
}

/// FC67 word 3 reported by the Delta editor/COMMGR simulators instead of a PLC model code
/// (observed against each installed simulator).
pub fn simulator_name(info: &[u16; 5]) -> Option<&'static str> {
    if info[2] != 1 {
        return None;
    }
    Some(match info[3] {
        0x50 => "EH2",
        0x51 => "EH3",
        0x52 => "ES2",
        0x53 => "SE",
        0x54 => "SS2",
        0x57 => "SE2",
        _ => return None,
    })
}

pub fn model_name(info: &[u16; 5]) -> Option<&'static str> {
    if info[2] != 1 {
        return None;
    }
    MODEL_CODES
        .iter()
        .find(|(code, _)| *code == info[3])
        .map(|(_, name)| *name)
}

/// Word address and chunk size used for program word `index` on a PLC of `capacity` steps.
fn program_layout(capacity: u16, index: usize) -> (u8, u8, u16, usize) {
    if capacity <= BANKED_CAPACITY_LIMIT {
        let address = if index < BANK_SPLIT {
            BANK_LOW + index as u16
        } else {
            BANK_HIGH + (index - BANK_SPLIT) as u16
        };
        // A bank chunk never crosses the 0x2000 split.
        let room = if index < BANK_SPLIT {
            BANK_SPLIT - index
        } else {
            usize::MAX
        };
        (0x03, 0x10, address, BANK_CHUNK.min(room))
    } else {
        (0x74, 0x75, index as u16, LINEAR_CHUNK)
    }
}

fn data(
    link: &mut dyn PduLink,
    function: u8,
    request: &[u8],
    what: &str,
) -> Result<Vec<u8>, String> {
    link.transact(function, request)?
        .map_err(|code| format!("{what}: PLC Modbus istisnası {code:#04x}"))
}

pub(crate) fn write_coil(link: &mut dyn PduLink, address: u16, on: bool, what: &str) -> Result<(), String> {
    let mut request = address.to_be_bytes().to_vec();
    request.extend(if on { [0xFF, 0x00] } else { [0x00, 0x00] });
    let reply = data(link, 0x05, &request, what)?;
    if reply != request {
        return Err(format!("{what}: bobin yazma yankısı eşleşmedi"));
    }
    Ok(())
}

pub(crate) fn read_words(
    link: &mut dyn PduLink,
    function: u8,
    address: u16,
    count: usize,
    what: &str,
) -> Result<Vec<u16>, String> {
    let mut request = address.to_be_bytes().to_vec();
    request.extend((count as u16).to_be_bytes());
    let reply = data(link, function, &request, what)?;
    if reply.len() != 1 + 2 * count || usize::from(reply[0]) != 2 * count {
        return Err(format!("{what}: beklenmeyen yanıt uzunluğu"));
    }
    Ok(reply[1..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|w| u16::from_be_bytes(*w))
        .collect())
}

fn write_words(
    link: &mut dyn PduLink,
    function: u8,
    address: u16,
    words: &[u16],
    what: &str,
) -> Result<(), String> {
    let mut request = address.to_be_bytes().to_vec();
    request.extend((words.len() as u16).to_be_bytes());
    request.push((words.len() * 2) as u8);
    request.extend(words.iter().flat_map(|w| w.to_be_bytes()));
    let reply = data(link, function, &request, what)?;
    if reply != request[..4] {
        return Err(format!("{what}: adres/adet yankısı eşleşmedi"));
    }
    Ok(())
}

/// One Delta FC64 sub-command; returns the reply payload after the length byte.
pub(crate) fn fc64(
    link: &mut dyn PduLink,
    command: u8,
    payload: &[u8],
    what: &str,
) -> Result<Result<Vec<u8>, u8>, String> {
    let mut request = vec![0x01, command, payload.len() as u8];
    request.extend(payload);
    Ok(match link.transact(0x64, &request)? {
        Err(code) => Err(code),
        Ok(reply) => {
            if reply.len() < 3
                || reply[..2] != [0x01, command]
                || usize::from(reply[2]) != reply.len() - 3
            {
                return Err(format!("{what}: FC64 yanıt biçimi geçersiz"));
            }
            Ok(reply[3..].to_vec())
        }
    })
}

pub fn identify(link: &mut dyn PduLink) -> Result<Identity, String> {
    let info = match link.transact(0x67, &[0x00, 0x10, 0x00, 0x05])? {
        Ok(reply) if reply.len() == 11 && reply[0] == 10 => {
            let mut words = [0u16; 5];
            for (i, w) in reply[1..].as_chunks::<2>().0.iter().enumerate() {
                words[i] = u16::from_be_bytes(*w);
            }
            Some(words)
        }
        Ok(_) => return Err("Model bilgisi (FC67) yanıtı geçersiz".into()),
        Err(_) => None,
    };
    let capacity_steps = read_words(
        link,
        0x03,
        CAPACITY_REGISTER,
        1,
        "Program kapasitesi (D1002)",
    )?[0];
    if capacity_steps == 0 {
        return Err("PLC program kapasitesini 0 bildirdi".into());
    }
    let slave_id = data(link, 0x11, &[], "Slave ID (FC11)")?;
    Ok(Identity {
        model: info.as_ref().and_then(model_name),
        info,
        capacity_steps,
        slave_id,
    })
}

/// Result of a read-only connection test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub identity: Identity,
    /// M1072 (RUN); `None` when the read was rejected.
    pub running: Option<bool>,
    /// FC64 1/0C: `Some(true)` when a password is set; `None` when not answered.
    pub password_set: Option<bool>,
    /// FC64 1/28: `Some(true)` when the PLC carries an ID; `None` when not answered.
    pub plc_id_set: Option<bool>,
}

/// Connection test that only reads: identity, RUN state and the password / PLC-ID
/// status queries DVPtool sends before a download. Nothing is written to the PLC.
pub fn probe(link: &mut dyn PduLink) -> Result<Probe, String> {
    let identity = identify(link)?;
    let running = match link.transact(0x01, &[0x0C, 0x30, 0x00, 0x01])? {
        Ok(reply) if reply.len() == 2 && reply[0] == 1 => Some(reply[1] & 1 != 0),
        _ => None,
    };
    let password_set = match fc64(link, 0x0C, &[0x01], "Parola durumu (FC64 1/0C)")? {
        Ok(reply) => match reply.first() {
            Some(&PASSWORD_NONE) => Some(false),
            Some(&PASSWORD_SET) => Some(true),
            _ => None,
        },
        Err(_) => None,
    };
    let plc_id_set = match fc64(link, 0x28, &[0x00], "PLC kimliği (FC64 1/28)")? {
        Ok(reply) => reply.first().map(|&b| b == 0xFF),
        Err(_) => None,
    };
    Ok(Probe {
        identity,
        running,
        password_set,
        plc_id_set,
    })
}

/// Read `count` program words starting at word 0.
pub fn read_program(
    link: &mut dyn PduLink,
    capacity: u16,
    count: usize,
    mut progress: impl FnMut(usize),
) -> Result<Vec<u16>, String> {
    let mut words = Vec::with_capacity(count);
    while words.len() < count {
        let (read_fc, _, address, chunk) = program_layout(capacity, words.len());
        let n = chunk.min(count - words.len());
        words.extend(read_words(link, read_fc, address, n, "Program okuma")?);
        progress(words.len());
    }
    Ok(words)
}

/// Read the program from word 0 up to and including END, chunk by chunk (read only).
/// END is found by decoding instruction boundaries, so an operand word equal to 0x0F00
/// (for example K3840) does not end the read early.
pub fn read_until_end(link: &mut dyn PduLink, capacity: u16) -> Result<Vec<u16>, String> {
    let mut words: Vec<u16> = Vec::new();
    while words.len() < usize::from(capacity) {
        let (read_fc, _, address, chunk) = program_layout(capacity, words.len());
        let n = chunk.min(usize::from(capacity) - words.len());
        words.extend(read_words(link, read_fc, address, n, "Program okuma")?);
        if words.contains(&PROGRAM_END) {
            match crate::es2_program_decode::decode_prefix(&words) {
                Ok(crate::es2_program_decode::PrefixDecode::Complete {
                    words_consumed, ..
                }) => {
                    words.truncate(words_consumed);
                    return Ok(words);
                }
                Ok(crate::es2_program_decode::PrefixDecode::Incomplete { .. }) => {}
                Err(e) => return Err(format!("PLC programı çözülemedi: {e}")),
            }
        }
    }
    Err("Program kapasitesi boyunca END bulunamadı".into())
}

pub fn download(
    link: &mut dyn PduLink,
    program: &[u16],
    options: Options,
    mut stage: impl FnMut(Stage),
) -> Result<Report, String> {
    if program.last() != Some(&PROGRAM_END) {
        return Err("Program END (0x0F00) ile bitmeli".into());
    }
    stage(Stage::Identify);
    let identity = identify(link)?;
    if program.len() > usize::from(identity.capacity_steps) {
        return Err(format!(
            "Program {} adım; PLC kapasitesi {} adım",
            program.len(),
            identity.capacity_steps
        ));
    }

    stage(Stage::Stop);
    write_coil(link, RUN_COIL, false, "STOP (M1072)")?;

    stage(Stage::Checks);
    let mut unsupported_checks = Vec::new();
    let mut check = |link: &mut dyn PduLink,
                     name: &'static str,
                     command: u8,
                     payload: &[u8]|
     -> Result<Option<Vec<u8>>, String> {
        match fc64(link, command, payload, name)? {
            Ok(reply) => Ok(Some(reply)),
            Err(UNSUPPORTED) if options.target == Target::Simulator => {
                unsupported_checks.push(name);
                Ok(None)
            }
            Err(code) => Err(format!("{name}: PLC Modbus istisnası {code:#04x}")),
        }
    };
    // Validate the password before touching the PLC's lock state.
    let password = options
        .password
        .as_deref()
        .map(validate_password)
        .transpose()?;
    if is_legacy(&identity) {
        // No FC64 on these series: no ID or transfer-mode query either (DVPtool goes
        // straight from the password check to the 0x1580 header).
        unlock_legacy(link, password)?;
    } else {
        if let Some(reply) = check(link, "Parola durumu (FC64 1/0C)", 0x0C, &[0x01])? {
            match reply.first() {
                Some(&PASSWORD_NONE) => {}
                Some(&PASSWORD_SET) => {
                    let password = password.ok_or(
                        "PLC parola korumalı; indirmek için 4 karakterlik PLC parolasını girin",
                    )?;
                    // DVPtool (routine) accepts the unlock only when the 4 bytes are echoed.
                    match fc64(link, 0x0D, &password, "Parola ile kilit açma (FC64 1/0D)")? {
                        Ok(echo) if echo == password => {}
                        Ok(_) | Err(_) => {
                            return Err("PLC parolası kabul edilmedi. Otomatik tekrar denenmedi: \
                                        PLC hatalı denemeleri sayar ve sınırda kilitlenir."
                                .into());
                        }
                    }
                }
                _ => return Err("Parola durumu yanıtı tanınmadı; indirme durduruldu".into()),
            }
        }
        if let Some(reply) = check(link, "PLC kimliği (FC64 1/28)", 0x28, &[0x00])?
            && reply.first() == Some(&0xFF)
        {
            let words = read_words(
                link,
                0x03,
                PLC_ID_REGISTER,
                PLC_ID_WORDS,
                "PLC kimliği okuma",
            )?;
            let plc_id = decode_plc_id(&words)?;
            match options.plc_id.as_deref() {
                Some(expected) if expected == plc_id => {}
                Some(_) => {
                    return Err("PLC kimliği projedekiyle eşleşmiyor; indirme durduruldu".into());
                }
                None => {
                    return Err(format!(
                        "PLC'de kimlik tanımlı (\"{plc_id}\"); indirmek için aynı PLC kimliğini girin"
                    ));
                }
            }
        }
        check(link, "Aktarım kipi (FC64 1/19)", 0x19, &[0x01])?;
    }

    write_words(
        link,
        0x10,
        SOURCE_HEADER,
        &[0xFFFF, 0xFFFF],
        "Kaynak başlığı (0x1580)",
    )?;
    let capacity = identity.capacity_steps;
    let mut done = 0;
    while done < program.len() {
        let (_, write_fc, address, chunk) = program_layout(capacity, done);
        let n = chunk.min(program.len() - done);
        write_words(
            link,
            write_fc,
            address,
            &program[done..done + n],
            "Program yazma",
        )?;
        done += n;
        stage(Stage::Program {
            done,
            total: program.len(),
        });
    }

    stage(Stage::Commit);
    write_coil(link, COMMIT_COIL, true, "Program kabulü (M1073)")?;
    link.wait(COMMIT_SETTLE);
    if capacity >= COMPLETION_POLL_CAPACITY {
        wait_for_completion(link)?;
    }

    let total = program.len();
    let readback = read_program(link, capacity, total, |done| {
        stage(Stage::Verify { done, total })
    })?;
    if let Some(i) = readback.iter().zip(program).position(|(a, b)| a != b) {
        return Err(format!(
            "Doğrulama hatası: adım {i}: yazılan {:#06x}, okunan {:#06x}",
            program[i], readback[i]
        ));
    }

    // The PLC is still in STOP, so the values are in place before the first scan.
    for (i, value) in options.initial.iter().enumerate() {
        match value {
            Initial::Bit { address, on } => {
                write_coil(link, *address, *on, "Başlangıç değeri (bit)")?;
            }
            Initial::Words { address, words } => {
                write_words(link, 0x10, *address, words, "Başlangıç değeri (kelime)")?;
            }
        }
        stage(Stage::Initial {
            done: i + 1,
            total: options.initial.len(),
        });
    }

    if options.run_after {
        stage(Stage::Run);
        write_coil(link, RUN_COIL, true, "RUN (M1072)")?;
    }
    Ok(Report {
        identity,
        words_written: program.len(),
        running: options.run_after,
        initial_written: options.initial.len(),
        unsupported_checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected request PDUs with the scripted reply (data or Modbus exception code).
    type Steps = Vec<(Vec<u8>, Result<Vec<u8>, u8>)>;

    /// Scripted link: checks each request against the expected PDU and returns the reply.
    struct Script {
        steps: Steps,
        waited: Duration,
    }
    impl PduLink for Script {
        fn transact(&mut self, function: u8, data: &[u8]) -> Result<Result<Vec<u8>, u8>, String> {
            let mut pdu = vec![function];
            pdu.extend(data);
            assert!(!self.steps.is_empty(), "unexpected request {pdu:02x?}");
            let (expected, reply) = self.steps.remove(0);
            assert_eq!(pdu, expected, "request mismatch");
            Ok(reply)
        }
        fn wait(&mut self, duration: Duration) {
            self.waited += duration;
        }
    }
    fn h(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// The exact request sequence DVPtool sent to the Delta editor ES2 simulator for a
    /// three-step program, minus the steps
    /// OpenDVP deliberately omits (1/2A, D1119, D1016 timestamp, D1004, PLC-ID write,
    /// register and file-register restore), plus the readback DVPtool performs only on /V.
    #[test]
    fn es2_sequence_matches_dvptool_capture() {
        let ok = |s: &str| Ok(h(s));
        let mut link = Script {
            steps: vec![
                // Replies are the simulator's own, from the capture.
                (h("6700100005"), ok("0a00010500000100520000")),
                (h("0313ea0001"), ok("023e00")),
                (h("11"), ok("0401007c10")),
                (h("050c300000"), ok("0c300000")),
                (h("64010c0101"), Err(3)),
                (h("6401280100"), Err(3)),
                (h("6401190101"), ok("01190101")),
                (h("101580000204ffffffff"), ok("15800002")),
                (h("102000000306002400c50f00"), ok("20000003")),
                (h("050c31ff00"), ok("0c31ff00")),
                (h("0320000003"), ok("06002400c50f00")),
            ],
            waited: Duration::ZERO,
        };
        let mut stages = Vec::new();
        let report = download(
            &mut link,
            &[0x0024, 0x00c5, 0x0f00],
            Options {
                target: Target::Simulator,
                ..Options::default()
            },
            |s| stages.push(s),
        )
        .unwrap();
        assert!(link.steps.is_empty());
        assert_eq!(link.waited, COMMIT_SETTLE);
        assert_eq!(report.identity.capacity_steps, 0x3E00);
        assert_eq!(
            report.identity.model, None,
            "0x0052 is the simulator's own model code"
        );
        assert_eq!(report.unsupported_checks.len(), 2);
        assert_eq!(stages.first(), Some(&Stage::Identify));
    }

    #[test]
    fn initial_values_are_written_after_verification_and_before_run() {
        let ok = |s: &str| Ok(h(s));
        let mut link = Script {
            steps: vec![
                (h("6700100005"), ok("0a00010500000100520000")),
                (h("0313ea0001"), ok("023e00")),
                (h("11"), ok("0401007c10")),
                (h("050c300000"), ok("0c300000")),
                (h("64010c0101"), Err(3)),
                (h("6401280100"), Err(3)),
                (h("6401190101"), ok("01190101")),
                (h("101580000204ffffffff"), ok("15800002")),
                (h("102000000306002400c50f00"), ok("20000003")),
                (h("050c31ff00"), ok("0c31ff00")),
                (h("0320000003"), ok("06002400c50f00")),
                // Initial values: D2 = 7 (FC16), M5 = ON (FC05); then RUN.
                (h("1010020001020007"), ok("10020001")),
                (h("050805ff00"), ok("0805ff00")),
                (h("050c30ff00"), ok("0c30ff00")),
            ],
            waited: Duration::ZERO,
        };
        let mut stages = Vec::new();
        let report = download(
            &mut link,
            &[0x0024, 0x00c5, 0x0f00],
            Options {
                target: Target::Simulator,
                run_after: true,
                initial: vec![
                    Initial::Words { address: 0x1002, words: vec![7] },
                    Initial::Bit { address: 0x0805, on: true },
                ],
                ..Options::default()
            },
            |s| stages.push(s),
        )
        .unwrap();
        assert!(link.steps.is_empty());
        assert_eq!(report.initial_written, 2);
        let position = |wanted: Stage| stages.iter().position(|s| *s == wanted).unwrap();
        assert!(position(Stage::Initial { done: 2, total: 2 }) < position(Stage::Run));
    }

    #[test]
    fn plc_target_refuses_unanswered_safety_queries_and_passwords() {
        let base = || -> Steps {
            vec![
                (h("6700100005"), Ok(h("0a000105000001000c0000"))),
                (h("0313ea0001"), Ok(h("023e00"))),
                (h("11"), Ok(h("0401007c10"))),
                (h("050c300000"), Ok(h("0c300000"))),
            ]
        };
        let options = Options::default();
        let mut unsupported = Script {
            steps: base(),
            waited: Duration::ZERO,
        };
        unsupported.steps.push((h("64010c0101"), Err(3)));
        assert!(
            download(&mut unsupported, &[0x0f00], options.clone(), |_| {})
                .unwrap_err()
                .contains("Parola")
        );

        let mut locked = Script {
            steps: base(),
            waited: Duration::ZERO,
        };
        locked.steps.push((h("64010c0101"), Ok(h("010c015a"))));
        assert!(
            download(&mut locked, &[0x0f00], options.clone(), |_| {})
                .unwrap_err()
                .contains("parola korumalı")
        );
    }

    fn after_checks() -> Steps {
        vec![
            (h("6401190101"), Ok(h("01190101"))),
            (h("101580000204ffffffff"), Ok(h("15800002"))),
            (h("1020000001020f00"), Ok(h("20000001"))),
            (h("050c31ff00"), Ok(h("0c31ff00"))),
            (h("0320000001"), Ok(h("020f00"))),
        ]
    }
    fn identity_and_stop() -> Steps {
        vec![
            (h("6700100005"), Ok(h("0a000105000001000c0000"))),
            (h("0313ea0001"), Ok(h("023e00"))),
            (h("11"), Ok(h("0401007c10"))),
            (h("050c300000"), Ok(h("0c300000"))),
        ]
    }

    /// A program longer than one 16-word bank read is read in chunks up to its END
    /// instruction; the unused memory after END is not returned.
    #[test]
    fn program_read_spans_chunks_and_stops_at_end() {
        let source = "LD X0
OUT Y0
"
        .repeat(10)
            + "END
";
        let words = crate::es2_encoder::encode(&crate::plc::compile(&source).unwrap())
            .unwrap()
            .words;
        assert!(words.len() > 16 && words.len() <= 32, "{}", words.len());
        let mut padded = words.clone();
        padded.resize(32, 0xFFFF);
        let reply = |part: &[u16]| {
            let mut r = vec![32u8];
            r.extend(part.iter().flat_map(|w| w.to_be_bytes()));
            r
        };
        let mut link = Script {
            steps: vec![
                (h("0320000010"), Ok(reply(&padded[..16]))),
                (h("0320100010"), Ok(reply(&padded[16..]))),
            ],
            waited: Duration::ZERO,
        };
        assert_eq!(read_until_end(&mut link, 0x3E00).unwrap(), words);
        assert!(link.steps.is_empty());
    }

    /// The connection test sends only reads; unsupported status queries become `None`.
    #[test]
    fn probe_reads_identity_and_status_without_writing() {
        let mut link = Script {
            steps: vec![
                (h("6700100005"), Ok(h("0a000105000001000e0000"))),
                (h("0313ea0001"), Ok(h("021ef0"))),
                (h("11"), Ok(h("0401008810"))),
                (h("010c300001"), Ok(h("0101"))),
                (h("64010c0101"), Ok(h("010c015a"))),
                (h("6401280100"), Err(3)),
            ],
            waited: Duration::ZERO,
        };
        let probe = probe(&mut link).unwrap();
        assert!(link.steps.is_empty());
        assert_eq!(probe.identity.model, Some("SS2"));
        assert_eq!(probe.identity.capacity_steps, 7920);
        assert_eq!(probe.running, Some(true));
        assert_eq!(probe.password_set, Some(true));
        assert_eq!(probe.plc_id_set, None);
    }

    #[test]
    fn locked_plc_is_unlocked_once_with_the_password() {
        let mut steps = identity_and_stop();
        steps.push((h("64010c0101"), Ok(h("010c015a"))));
        // "AB12" echoed back = accepted (routine memcmp).
        steps.push((h("64010d0441423132"), Ok(h("010d0441423132"))));
        steps.push((h("6401280100"), Ok(h("01280100"))));
        steps.extend(after_checks());
        let mut link = Script {
            steps,
            waited: Duration::ZERO,
        };
        let options = Options {
            password: Some("AB12".into()),
            ..Options::default()
        };
        download(&mut link, &[0x0f00], options, |_| {}).unwrap();
        assert!(link.steps.is_empty());

        // Wrong password: exactly one attempt, then stop.
        let mut steps = identity_and_stop();
        steps.push((h("64010c0101"), Ok(h("010c015a"))));
        steps.push((h("64010d0441423132"), Err(3)));
        let mut link = Script {
            steps,
            waited: Duration::ZERO,
        };
        let options = Options {
            password: Some("AB12".into()),
            ..Options::default()
        };
        let error = download(&mut link, &[0x0f00], options, |_| {}).unwrap_err();
        assert!(error.contains("tekrar denenmedi"), "{error}");
        assert!(link.steps.is_empty());

        assert!(validate_password("ABC").is_err());
        assert!(validate_password("AB 1").is_err());
    }

    /// Older SS (FC67 model 0x02): no FC64. M1069 says locked, D1013 holds "AB12",
    /// matching password clears M1069; no 1/28 or 1/19 is sent.
    #[test]
    fn legacy_series_unlocks_through_m1069_and_d1013() {
        let legacy = |password: &str| {
            let mut steps = vec![
                (h("6700100005"), Ok(h("0a00010500000100020000"))),
                (h("0313ea0001"), Ok(h("021f00"))),
                (h("11"), Ok(h("0401007c10"))),
                (h("050c300000"), Ok(h("0c300000"))),
                (h("010c2d0001"), Ok(h("0101"))),
                (h("0313f50002"), Ok(h("0441423132"))),
            ];
            if password == "AB12" {
                steps.extend([
                    (h("050c2d0000"), Ok(h("0c2d0000"))),
                    (h("101580000204ffffffff"), Ok(h("15800002"))),
                    (h("1020000001020f00"), Ok(h("20000001"))),
                    (h("050c31ff00"), Ok(h("0c31ff00"))),
                    (h("0320000001"), Ok(h("020f00"))),
                ]);
            }
            let mut link = Script {
                steps,
                waited: Duration::ZERO,
            };
            let options = Options {
                password: Some(password.into()),
                ..Options::default()
            };
            let result = download(&mut link, &[0x0f00], options, |_| {});
            assert!(link.steps.is_empty());
            result
        };
        assert_eq!(legacy("AB12").unwrap().identity.model, Some("SS"));
        // Mismatch: nothing is written to M1069.
        assert!(legacy("XY99").unwrap_err().contains("gönderilmedi"));
    }

    /// 64K-step model: after M1073 DVPtool polls FC64 1/1A until `00 5A`.
    #[test]
    fn large_models_wait_for_the_completion_report() {
        let run = |final_reply: &str, syntax_error: Option<&str>| {
            let mut steps = vec![
                (h("6700100005"), Ok(h("0a000105000001001c0000"))),
                (h("0313ea0001"), Ok(h("02fa00"))),
                (h("11"), Ok(h("0401007c10"))),
                (h("050c300000"), Ok(h("0c300000"))),
                (h("64010c0101"), Ok(h("010c0101"))),
                (h("6401280100"), Ok(h("01280100"))),
                (h("6401190101"), Ok(h("01190101"))),
                (h("101580000204ffffffff"), Ok(h("15800002"))),
                (h("7500000001020f00"), Ok(h("00000001"))),
                (h("050c31ff00"), Ok(h("0c31ff00"))),
                (h("64011a0100"), Ok(h("011a050000010002"))),
                (h("64011a0100"), Ok(h(final_reply))),
            ];
            match syntax_error {
                Some(bits) => steps.push((h("010bec0001"), Ok(h(bits)))),
                None => steps.push((h("7400000001"), Ok(h("020f00")))),
            }
            let mut link = Script {
                steps,
                waited: Duration::ZERO,
            };
            let result = download(&mut link, &[0x0f00], Options::default(), |_| {});
            assert!(link.steps.is_empty(), "{result:?} {:02x?}", link.steps);
            (result, link.waited)
        };
        let (ok, waited) = run("011a02005a", None);
        assert_eq!(ok.unwrap().identity.model, Some("ES2-64K"));
        assert_eq!(waited, COMMIT_SETTLE + COMPLETION_POLL_INTERVAL);
        let (failed, _) = run("011a020000", Some("0101"));
        assert!(failed.unwrap_err().contains("M1004"));
    }

    #[test]
    fn plc_id_must_match_when_the_plc_has_one() {
        let id_block = "12" /* 9 words = 18 bytes */
            .to_owned()
            + "0004"
            + "0050"
            + "004c"
            + "0043"
            + "0031"
            + "0000"
            + "0000"
            + "0000"
            + "0000";
        let with_id = |expected: Option<&str>| {
            let mut steps = identity_and_stop();
            steps.push((h("64010c0101"), Ok(h("010c0101"))));
            steps.push((h("6401280100"), Ok(h("012801ff"))));
            steps.push((h("036e0a0009"), Ok(h(&id_block))));
            let mut link = Script {
                steps,
                waited: Duration::ZERO,
            };
            if expected == Some("PLC1") {
                link.steps.extend(after_checks());
            }
            let options = Options {
                plc_id: expected.map(Into::into),
                ..Options::default()
            };
            (
                download(&mut link, &[0x0f00], options, |_| {}),
                link.steps.is_empty(),
            )
        };
        assert!(matches!(with_id(Some("PLC1")), (Ok(_), true)));
        assert!(with_id(Some("PLC2")).0.unwrap_err().contains("eşleşmiyor"));
        assert!(with_id(None).0.unwrap_err().contains("PLC1"));
    }

    #[test]
    fn identifies_models_from_dvptool_table() {
        assert_eq!(model_name(&[1, 0x500, 1, 0x0C, 0]), Some("ES2"));
        assert_eq!(model_name(&[1, 0x500, 1, 0x0E, 0]), Some("SS2"));
        assert_eq!(model_name(&[1, 0x500, 1, 0x52, 0]), None);
        assert_eq!(model_name(&[1, 0x500, 2, 0x0C, 0]), None);
    }

    #[test]
    fn program_layout_follows_capacity_rule() {
        assert_eq!(program_layout(0x3E00, 0), (0x03, 0x10, 0x2000, 16));
        assert_eq!(program_layout(0x3E00, 0x1FF8), (0x03, 0x10, 0x3FF8, 8));
        assert_eq!(program_layout(0x3E00, 0x2000), (0x03, 0x10, 0x5000, 16));
        assert_eq!(program_layout(0xFA00, 0x1234), (0x74, 0x75, 0x1234, 99));
    }

    #[test]
    fn rejects_program_without_end_or_over_capacity() {
        let mut link = Script {
            steps: vec![],
            waited: Duration::ZERO,
        };
        assert!(
            download(&mut link, &[0x0024], Options::default(), |_| {})
                .unwrap_err()
                .contains("END")
        );
    }
}
