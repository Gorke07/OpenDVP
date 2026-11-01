//! Delta editor program file (`.dvp`) reader: model, program words and device comments.
//!
//! Layout recovered from Delta editor (loader `routine`, writers `routine` V2.40,
//! `routine` V2.30, `routine` V2.20, program XOR `routine`, checksum
//! `routine`) and checked against a file Delta editor itself saved; see
//! Little-endian header:
//!
//! | offset | content |
//! |---|---|
//! | 0x00 | magic `57 01 01` |
//! | 0x0C | u16 program-area size (×4 bytes from format 0xD4) |
//! | 0x4A | u16 Delta editor model index (selects the default area size) |
//! | 0x4C | u16 format version (0xD5 V2.20, 0xD6 V2.30, 0xD7, 0xD8 V2.40) |
//! | 0x54 | u16 model code, same numbering as the `.dvz` model indices |
//! | 0x56 | u16 sum of the program words up to and including END |
//! | 0x5E | u32 program key (0xD7/0xD8) |
//! | 0x100 | program memory image, big-endian words, XOR-obfuscated |
//!
//! The ladder layout records and passwords that follow are not needed for IL and are skipped.
//!
//! [`write`] produces a V2.40 ES2 file with the same layout as the synthetic generator
//! (`decode_dvp.py --synth`) that real Delta editor opened and re-saved.

const HEADER: usize = 0x100;
const END: u16 = 0x0F00;
/// Area sizes the loader accepts as stored; anything else falls back to the model default.
const KNOWN_SIZES: [usize; 12] = [
    0x3E8, 0x2BC, 0xE20, 0x1DA0, 0x1EA0, 0x3DE0, 0x7C00, 0x4E20, 0xEA60, 0x1F400, 0x1D4C0, 0x2710,
];
/// Delta editor model index (0x4A) → default program-area bytes (`routine`).
const MODEL_DEFAULT_SIZES: [(u16, usize); 20] = [
    (0x00, 0x1DA0),
    (0x01, 0x3DE0),
    (0x02, 0x7C00),
    (0x03, 0x3E8),
    (0x04, 0x7C00),
    (0x05, 0x3DE0),
    (0x06, 0x4E20),
    (0x07, 0xEA60),
    (0x08, 0x2710),
    (0x09, 0x4E20),
    (0x0A, 0x7C00),
    (0x0B, 0x3DE0),
    (0x0C, 0xFA0),
    (0x0D, 0x2710),
    (0x0E, 0x7C00),
    (0x0F, 0x6D60),
    (0x10, 0x3DE0),
    (0x11, 0x6D60),
    (0x12, 0x6D60),
    (0x13, 0x9C40),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DvpFile {
    pub format_version: u16,
    /// Model code at 0x54 (the `.dvz` model index numbering, ES2 = 8).
    pub model_code: u16,
    /// Project title (header 0x0E, 20 bytes, trailing spaces/NULs removed).
    pub title: String,
    /// Program words up to and including END.
    pub program: Vec<u16>,
    /// `(device, comment)` from the "Device Comment" section.
    pub device_comments: Vec<(String, String)>,
}

impl DvpFile {
    pub fn model_name(&self) -> Option<&'static str> {
        crate::dvz::MODEL_INDICES
            .iter()
            .find(|(_, index)| *index == self.model_code)
            .map(|(name, _)| *name)
    }
}

fn u16_at(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

/// Text in the file is in the Windows ANSI code page of the PC that saved it.
pub(crate) fn ansi_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => encoding_rs::WINDOWS_1254.decode(bytes).0.into_owned(),
    }
}

pub fn read(data: &[u8]) -> Result<DvpFile, String> {
    if data.len() < HEADER || data[..3] != [0x57, 0x01, 0x01] {
        return Err("Delta editor .dvp dosyası değil (başlık tanınmadı)".into());
    }
    let format_version = u16_at(data, 0x4C);
    let raw_size = usize::from(u16_at(data, 0x0C));
    let mut size = if format_version >= 0xD4 {
        raw_size * 4
    } else {
        raw_size
    };
    if !KNOWN_SIZES.contains(&size) && size < 0x1D4C1 {
        let model = u16_at(data, 0x4A);
        size = MODEL_DEFAULT_SIZES
            .iter()
            .find(|(index, _)| *index == model)
            .map(|(_, bytes)| *bytes)
            .ok_or_else(|| format!("Bilinmeyen Delta editor model indeksi {model:#x}"))?;
    }
    let image = data
        .get(HEADER..HEADER + size)
        .ok_or("DVP program alanı kesik")?;
    let checksum = u16_at(data, 0x56);
    let key = u32::from_le_bytes(data[0x5E..0x62].try_into().unwrap()).to_le_bytes();
    let image: Vec<u8> = image
        .iter()
        .enumerate()
        .map(|(i, b)| match format_version {
            0xD5 | 0xD6 => b ^ checksum as u8,
            0xD7 => b ^ key[i % 4],
            0xD8 => b ^ key[0],
            _ => *b,
        })
        .collect();
    let words: Vec<u16> = image
        .as_chunks::<2>()
        .0
        .iter()
        .map(|w| u16::from_be_bytes(*w))
        .collect();
    let end = words
        .iter()
        .position(|&w| w == END)
        .ok_or("DVP programında END bulunamadı")?;
    let program = words[..=end].to_vec();
    if format_version >= 0xD5 {
        let sum = program.iter().fold(0u16, |s, &w| s.wrapping_add(w));
        if sum != checksum {
            return Err("DVP program sağlama toplamı tutmuyor".into());
        }
    }
    Ok(DvpFile {
        format_version,
        model_code: u16_at(data, 0x54),
        title: ansi_text(&data[0x0E..0x22])
            .trim_end_matches([' ', '\0'])
            .to_owned(),
        program,
        device_comments: device_comments(&data[HEADER + size..]),
    })
}

/// "Device Comment" section lines look like `X0$&start`.
fn device_comments(tail: &[u8]) -> Vec<(String, String)> {
    const MARK: &[u8] = b"Function=Device Comment\r\n";
    const MARK_LF: &[u8] = b"Function=Device Comment\n";
    let start = tail
        .windows(MARK.len())
        .position(|w| w == MARK)
        .map(|p| p + MARK.len())
        .or_else(|| {
            tail.windows(MARK_LF.len())
                .position(|w| w == MARK_LF)
                .map(|p| p + MARK_LF.len())
        });
    let Some(start) = start else {
        return Vec::new();
    };
    let mut comments = Vec::new();
    for line in tail[start..].split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        match line {
            b"<TypeEnd>" | b"<DataBegin>" => continue,
            b"<DataEnd>" => break,
            _ if line.starts_with(b"<") || line.starts_with(b"Function=") => break,
            _ => {
                let text = ansi_text(line);
                if let Some((device, comment)) = text.split_once("$&") {
                    comments.push((device.trim().to_owned(), comment.trim().to_owned()));
                }
            }
        }
    }
    comments
}

/// ES2 program area: Delta editor model index 4 (0x7C00 bytes = 15872 words).
const ES2_MODEL_INDEX: u16 = 0x04;
const ES2_MODEL_CODE: u16 = 0x08;
const ES2_AREA_BYTES: usize = 0x7C00;
const FORMAT_V240: u16 = 0xD8;
const TITLE_BYTES: usize = 20;
/// Device letters of the "Device Comment" grids (`XYMSTCDPI`).
const COMMENT_LETTERS: &str = "XYMSTCDPI";

/// Largest program (in words, END included) an ES2 `.dvp` can hold.
pub const ES2_MAX_WORDS: usize = ES2_AREA_BYTES / 2;

fn ansi_bytes(text: &str, what: &str) -> Result<Vec<u8>, String> {
    if text.chars().any(|c| c.is_control()) {
        return Err(format!("{what}: kontrol karakteri içeremez"));
    }
    if text.is_ascii() {
        return Ok(text.as_bytes().to_vec());
    }
    let (bytes, _, had_errors) = encoding_rs::WINDOWS_1254.encode(text);
    if had_errors {
        return Err(format!(
            "{what}: Windows-1254'te gösterilemeyen karakter var"
        ));
    }
    Ok(bytes.into_owned())
}

/// Check one `(device, comment)` pair and return the line Delta editor stores: `X0$&start`.
///
/// Only the line shape and the devices `X0` / `Y0` are confirmed by a file Delta editor saved. The
/// device name is written verbatim (decimal index after the letter). Delta editor stores the grid row
/// index, so how X/Y (octal on the PLC) numbers above 7 are stored is unverified; those are
/// rejected rather than guessed.
pub(crate) fn comment_line(device: &str, text: &str) -> Result<Vec<u8>, String> {
    let bad_device = || format!("Geçersiz cihaz adı: {device:?}");
    let mut chars = device.chars();
    let letter = chars
        .next()
        .filter(|c| COMMENT_LETTERS.contains(*c))
        .ok_or_else(bad_device)?;
    let digits = chars.as_str();
    if digits.is_empty()
        || digits.len() > 5
        || !digits.bytes().all(|b| b.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
    {
        return Err(bad_device());
    }
    let index: u32 = digits.parse().map_err(|_| bad_device())?;
    if (letter == 'X' || letter == 'Y') && index > 7 {
        return Err(format!(
            "{device}: X/Y cihaz yorumlarında 7'den büyük numaralandırma doğrulanmadı"
        ));
    }
    if text.contains("$&") || text.starts_with('<') || text != text.trim() {
        return Err(format!(
            "{device}: yorum metni geçersiz (\"$&\", baştaki '<' veya kenar boşluğu)"
        ));
    }
    let mut line = device.as_bytes().to_vec();
    line.extend_from_slice(b"$&");
    line.extend(ansi_bytes(text, device)?);
    line.push(b'\n');
    Ok(line)
}

fn text_section(out: &mut Vec<u8>, name: &str) {
    out.extend_from_slice(b"<TypeBegin>\nFunction=");
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(b"\n<TypeEnd>\n<DataBegin>\n");
}

/// Write a V2.40 (`0xD8`) ES2 `.dvp` file.
///
/// `program_words` are PLC-form big-endian words ending with END (`0x0F00`), as produced by
/// `es2_encoder::encode`. The key is the checksum (Delta editor adds a random salt, which the loader
/// does not check). Ladder layout tables are written empty, as `decode_dvp.py --synth` does;
/// Delta editor rebuilds them from the program when it loads the file.
pub fn write(
    program_words: &[u16],
    title: &str,
    comments: &[(String, String)],
) -> Result<Vec<u8>, String> {
    write_with_salt(program_words, title, comments, 0)
}

/// [`write`] with an explicit salt added to the XOR key (only the key's low byte matters).
pub fn write_with_salt(
    program_words: &[u16],
    title: &str,
    comments: &[(String, String)],
    salt: u32,
) -> Result<Vec<u8>, String> {
    if program_words.last() != Some(&END) || program_words[..program_words.len() - 1].contains(&END)
    {
        return Err("Program tam olarak bir END (0x0F00) ile, sonda bitmeli".into());
    }
    if program_words.len() > ES2_MAX_WORDS {
        return Err(format!(
            "Program {} kelime; ES2 sınırı {ES2_MAX_WORDS}",
            program_words.len()
        ));
    }
    let title_bytes = ansi_bytes(title, "Başlık")?;
    if title_bytes.len() > TITLE_BYTES {
        return Err(format!("Başlık en çok {TITLE_BYTES} karakter olabilir"));
    }

    let mut area = vec![0xFFu8; ES2_AREA_BYTES];
    for (i, w) in program_words.iter().enumerate() {
        area[2 * i..2 * i + 2].copy_from_slice(&w.to_be_bytes());
    }
    let checksum = program_words.iter().fold(0u16, |s, &w| s.wrapping_add(w));
    // key = checksum + sum of the four BE password words (empty password: 0) + salt
    let key = u32::from(checksum).wrapping_add(salt);
    let xor = key as u8;

    let mut header = [0u8; HEADER];
    header[..3].copy_from_slice(&[0x57, 0x01, 0x01]);
    header[0x0C..0x0E].copy_from_slice(&((ES2_AREA_BYTES >> 2) as u16).to_le_bytes());
    for off in [0x0E, 0x22, 0x36] {
        header[off..off + TITLE_BYTES].fill(b' ');
    }
    header[0x0E..0x0E + title_bytes.len()].copy_from_slice(&title_bytes);
    header[0x4A..0x4C].copy_from_slice(&ES2_MODEL_INDEX.to_le_bytes());
    header[0x4C..0x4E].copy_from_slice(&FORMAT_V240.to_le_bytes());
    header[0x50] = 1;
    header[0x51] = 1;
    header[0x53] = 1; // PLC
    header[0x54..0x56].copy_from_slice(&ES2_MODEL_CODE.to_le_bytes());
    header[0x56..0x58].copy_from_slice(&checksum.to_le_bytes());
    header[0x5E..0x62].copy_from_slice(&key.to_le_bytes());

    let mut out = header.to_vec();
    out.extend(area.iter().map(|b| b ^ xor));
    // Passwords (empty): first 8 bytes encoded as (i - b), two bytes of padding; hidden length 0.
    let pwd: Vec<u8> = (0..10u8).map(|i| if i < 8 { i } else { 0 }).collect();
    out.extend_from_slice(&pwd);
    out.extend_from_slice(&pwd);
    out.extend_from_slice(&0u16.to_le_bytes());
    // PLC ID (4 bytes, encoded empty), then the 0x10 and 0x18 byte blocks.
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&[0, 1, 2, 3]);
    out.extend_from_slice(&0x10u16.to_le_bytes());
    out.extend_from_slice(&[0; 0x10]);
    out.extend_from_slice(&0x18u16.to_le_bytes());
    out.extend_from_slice(&[0; 0x18]);
    text_section(&mut out, "Initial Value");
    out.extend_from_slice(b"<DataEnd>\n");
    // Ladder layout: one empty block (no rows, no pairs).
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&[0; 0x100]);
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&[0; 0x80]);
    out.extend_from_slice(&0x100u16.to_le_bytes());
    out.extend_from_slice(&[0; 0x200]);
    out.extend_from_slice(&1000u16.to_le_bytes());
    out.extend_from_slice(&[0; 2000]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&[0; 0x100]);
    // 64-byte records: the loader always reads its current count (0x400); 0 fails to load.
    out.extend_from_slice(&0x400u32.to_le_bytes());
    out.resize(out.len() + (0x400 << 6), 0xFF);
    out.extend_from_slice(&0u32.to_le_bytes()); // interreg records
    out.extend_from_slice(&[0; 0x80]);
    out.extend_from_slice(&0x10u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&[0; 0x80]);

    text_section(&mut out, "Device Comment");
    for (device, text) in comments {
        out.extend(comment_line(device, text)?);
    }
    out.extend_from_slice(b"<DataEnd>\n");
    for name in ["Output Comment", "Symbol Table"] {
        text_section(&mut out, name);
        out.extend_from_slice(b"<DataEnd>\n");
    }
    text_section(&mut out, "Device Comment Range");
    out.extend_from_slice(b"Group=0\n<DataEnd>\n");
    out.extend_from_slice(b"<SegCmt>\n</SegCmt>\n<PrjectEnd>\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_file_saved_by_editor() {
        let editor_saved = fixture!("isp-dvp-formats/dvp-samples/editor_resaved_es2.dvp");
        let file = read(&editor_saved).unwrap();
        assert_eq!(file.format_version, 0xD8);
        assert_eq!(file.model_name(), Some("ES2"));
        assert_eq!(file.program, [0x0024, 0x00c5, 0x0f00]);
        assert_eq!(
            file.device_comments,
            [("X0".into(), "start".into()), ("Y0".into(), "lamp".into())]
        );
        assert_eq!(
            crate::es2_program_decode::decode(&file.program)
                .unwrap()
                .trim(),
            "LD X0\nOUT Y0\nEND"
        );
    }

    /// Synthetic file from `decode_dvp.py --synth` (Delta editor opened and re-saved it).
    fn lamp_comments() -> Vec<(String, String)> {
        vec![("X0".into(), "start".into()), ("Y0".into(), "lamp".into())]
    }

    /// Byte ranges where two equally long buffers differ.
    fn diff_ranges(a: &[u8], b: &[u8]) -> Vec<(usize, usize)> {
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for (i, (x, y)) in a.iter().zip(b).enumerate() {
            if x != y {
                match ranges.last_mut() {
                    Some(last) if last.1 == i => last.1 = i + 1,
                    _ => ranges.push((i, i + 1)),
                }
            }
        }
        ranges
    }

    #[test]
    fn write_then_read_round_trips_the_editor_sample() {
        let editor_saved = fixture!("isp-dvp-formats/dvp-samples/editor_resaved_es2.dvp");
        let saved = read(&editor_saved).unwrap();
        let bytes = write(&saved.program, "Motor start", &saved.device_comments).unwrap();
        let again = read(&bytes).unwrap();
        assert_eq!(again.program, saved.program);
        assert_eq!(again.device_comments, saved.device_comments);
        assert_eq!(again.title, "Motor start");
        assert_eq!(again.format_version, 0xD8);
        assert_eq!(again.model_name(), Some("ES2"));
    }

    #[test]
    fn write_matches_the_synthetic_generator_byte_for_byte() {
        let synthetic = fixture!("isp-dvp-formats/dvp-samples/synthetic_es2.dvp");
        let bytes = write(&[0x0024, 0x00c5, 0x0f00], "", &lamp_comments()).unwrap();
        assert_eq!(bytes.len(), synthetic.len());
        assert!(bytes == synthetic, "{:?}", diff_ranges(&bytes, &synthetic));
    }

    fn find(hay: &[u8], needle: &[u8]) -> usize {
        hay.windows(needle.len()).position(|w| w == needle).unwrap()
    }

    #[test]
    fn write_vs_editor_saved_file_differs_only_where_expected() {
        let editor_saved = fixture!("isp-dvp-formats/dvp-samples/editor_resaved_es2.dvp");
        // Same key as the real file (Delta editor's random salt) so the XORed program area compares.
        let salt = 0x00fe_30d2 - 0x0fe9;
        let ours = write_with_salt(&[0x0024, 0x00c5, 0x0f00], "", &lamp_comments(), salt).unwrap();
        let real = &editor_saved[..];
        // Header: only 0x4E..0x50 (file-register capacity, 5000 in the real file; 0 in ours).
        assert_eq!(diff_ranges(&ours[..0x100], &real[..0x100]), [(0x4E, 0x50)]);
        assert_eq!(u16_at(real, 0x4E), 5000);
        // Program area, passwords, PLC ID, fixed blocks and "Initial Value" text: identical.
        let ladder = find(real, b"Initial Value")
            + b"Initial Value
<TypeEnd>
<DataBegin>
<DataEnd>
"
            .len();
        assert_eq!(find(&ours, b"Initial Value"), find(real, b"Initial Value"));
        assert_eq!(&ours[0x100..ladder], &real[0x100..ladder]);
        // Ladder layout (rows, pair maps, 0xFFFF tables) is where we differ: ours is empty.
        let marker = b"<TypeBegin>
Function=Device Comment
";
        let (our_text, real_text) = (find(&ours, marker), find(real, marker));
        assert_ne!(&ours[ladder..our_text], &real[ladder..real_text]);
        // Text sections: Device Comment .. Symbol Table identical, Range differs, tail identical.
        let range = b"<TypeBegin>
Function=Device Comment Range
";
        let (our_range, real_range) = (find(&ours, range), find(real, range));
        assert_eq!(&ours[our_text..our_range], &real[real_text..real_range]);
        assert_ne!(&ours[our_range..], &real[real_range..]);
        let tail = b"<SegCmt>
</SegCmt>
<PrjectEnd>
";
        assert!(ours.ends_with(tail) && real.ends_with(tail));
    }

    #[test]
    fn write_rejects_bad_input() {
        assert!(write(&[0x0024], "", &[]).is_err());
        assert!(write(&[], "", &[]).is_err());
        assert!(write(&[0x0f00, 0x0f00], "", &[]).is_err());
        let mut big = vec![0x0000; ES2_MAX_WORDS];
        big.push(END);
        assert!(write(&big, "", &[]).is_err());
        big.truncate(ES2_MAX_WORDS - 1);
        big.push(END);
        assert!(write(&big, "", &[]).is_ok());
        assert!(write(&[END], &"x".repeat(21), &[]).is_err());
        assert!(write(&[END], &"x".repeat(20), &[]).is_ok());
        assert!(write(&[END], "bad\nname", &[]).is_err());
        let bad = |d: &str, t: &str| write(&[END], "", &[(d.into(), t.into())]);
        assert!(bad("Q0", "x").is_err());
        assert!(bad("X", "x").is_err());
        assert!(
            bad("X10", "x").is_err(),
            "X/Y numbering above 7 is unverified"
        );
        assert!(bad("M10", "x").is_ok());
        assert!(bad("X0", "a$&b").is_err());
        assert!(bad("X0", "a\nb").is_err());
        assert!(bad("X0", "<x").is_err());
    }

    #[test]
    fn write_keeps_turkish_text() {
        let comments = vec![("M0".into(), "Şalter açık".into())];
        let bytes = write(&[END], "Pompa İstasyonu", &comments).unwrap();
        let file = read(&bytes).unwrap();
        assert_eq!(file.title, "Pompa İstasyonu");
        assert_eq!(file.device_comments, comments);
        assert!(write(&[END], "漢字", &[]).is_err());
    }

    #[test]
    fn rejects_other_files_and_corruption() {
        let editor_saved = fixture!("isp-dvp-formats/dvp-samples/editor_resaved_es2.dvp");
        assert!(read(b"not a dvp").is_err());
        let mut bad = editor_saved.clone();
        bad[0x101] ^= 0x01;
        assert!(read(&bad).unwrap_err().contains("sağlama"));
    }
}
