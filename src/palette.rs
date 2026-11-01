//! Instruction palette for the ladder editor, grouped like the instruction cards of PLC
//! IDEs. Every template is compiled by the tests below, so the palette only offers
//! instructions OpenDVP accepts.

#[derive(Debug, PartialEq, Eq)]
pub struct Item {
    pub group: &'static str,
    /// Short symbol shown in the tile (`─| |─`, `MOV`).
    pub symbol: &'static str,
    pub name: &'static str,
    /// IL line inserted into the ladder; operands are placeholders to edit afterwards.
    pub template: &'static str,
}

impl Item {
    /// Contacts and compare contacts go into the condition part of a rung.
    pub fn is_condition(&self) -> bool {
        self.template
            .split_whitespace()
            .next()
            .is_some_and(|op| op.starts_with("LD") || op.starts_with("DLD"))
    }
    pub fn mnemonic(&self) -> &'static str {
        self.template.split_whitespace().next().unwrap_or_default()
    }
}

const fn item(
    group: &'static str,
    symbol: &'static str,
    name: &'static str,
    template: &'static str,
) -> Item {
    Item {
        group,
        symbol,
        name,
        template,
    }
}

pub const GROUPS: &[&str] = &[
    "Bit mantığı",
    "Karşılaştırma",
    "Zamanlayıcı / sayaç",
    "Veri taşıma",
    "Aritmetik",
    "Mantıksal işlem",
    "Döndürme / kaydırma",
    "Veri işleme",
    "Kayan nokta",
    "Gerçek zaman saati",
    "Darbe / yüksek hız",
    "Haberleşme",
    "Proses",
];

const BIT: &str = "Bit mantığı";
const CMP: &str = "Karşılaştırma";
const TC: &str = "Zamanlayıcı / sayaç";
const MOVE: &str = "Veri taşıma";
const MATH: &str = "Aritmetik";
const LOGIC: &str = "Mantıksal işlem";
const SHIFT: &str = "Döndürme / kaydırma";
const DATA: &str = "Veri işleme";
const FLOAT: &str = "Kayan nokta";
const RTC: &str = "Gerçek zaman saati";
const PULSE: &str = "Darbe / yüksek hız";
const COMM: &str = "Haberleşme";
const PROC: &str = "Proses";

pub const ITEMS: &[Item] = &[
    item(BIT, "─| |─", "Normal kontak", "LD X0"),
    item(BIT, "─|/|─", "Ters kontak", "LDI X0"),
    item(BIT, "─|↑|─", "Yükselen kenar kontağı", "LDP X0"),
    item(BIT, "─|↓|─", "Düşen kenar kontağı", "LDF X0"),
    item(BIT, "─( )─", "Çıkış bobini", "OUT Y0"),
    item(BIT, "SET", "Kur (mühürle)", "SET M0"),
    item(BIT, "RST", "Sıfırla", "RST M0"),
    item(BIT, "PLS", "Yükselen kenar darbesi", "PLS M0"),
    item(BIT, "PLF", "Düşen kenar darbesi", "PLF M0"),
    item(BIT, "ALT", "Durum değiştir (toggle)", "ALT M0"),
    item(BIT, "ZRST", "Alanı sıfırla", "ZRST M0 M10"),
    item(CMP, "=", "Eşitse kontak", "LD= D0 K10"),
    item(CMP, "<>", "Eşit değilse kontak", "LD<> D0 K10"),
    item(CMP, ">", "Büyükse kontak", "LD> D0 K10"),
    item(CMP, ">=", "Büyük/eşitse kontak", "LD>= D0 K10"),
    item(CMP, "<", "Küçükse kontak", "LD< D0 K10"),
    item(CMP, "<=", "Küçük/eşitse kontak", "LD<= D0 K10"),
    item(CMP, "D=", "32 bit eşitse kontak", "DLD= D0 K10"),
    item(CMP, "D>", "32 bit büyükse kontak", "DLD> D0 K10"),
    item(CMP, "CMP", "Karşılaştır (3 sonuç biti)", "CMP D0 K10 M0"),
    item(CMP, "ZCP", "Bölge karşılaştır", "ZCP K0 K100 D0 M0"),
    item(CMP, "DCMP", "32 bit karşılaştır", "DCMP D0 K10 M0"),
    item(
        CMP,
        "DZCP",
        "32 bit bölge karşılaştır",
        "DZCP K0 K100 D0 M0",
    ),
    item(TC, "TMR", "Zamanlayıcı", "TMR T0 K30"),
    item(TC, "CNT", "Sayaç (16 bit)", "CNT C0 K5"),
    item(TC, "DCNT", "Sayaç (32 bit)", "DCNT C200 K1000"),
    item(TC, "TTMR", "Öğretmeli zamanlayıcı", "TTMR D10 K1"),
    item(TC, "STMR", "Özel zamanlayıcı", "STMR T7 K30 M20"),
    item(TC, "HOUR", "Çalışma saati sayacı", "HOUR K100 D0 Y0"),
    item(MOVE, "MOV", "Veri taşı", "MOV K0 D0"),
    item(MOVE, "DMOV", "32 bit veri taşı", "DMOV K0 D0"),
    item(MOVE, "MOVP", "Kenarda bir kez taşı", "MOVP K0 D0"),
    item(MOVE, "BMOV", "Blok taşı", "BMOV D0 D10 K5"),
    item(MOVE, "FMOV", "Bloğu doldur", "FMOV K0 D0 K10"),
    item(MOVE, "SMOV", "Basamak taşı", "SMOV D0 K4 K2 D1 K3"),
    item(MOVE, "CML", "Tümleyen taşı", "CML D0 D1"),
    item(MOVE, "XCH", "Değiş tokuş", "XCH D0 D1"),
    item(MOVE, "BCD", "BIN → BCD", "BCD D0 D1"),
    item(MOVE, "BIN", "BCD → BIN", "BIN D0 D1"),
    item(MATH, "ADD", "Topla", "ADD D0 D1 D2"),
    item(MATH, "SUB", "Çıkar", "SUB D0 D1 D2"),
    item(MATH, "MUL", "Çarp", "MUL D0 D1 D2"),
    item(MATH, "DIV", "Böl", "DIV D0 D1 D2"),
    item(MATH, "INC", "Bir artır", "INC D0"),
    item(MATH, "INCP", "Kenarda bir artır", "INCP D0"),
    item(MATH, "DEC", "Bir azalt", "DEC D0"),
    item(MATH, "NEG", "İşaret değiştir", "NEG D0"),
    item(MATH, "DADD", "32 bit topla", "DADD D0 D2 D4"),
    item(MATH, "DSUB", "32 bit çıkar", "DSUB D0 D2 D4"),
    item(MATH, "DMUL", "32 bit çarp", "DMUL D0 D2 D4"),
    item(MATH, "DDIV", "32 bit böl", "DDIV D0 D2 D4"),
    item(MATH, "SQR", "Karekök", "SQR D0 D2"),
    item(MATH, "ABS", "Mutlak değer", "ABS D0"),
    item(LOGIC, "WAND", "Kelime VE", "WAND D0 D1 D2"),
    item(LOGIC, "WOR", "Kelime VEYA", "WOR D0 D1 D2"),
    item(LOGIC, "WXOR", "Kelime ÖZEL VEYA", "WXOR D0 D1 D2"),
    item(LOGIC, "DAND", "32 bit VE", "DAND D0 D2 D4"),
    item(LOGIC, "DOR", "32 bit VEYA", "DOR D0 D2 D4"),
    item(LOGIC, "DXOR", "32 bit ÖZEL VEYA", "DXOR D0 D2 D4"),
    item(SHIFT, "ROR", "Sağa döndür", "ROR D0 K1"),
    item(SHIFT, "ROL", "Sola döndür", "ROL D0 K1"),
    item(SHIFT, "RCR", "Taşıma ile sağa döndür", "RCR D0 K1"),
    item(SHIFT, "RCL", "Taşıma ile sola döndür", "RCL D0 K1"),
    item(SHIFT, "SFTR", "Bit kaydır (sağ)", "SFTR X0 M0 K16 K4"),
    item(SHIFT, "SFTL", "Bit kaydır (sol)", "SFTL X0 M0 K16 K4"),
    item(SHIFT, "WSFR", "Kelime kaydır (sağ)", "WSFR D0 D10 K16 K4"),
    item(SHIFT, "WSFL", "Kelime kaydır (sol)", "WSFL D0 D10 K16 K4"),
    item(SHIFT, "SFWR", "FIFO yaz", "SFWR D0 D10 K10"),
    item(SHIFT, "SFRD", "FIFO oku", "SFRD D10 D20 K10"),
    item(DATA, "DECO", "Kod çöz", "DECO D0 M0 K3"),
    item(DATA, "ENCO", "Kodla", "ENCO M0 D0 K3"),
    item(DATA, "SUM", "Açık bit say", "SUM D0 D1"),
    item(DATA, "BON", "Bit kontrolü", "BON D0 M0 K1"),
    item(DATA, "MEAN", "Ortalama", "MEAN D0 D10 K4"),
    item(DATA, "ANS", "Alarm kur", "ANS T0 K10 S912"),
    item(DATA, "ANR", "Alarm sıfırla", "ANR"),
    item(DATA, "SER", "Veri ara", "SER D0 K5 D10 K10"),
    item(DATA, "SORT", "Sırala", "SORT D0 K4 K2 D100 K1"),
    item(DATA, "SWAP", "Bayt değiştir", "SWAP D0"),
    item(FLOAT, "FLT", "Tamsayı → kayan nokta", "FLT D0 D10"),
    item(FLOAT, "INT", "Kayan nokta → tamsayı", "INT D10 D20"),
    item(FLOAT, "DMOVR", "Kayan nokta taşı", "DMOVR F1.5 D10"),
    item(FLOAT, "DEADD", "Kayan nokta topla", "DEADD D10 D12 D14"),
    item(FLOAT, "DESUB", "Kayan nokta çıkar", "DESUB D10 D12 D14"),
    item(FLOAT, "DEMUL", "Kayan nokta çarp", "DEMUL D10 D12 D14"),
    item(FLOAT, "DEDIV", "Kayan nokta böl", "DEDIV D10 D12 D14"),
    item(FLOAT, "DESQR", "Kayan nokta karekök", "DESQR D10 D12"),
    item(
        FLOAT,
        "DECMP",
        "Kayan nokta karşılaştır",
        "DECMP D10 D12 M0",
    ),
    item(
        FLOAT,
        "DEZCP",
        "Kayan nokta bölge karşılaştır",
        "DEZCP D10 D12 D14 M0",
    ),
    item(FLOAT, "DSIN", "Sinüs", "DSIN D10 D12"),
    item(FLOAT, "DCOS", "Kosinüs", "DCOS D10 D12"),
    item(FLOAT, "DTAN", "Tanjant", "DTAN D10 D12"),
    item(FLOAT, "DEXP", "Üstel", "DEXP D10 D12"),
    item(FLOAT, "DLN", "Doğal logaritma", "DLN D10 D12"),
    item(FLOAT, "DPOW", "Üs alma", "DPOW D10 D12 D14"),
    item(FLOAT, "DRAD", "Derece → radyan", "DRAD D10 D12"),
    item(FLOAT, "DDEG", "Radyan → derece", "DDEG D10 D12"),
    item(RTC, "TRD", "Saati oku", "TRD D0"),
    item(RTC, "TWR", "Saati yaz", "TWR D0"),
    item(RTC, "TCMP", "Saat karşılaştır", "TCMP K10 K20 K30 D0 M0"),
    item(RTC, "TZCP", "Saat bölge karşılaştır", "TZCP D0 D10 D20 M0"),
    item(RTC, "TADD", "Saat topla", "TADD D0 D10 D20"),
    item(RTC, "TSUB", "Saat çıkar", "TSUB D0 D10 D20"),
    item(PULSE, "PLSY", "Darbe çıkışı", "PLSY K1000 K100 Y0"),
    item(
        PULSE,
        "PWM",
        "Darbe genişlik modülasyonu",
        "PWM K100 K200 Y1",
    ),
    item(
        PULSE,
        "PLSR",
        "Rampalı darbe çıkışı",
        "PLSR K1000 K10000 K500 Y0",
    ),
    item(PULSE, "DDRVI", "Göreli konumlama", "DDRVI K1000 K500 Y0 Y4"),
    item(PULSE, "DDRVA", "Mutlak konumlama", "DDRVA K1000 K500 Y0 Y4"),
    item(
        PULSE,
        "DHSCS",
        "Yüksek hız sayıcı kur",
        "DHSCS K100 C235 Y0",
    ),
    item(
        PULSE,
        "DHSCR",
        "Yüksek hız sayıcı sıfırla",
        "DHSCR K100 C235 Y0",
    ),
    item(PULSE, "SPD", "Hız algıla", "SPD X0 K1000 D0"),
    item(COMM, "MODRD", "Modbus oku", "MODRD K1 H1000 K2"),
    item(COMM, "MODWR", "Modbus yaz", "MODWR K1 H1000 K100"),
    item(COMM, "MODRW", "Modbus oku/yaz", "MODRW K1 K3 H1000 D0 K2"),
    item(COMM, "RS", "Seri veri gönder/al", "RS D0 K10 D100 K10"),
    item(COMM, "ASCI", "HEX → ASCII", "ASCI D0 D10 K4"),
    item(COMM, "HEX", "ASCII → HEX", "HEX D0 D10 K4"),
    item(COMM, "CRC", "CRC hesapla", "CRC D0 D10 D20"),
    item(PROC, "PID", "PID kontrol", "PID D0 D1 D100 D200"),
    item(PROC, "RAMP", "Rampa", "RAMP D0 D1 D2 K100"),
];

/// Role of an operand in the template, from its device prefix.
pub fn operand_role(operand: &str) -> &'static str {
    let upper = operand.to_ascii_uppercase();
    match upper.chars().next() {
        Some('X') => "Dijital giriş (X)",
        Some('Y') => "Dijital çıkış (Y)",
        Some('M') => "Yardımcı röle (M)",
        Some('S') => "Adım rölesi (S)",
        Some('T') => "Zamanlayıcı (T)",
        Some('C') => "Sayaç (C)",
        Some('D') => "Veri register'ı (D)",
        Some('K') => "Ondalık sabit (K)",
        Some('H') => "Onaltılık sabit (H)",
        Some('F') => "Kayan nokta sabiti (F)",
        Some('E') => "İndeks register'ı (E)",
        Some('P') => "Atlama etiketi (P)",
        _ => "Operand",
    }
}

/// Other spellings of the same instruction that OpenDVP compiles: edge-triggered `P`,
/// 32-bit `D` and both. Compare contacts and plain contacts have none.
pub fn variants(item: &Item) -> Vec<String> {
    if item.is_condition() {
        return Vec::new();
    }
    let op = item.mnemonic();
    let operands: Vec<&str> = item.template.split_whitespace().skip(1).collect();
    let mut found = Vec::new();
    for candidate in [
        format!("{op}P"),
        format!("D{op}"),
        format!("D{op}P"),
        op.strip_prefix('D').map(str::to_owned).unwrap_or_default(),
        op.strip_suffix('P').map(str::to_owned).unwrap_or_default(),
    ] {
        if candidate.is_empty() || candidate == op || found.contains(&candidate) {
            continue;
        }
        let source = format!("LD M0\n{candidate} {}\nEND\n", operands.join(" "));
        if opendvp::plc::compile(&source).is_ok() {
            found.push(candidate);
        }
    }
    found
}

/// Guide topics (indices into `instruction_help::GUIDE`) whose heading names the
/// instruction or its base form.
pub fn related_topics(item: &Item) -> Vec<usize> {
    let op = item.mnemonic();
    let base = op.strip_suffix('P').filter(|b| !b.is_empty()).unwrap_or(op);
    let bare = base
        .strip_prefix('D')
        .filter(|b| b.len() > 1)
        .unwrap_or(base);
    crate::instruction_help::GUIDE
        .iter()
        .enumerate()
        .filter(|(_, (heading, _))| {
            heading
                // `$MOV` (string move) is a different instruction from `MOV`.
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '$')
                .any(|t| [op, base, bare].iter().any(|w| t.eq_ignore_ascii_case(w)))
        })
        .map(|(i, _)| i)
        .take(4)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_helpers_describe_operands_variants_and_topics() {
        let mov = ITEMS.iter().find(|i| i.template == "MOV K0 D0").unwrap();
        let forms = variants(mov);
        assert!(forms.contains(&"MOVP".to_string()), "{forms:?}");
        assert!(forms.contains(&"DMOV".to_string()), "{forms:?}");
        assert_eq!(operand_role("K0"), "Ondalık sabit (K)");
        assert_eq!(operand_role("D10"), "Veri register'ı (D)");
        let contact = ITEMS.iter().find(|i| i.template == "LD X0").unwrap();
        assert!(variants(contact).is_empty());
        let pid = ITEMS.iter().find(|i| i.mnemonic() == "PID").unwrap();
        assert!(!related_topics(pid).is_empty());
        let titles: Vec<&str> = related_topics(mov)
            .into_iter()
            .map(|i| crate::instruction_help::GUIDE[i].0)
            .collect();
        assert!(titles.iter().all(|t| !t.contains("$MOV")), "{titles:?}");
    }

    fn compiles(item: &Item) -> Result<(), String> {
        let source = if item.is_condition() {
            format!("{}\nOUT Y0\nEND\n", item.template)
        } else {
            format!("LD M0\n{}\nEND\n", item.template)
        };
        let program = opendvp::plc::compile(&source).map_err(|e| e.to_string())?;
        if !program.graphically_editable() || program.networks.len() != 1 {
            return Err("ladder'da düzenlenemez".into());
        }
        Ok(())
    }

    #[test]
    fn every_palette_template_compiles_as_a_ladder_element() {
        let failures: Vec<String> = ITEMS
            .iter()
            .filter_map(|i| compiles(i).err().map(|e| format!("{}: {e}", i.template)))
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn groups_are_listed_and_mnemonics_unique() {
        for item in ITEMS {
            assert!(GROUPS.contains(&item.group), "{}", item.group);
        }
        let mut seen = std::collections::BTreeSet::new();
        for item in ITEMS {
            assert!(seen.insert(item.template), "{}", item.template);
        }
    }
}
