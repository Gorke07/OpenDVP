//! Native comparison corpus for eighteen addressed bit-number forms, without P.
//! Address offsets are signed WORDs; dynamic n independently wraps modulo16/32.
//! Direct wide Fn is OpenDVP/current-native compatibility, not vendor-validator acceptance.
use super::{Case as ComparisonCase, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 18] = [
    "BOUT", "DBOUT", "BSET", "DBSET", "BRST", "DBRST", "BLD", "DBLD", "BLDI", "DBLDI", "BAND",
    "DBAND", "BANI", "DBANI", "BOR", "DBOR", "BORI", "DBORI",
];
const INPUTS: [&str; 32] = [
    "X100", "X101", "X102", "X103", "X104", "X105", "X106", "X107", "X110", "X111", "X112", "X113",
    "X114", "X115", "X116", "X117", "X120", "X121", "X122", "X123", "X124", "X125", "X126", "X127",
    "X130", "X131", "X132", "X133", "X134", "X135", "X136", "X137",
];
fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}
fn base(op: &str) -> &str {
    op.strip_prefix('D').unwrap_or(op)
}
fn action(op: &str) -> bool {
    matches!(base(op), "BOUT" | "BSET" | "BRST")
}
fn load(op: &str) -> bool {
    matches!(base(op), "BLD" | "BLDI")
}
fn inverse(op: &str) -> bool {
    matches!(base(op), "BLDI" | "BANI" | "BORI")
}
fn mov(op: &str) -> &'static str {
    if op.starts_with('D') { "DMOV" } else { "MOV" }
}
fn address(area: &str, index: u16) -> String {
    if matches!(area, "X" | "Y") {
        format!("{area}{index:o}")
    } else {
        format!("{area}{index}")
    }
}
fn index_base(index: u16) -> u16 {
    if index == 0 { 1028 } else { 1180 + index * 2 }
}
fn seed_words(area: &str, start: u16, count: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "MOV H{:04X} {area}{}",
                0xa55au16.wrapping_add(i * 0x111),
                start + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn read_words(area: &str, start: u16, count: u16, out: u16) -> String {
    (0..count)
        .map(|i| format!("MOV {area}{} D{}", start + i, out + i))
        .collect::<Vec<_>>()
        .join("\n")
}
fn seed_bits(area: &str, start: u16, bits: u16, value: u32) -> String {
    assert_ne!(area, "X");
    assert_eq!(bits % 4, 0);
    (0..bits.div_ceil(16))
        .map(|i| {
            format!(
                "MOV H{:04X} K{}{}",
                (value >> ((i % 2) * 16)) & 0xffff,
                (bits - i * 16).min(16) / 4,
                address(area, start + i * 16)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn read_bits(area: &str, start: u16, bits: u16, out: u16) -> String {
    assert_eq!(bits % 4, 0);
    (0..bits.div_ceil(16))
        .map(|i| {
            format!(
                "MOV K{}{} D{}",
                (bits - i * 16).min(16) / 4,
                address(area, start + i * 16),
                out + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn operand(area: &str, index: u16, digits: u16, suffix: &str) -> String {
    if matches!(area, "X" | "Y" | "M" | "S") {
        format!("K{digits}{}{suffix}", address(area, index))
    } else {
        format!("{area}{index}{suffix}")
    }
}
// force=true selects the incoming bit which requires AND/OR to read. With
// force=false, X0 toggles both deciding and nondeciding paths across scans.
fn contact_prefix(op: &str, force: bool) -> &'static str {
    if load(op) {
        ""
    } else if !force {
        "LD X0\n"
    } else if matches!(base(op), "BAND" | "BANI") {
        "LDI X7\n"
    } else {
        "LD X7\n"
    }
}
fn contact(op: &str, first: &str, n: &str, out: u16, force: bool) -> String {
    format!("{}{op} {first} {n}\nOUT M{out}", contact_prefix(op, force))
}
fn site(op: &str, first: &str, n: &str, out: u16, force: bool) -> String {
    if action(op) {
        format!("LD X0\n{op} {first} {n}")
    } else {
        contact(op, first, n, out, force)
    }
}
fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> ComparisonCase {
    let gate = if body.starts_with("LD")
        || body.starts_with("STL")
        || body.starts_with("BLD")
        || body.starts_with("DBLD")
    {
        ""
    } else {
        "LD X0"
    };
    let source=["LD X1\nMOV H55AA E0\nMOV H1234 F0\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068",seed,gate,body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",observe,"END"].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(source.lines().count() <= 512, "source budget: {name}");
    assert!(source.lines().count() * 9 <= 0x1ef0, "WORD budget: {name}");
    let mut indexed = false;
    for line in source.lines() {
        let f: Vec<_> = line.split_whitespace().collect();
        if f.first().is_some_and(|op| FORMS.contains(op)) {
            assert_eq!(f.len(), 3, "{name}: {line}");
            indexed |= f[1..].iter().any(|s| {
                let b = s.as_bytes();
                !s.starts_with(['H', 'E', 'F'])
                    && b.len() >= 4
                    && matches!(b[b.len() - 2], b'E' | b'F')
                    && b[b.len() - 1].is_ascii_digit()
            });
        }
    }
    assert!(indexed, "missing addressed bit-number instruction: {name}");
    case(&format!("indexed_bit_number_{name}"), &source, stages())
}
fn add_inputs(entry: &mut ComparisonCase, value: u32) {
    entry.stages[0].0.extend(
        INPUTS
            .into_iter()
            .enumerate()
            .map(|(bit, input)| (input, (value >> bit) & 1 != 0)),
    );
}
fn check_cases(cases: &[ComparisonCase], count: usize) {
    assert_eq!(cases.len(), count);
    assert_eq!(
        cases.iter().map(|c| &c.name).collect::<BTreeSet<_>>().len(),
        count
    );
    let names: BTreeSet<_> = cases
        .iter()
        .flat_map(|c| c.source.lines())
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    for op in FORMS {
        assert!(names.contains(op), "missing {op}");
    }
}

pub(super) fn common_cases() -> Vec<ComparisonCase> {
    let mut cases = Vec::new();
    // Both operand roles at both widths use each of the sixteen modifier
    // words. Rotating areas distinguish numeric units from packed bit units.
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let suffix = format!("{bank}{index}");
            let alias = index_base(index) + u16::from(bank == 'F');
            let companion = index_base(index) + u16::from(bank == 'E');
            let delta = if index % 2 == 0 { -2 } else { 0 };
            let mut seed = vec![
                seed_words("D", 44, 16),
                seed_words("D", 156, 24),
                seed_words("T", 60, 12),
                seed_words("C", 60, 12),
            ];
            for area in ["Y", "M", "S"] {
                seed.push(seed_bits(area, 56, 48, 0x8001001f));
                seed.push(seed_bits(area, 120, 48, 0xa55a8009));
            }
            seed.push(format!("MOV H6A5A D{companion}\nMOV K{delta} {suffix}"));
            if bank == 'F' && index < 7 {
                seed.push(format!("MOV H5AA5 D{}", alias + 1));
            }
            let mut body = vec![format!(
                "LD X0\nINC {suffix}\nLDI X7\nSET M1020\nRST M1021\nSET M1022"
            )];
            for (slot, op) in FORMS.into_iter().enumerate() {
                let digits = if op.starts_with('D') { 8 } else { 4 };
                let a = ["D", "T", "C", "Y", "M", "S"][(slot / 2 + index as usize) % 6];
                let narea = ["D", "T", "C", "X", "Y", "M", "S"][(slot / 2 + index as usize) % 7];
                let first = operand(
                    a,
                    if a == "D" {
                        160
                    } else if matches!(a, "T" | "C") {
                        64
                    } else {
                        128
                    },
                    digits,
                    &suffix,
                );
                let n = operand(narea, if narea == "D" { 48 } else { 64 }, digits, &suffix);
                body.push(site(op, &first, &n, 700 + slot as u16, false));
            }
            let mut observe = vec![
                read_words("D", 156, 24, 320),
                read_words("D", 44, 16, 350),
                read_words("T", 60, 12, 240),
                read_words("C", 60, 12, 252),
                format!("MOV {suffix} D290\nMOV D{companion} D291"),
            ];
            for (i, area) in ["Y", "M", "S"].into_iter().enumerate() {
                observe.push(read_bits(area, 56, 48, 270 + i as u16 * 6));
                observe.push(read_bits(area, 120, 48, 273 + i as u16 * 6));
            }
            let mut entry = scenario(
                &format!("all_eighteen_both_roles_bank_{suffix}"),
                &seed.join("\n"),
                &body.join("\n"),
                &observe.join("\n"),
            );
            add_inputs(&mut entry, 0x8123001f);
            cases.push(entry);
        }
    }
    assert_eq!(cases.len(), 16);
    append_widths(&mut cases);
    append_wrap_vectors(&mut cases);
    append_direct_wide_f(&mut cases);
    append_aliases(&mut cases);
    append_numeric_boundaries(&mut cases);
    append_controls(&mut cases);
    append_master_false_outputs(&mut cases);
    check_cases(&cases, 117);
    cases
}

fn append_widths(cases: &mut Vec<ComparisonCase>) {
    for (slot, op) in FORMS.into_iter().enumerate() {
        let wide = op.starts_with('D');
        let max = if wide { 8u16 } else { 4 };
        let high = if wide { 31 } else { 15 };
        let area = ["Y", "M", "S"][slot % 3];
        let seed = format!(
            "{}\n{}\nMOV K1 E1\nMOV H6A5A F1\nMOV K0 F2",
            seed_bits(area, 120, 48, 0xa55a33cc),
            seed_words("D", 80, max * 6)
        );
        let mut body = Vec::new();
        for digits in 1..=max {
            let first = format!("K{digits}{}E1", address(area, 127));
            let reset = format!(
                "LDI X7\nDMOV HFFFFFFFF K8{}\nSET {}\nSET {}",
                address(area, 128),
                address(area, 127),
                address(area, 128 + digits * 4)
            );
            for (phase, n) in [format!("K{high}"), format!("K{digits}X100F2")]
                .into_iter()
                .enumerate()
            {
                body.push(reset.clone());
                body.push(site(
                    op,
                    &first,
                    &n,
                    700 + (digits - 1) * 2 + phase as u16,
                    true,
                ));
                body.push(format!(
                    "LDI X7\n{}",
                    read_bits(area, 120, 48, 80 + (digits - 1) * 6 + phase as u16 * 3)
                ));
            }
        }
        let observe = format!(
            "{}\nDMOV K8X100 D280\nMOV E1 D282\nMOV F2 D283",
            read_words("D", 80, max * 6, 320)
        );
        let mut entry = scenario(
            &format!("all_short_widths_guarded_{op}_{area}"),
            &seed,
            &body.join("\n"),
            &observe,
        );
        add_inputs(&mut entry, 0x8123001f);
        cases.push(entry);
    }
    assert_eq!(cases.len(), 34);
}

fn append_wrap_vectors(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let wide = op.starts_with('D');
        let bits = if wide { 32u32 } else { 16 };
        let old = if wide { 0x80000009u32 } else { 0x8009 };
        let m = mov(op);
        let compare = if wide { "DLD=" } else { "LD=" };
        let seed = format!(
            "{}\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 F2\nMOV H5AA5 E2",
            seed_words("D", 159, 5)
        );
        let mut body = Vec::new();
        // No out-of-range K/H bit-number literals: these values live in D32.
        let numbers = [
            -1,
            0,
            1,
            16,
            31,
            32,
            if wide { i32::MIN } else { i16::MIN as i32 },
        ];
        for (slot, n) in numbers.into_iter().enumerate() {
            let selected = (n as u32) & (bits - 1);
            let mask = 1u32 << selected;
            body.push(format!("LDI X7\n{m} H{old:08X} D160\n{m} K{n} D32"));
            body.push(site(op, "D160E1", "D32F2", 700 + slot as u16, true));
            if action(op) {
                let on = match base(op) {
                    "BOUT" | "BSET" => old | mask,
                    "BRST" => old & !mask,
                    _ => unreachable!(),
                };
                let off = if base(op) == "BOUT" { old & !mask } else { old };
                body.push(format!("LDI X7\n{m} D160 D{}\nLD X0\n{m} H{on:08X} D{}\nLDI X0\n{m} H{off:08X} D{}\n{compare} D{} D{}\nOUT M{}",80+slot*2,240+slot*2,240+slot*2,80+slot*2,240+slot*2,840+slot));
            } else {
                let expected = (old & mask != 0) ^ inverse(op);
                body.push(format!(
                    "{} M{}\nOUT M{}",
                    if expected { "LD" } else { "LDI" },
                    700 + slot,
                    840 + slot
                ));
            }
        }
        // M840..M846 are literal-oracle equality markers on every scan; they
        // are independent of native/local result pairing, not pass claims.
        cases.push(scenario(
            &format!("literal_dynamic_modulo_{bits}_oracle_{op}"),
            &seed,
            &body.join("\n"),
            &format!(
                "{}\n{}\nMOV D159 D280\nMOV D162 D281\nMOV D163 D282",
                read_words("D", 80, 14, 320),
                read_words("D", 240, 14, 350)
            ),
        ));
    }
    assert_eq!(cases.len(), 52);
}

fn append_direct_wide_f(cases: &mut Vec<ComparisonCase>) {
    // Vendor validators reject these double direct-F n forms. They remain an
    // explicit current-native/OpenDVP scoped compatibility regression only.
    for op in FORMS.into_iter().filter(|op| op.starts_with('D')) {
        let seed = "MOV K0 E1\nMOV HFFFF F1\nMOV H89AB E2\nMOV K32 F3\nMOV H1234 E4\nMOV K16 F6\nMOV H8765 E7\nDMOV H5678001F E5\nMOV H55AA D159\nMOV H6A5A D162";
        let mut body = Vec::new();
        for (slot, n) in ["F1", "F3", "F6", "E5", "K31", "H0"]
            .into_iter()
            .enumerate()
        {
            body.push("LDI X7\nDMOV H80010009 D160".into());
            body.push(site(op, "D160E1", n, 700 + slot as u16, true));
            body.push(format!("LDI X7\nDMOV D160 D{}", 80 + slot * 2));
        }
        cases.push(scenario(&format!("native_qualified_direct_Fn_wide_not_vendor_validator_{op}"),seed,&body.join("\n"),&format!("{}\nMOV E1 D240\nMOV F1 D241\nMOV E2 D242\nMOV F3 D243\nMOV E4 D244\nDMOV E5 D246\nMOV F6 D248\nMOV E7 D249\nMOV D159 D250\nMOV D162 D251",read_words("D",80,12,320))));
    }
    assert_eq!(cases.len(), 61);
}

fn append_aliases(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let m = mov(op);
        let wide = op.starts_with('D');
        let seed = format!(
            "MOV K0 E1\nMOV H8000 F1\nMOV K0 E2\nMOV H5AA5 F2\nMOV K0 E3\nMOV H2468 E4\nMOV H7788 F4\n{m} K3 D32\n{m} H080B D64\n{}",
            seed_words("D", 159, 6)
        );
        let body = if action(op) {
            let offset = if base(op) == "BRST" { 1 } else { 0 };
            let target = if offset == 1 { "D1181E1" } else { "D1182E1" };
            let n_alias = if base(op) == "BRST" { "H080B" } else { "H0003" };
            format!(
                "LDI X7\nMOV K{offset} E1\nMOV H8000 F1\nMOV H5AA5 E2\nMOV K0 E3\n{m} {n_alias} D64\nLD X0\n{op} {target} K0\nLDI X7\nMOV E1 D80\nMOV F1 D81\nMOV E2 D82\nLD X0\n{op} D64E3 D64E3\nLDI X7\n{m} D64 D84"
            )
        } else {
            let first = contact(op, "D64E1", "D64E1", 700, true);
            let second = contact(op, "D64E1", "D32E2", 701, true);
            let third = contact(op, "D64E1", "D32E2", 702, true);
            format!(
                "LDI X7\nMOV K0 E1\nMOV K0 E2\n{m} H080B D64\n{first}\nLD X0\nMOV K1 E1\n{second}\nLD X0\nMOV K1 E2\n{third}"
            )
        };
        cases.push(scenario(
            &format!(
                "snapshot_alias_and_live_index_cached_pair_{op}_{}",
                if wide { "wide" } else { "word" }
            ),
            &seed,
            &body,
            &format!(
                "{}\n{}\nMOV E1 D240\nMOV F1 D241\nMOV E2 D242",
                read_words("D", 63, 5, 220),
                read_words("D", 80, 6, 230)
            ),
        ));
    }
    assert_eq!(cases.len(), 79);
}

fn append_numeric_boundaries(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let wide = op.starts_with('D');
        let seed = "MOV H1357 C197\nMOV H8001 C198\nMOV H8009 C199\nDMOV H80010009 C200\nDMOV H12345678 C201\nDMOV H55667788 C254\nDMOV H80010009 C255\nMOV H55AA T253\nMOV H0001 T254\nMOV H001F T255\nMOV K0 E1\nMOV H6A5A F1\nMOV K1 F2\nMOV K-1 E3\nMOV K0 E4\nMOV H5AA5 F4\nDMOV K31 D32\nDMOV H80010009 D160";
        let pairs: Vec<(&str, &str)> = if wide {
            vec![
                ("C200E3", "D32E4"),
                ("C199F2", "C200E3"),
                ("C255E1", "T255E3"),
                ("T255E3", "C255E1"),
                ("C198E1", "C199F2"),
            ]
        } else {
            vec![
                ("C199E1", "T255E1"),
                ("T255E1", "C199E1"),
                ("C198F2", "D32E4"),
                ("D160E1", "C198E1"),
            ]
        };
        let mut body = Vec::new();
        for (slot, (first, n)) in pairs.into_iter().enumerate() {
            body.push(site(op, first, n, 700 + slot as u16, true));
            body.push(format!(
                "LDI X7\nMOV C199 D{}\nDMOV C200 D{}",
                80 + slot * 3,
                81 + slot * 3
            ));
        }
        cases.push(scenario(&format!("numeric_C198_C199_C200_C255_T255_{op}"),seed,&body.join("\n"),&format!("{}\n{}\n{}\nDMOV C200 D240\nDMOV C201 D242\nDMOV C254 D244\nDMOV C255 D246\nDMOV D160 D248",read_words("C",197,3,220),read_words("T",253,3,224),read_words("D",80,15,320))));
    }
    assert_eq!(cases.len(), 97);
}

fn append_controls(cases: &mut Vec<ComparisonCase>) {
    for (slot, op) in FORMS.into_iter().enumerate() {
        let m = mov(op);
        let seed = format!(
            "{}\n{}\n{m} H8009 D160\n{m} K3 D32\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 E2\nMOV H1234 F2\nMOV K0 E4\nMOV K1 F4\nSET M1018\nMOV K0 D30\nMOV K0 D60",
            seed_words("D", 159, 8),
            seed_words("D", 80, 8)
        );
        let raw = format!("{op} D160E1 D32E2");
        let ordinary = site(op, "D160E1", "D32E2", 700, true);
        let (label, extra, body) = match slot % 6 {
            0 => {
                let body = if action(op) {
                    format!(
                        "LD X0\nMPS\n{raw}\nMRD\nMOV K1 E1\nMRD\n{raw}\nMPP\nOUT M702\nLDI X7\nMOV K0 E1"
                    )
                } else if load(op) {
                    format!(
                        "{raw}\nMPS\nAND X0\nOUT M700\nMRD\nANI X0\nOUT M701\nMPP\nOR X0\nOUT M702"
                    )
                } else {
                    format!(
                        "LD X0\nMPS\n{raw}\nOUT M700\nMRD\nMOV K1 E1\nMRD\n{raw}\nOUT M701\nMPP\n{raw}\nOUT M702\nLDI X7\nMOV K0 E1"
                    )
                };
                ("MPS_full_logic", String::new(), body)
            }
            1 => (
                "FOR_live_n_same_PC",
                String::new(),
                format!(
                    "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMOV D30 D32\nMUL16 D30 K2 E4\n{ordinary}\nLDI X7\nDMOV D160 D80E4\nINC D30\nNEXT"
                ),
            ),
            2 => (
                "CJ_skipped_visits",
                String::new(),
                format!("LD X2\nCJ P10\n{ordinary}\nP10\nLDI X7\nINC D60"),
            ),
            3 => {
                let core = if action(op) {
                    format!("LDI X7\n{raw}\nMCR N0")
                } else {
                    format!("{}{raw}\nMCR N0\nOUT M700", contact_prefix(op, true))
                };
                (
                    "MC_false_true_logic",
                    String::new(),
                    format!("LD X0\nMC N0\n{core}\nLDI X7\nINC D60"),
                )
            }
            4 | 5 => {
                let active = slot % 6 == 5;
                let extra = format!(
                    "ZRST S0 S1023\n{}\nRST M1040\nRST M1045\nRST M1047",
                    if active { "SET S0" } else { "RST S0" }
                );
                let core = if action(op) {
                    format!("LDI X7\n{raw}")
                } else {
                    format!("{}{raw}\nMOV K123 D80", contact_prefix(op, true))
                };
                (
                    if active { "STL_active" } else { "STL_inactive" },
                    extra,
                    format!("STL S0\n{core}\nRET\nLDI X7\nINC D60"),
                )
            }
            _ => unreachable!(),
        };
        // A legal four-bit flag span tests actual memory effects separately
        // from implicit ALU preservation. n=3 selects M1022 (carry).
        let flag = site(op, "K1M1018F4", "D32E2", 710, true);
        let body = format!("{body}\nLDI X7\nMOV HF K1M1019\n{m} K3 D32\n{flag}");
        let observe = format!(
            "{}\n{}\nMOV K1M1019 D240\nMOV D30 D241\nMOV D60 D242\nMOV E1 D243\nLD M1018\nOUT Y11\nLD S0\nOUT Y12",
            read_words("D", 159, 8, 220),
            read_words("D", 80, 8, 230)
        );
        let mut entry = scenario(
            &format!("control_{label}_flag_span_{op}"),
            &format!("{seed}\n{extra}"),
            &body,
            &observe,
        );
        if slot % 6 == 2 {
            entry.stages = vec![
                (
                    vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                    1,
                ),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X0", false), ("X2", true)], 1),
                (vec![("X0", true), ("X2", false)], 1),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 1),
            ];
        }
        cases.push(entry);
    }
    assert_eq!(cases.len(), 115);
}

fn append_master_false_outputs(cases: &mut Vec<ComparisonCase>) {
    for prefix in ["", "D"] {
        let m = if prefix.is_empty() { "MOV" } else { "DMOV" };
        let seed = format!(
            "{}\n{m} K1 D160\n{m} K0 D162\n{m} K1 D164\n{m} K0 D32\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 E2",
            seed_words("D", 159, 8)
        );
        let body = format!(
            "LD X0\nMC N0\nLDI X7\n{prefix}BOUT D160E1 D32E2\n{prefix}BSET D162E1 D32E2\n{prefix}BRST D164E1 D32E2\nMCR N0"
        );
        cases.push(scenario(
            &format!("master_false_BOUT_clears_SET_RST_hold_{prefix}word"),
            &seed,
            &body,
            &read_words("D", 159, 8, 220),
        ));
    }
    assert_eq!(cases.len(), 117);
}

pub(super) fn model_cases(target: Target) -> Vec<ComparisonCase> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let mut cases = Vec::new();
    for op in FORMS {
        let wide = op.starts_with('D');
        let bits = if wide { 32u16 } else { 16 };
        let digits = bits / 4;
        let m = mov(op);
        let offset = if wide { -1 } else { 0 };
        let mut seed = vec![
            seed_words("D", last - 2, 3),
            format!(
                "{m} H8009 D{}\n{m} K0 D32\n{m} H8009 D160\nMOV K{offset} E1\nMOV H6A5A F1\nMOV K-{} E2\nMOV H1234 F2\nMOV K0 E3",
                last - u16::from(wide),
                bits - 1
            ),
        ];
        for (area, start) in [("Y", 220), ("M", 4060), ("S", 988)] {
            seed.push(seed_bits(area, start, 36, 0xa55a8009));
        }
        let end = format!("D{last}E1");
        let mut body = vec![
            site(op, &end, "D32E3", 700, true),
            site(op, "D160E3", &end, 701, true),
        ];
        for (slot, (area, last_bit)) in [("Y", 255), ("M", 4095), ("S", 1023)]
            .into_iter()
            .enumerate()
        {
            let group = format!("K{digits}{}E2", address(area, last_bit));
            body.push(site(op, &group, "D32E3", 702 + slot as u16 * 2, true));
            body.push(site(op, "D160E3", &group, 703 + slot as u16 * 2, true));
        }
        let observe = format!(
            "{}\n{}\n{}\n{}\nDMOV D160 D250\nMOV E1 D252\nMOV E2 D253",
            read_words("D", last - 2, 3, 200),
            read_bits("Y", 220, 36, 220),
            read_bits("M", 4060, 36, 224),
            read_bits("S", 988, 36, 228)
        );
        cases.push(scenario(
            &format!(
                "model_{}_last_word_pair_and_both_packed_roles_{op}",
                target.name()
            ),
            &seed.join("\n"),
            &body.join("\n"),
            &observe,
        ));
    }
    check_cases(&cases, 18);
    cases
}
