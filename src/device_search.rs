//! Lexical IL device search. This does not require the program to compile.
//!
//! Classifications are a source-level approximation of WPL search categories;
//! they do not claim that WPL's internal matcher uses the same representation.
//! Group operands match their written base address, not every bit in the group.

use crate::editor_search::{Match, Position};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Any,
    Contact,
    Coil,
    Parameter,
    Comment,
    Function,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Device {
    area: u8,
    number: u32,
}

fn parse_device(value: &str) -> Option<Device> {
    let bytes = value.as_bytes();
    let (&head, digits) = bytes.split_first()?;
    let area = head.to_ascii_uppercase();
    if !matches!(
        area,
        b'X' | b'Y' | b'M' | b'S' | b'T' | b'C' | b'D' | b'E' | b'F'
    ) || !digits.iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    if digits.is_empty() {
        return matches!(area, b'E' | b'F').then_some(Device { area, number: 0 });
    }
    let radix = if matches!(area, b'X' | b'Y') { 8 } else { 10 };
    let number = u32::from_str_radix(std::str::from_utf8(digits).ok()?, radix).ok()?;
    if matches!(area, b'E' | b'F') && number > 7 {
        return None;
    }
    Some(Device { area, number })
}

/// Validate a user-entered device independently of program compilation.
pub fn validate_device(query: &str) -> Result<(), String> {
    if parse_device(query.trim()).is_some() {
        Ok(())
    } else {
        Err("Cihaz adresi girin: X0, Y1, M0, S0, T0, C0, D0 veya E0/F0. X/Y sekizliktir.".into())
    }
}

fn contact(op: &str) -> bool {
    matches!(
        op,
        "LD" | "LDI"
            | "LDP"
            | "LDF"
            | "AND"
            | "ANI"
            | "ANDP"
            | "ANDF"
            | "OR"
            | "ORI"
            | "ORP"
            | "ORF"
    )
}

fn comparison(op: &str) -> bool {
    let op = op.strip_prefix('D').unwrap_or(op);
    ["LD", "AND", "OR"].iter().any(|prefix| {
        op.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "=" | "<>" | ">" | "<" | ">=" | "<=" | "Z=" | "Z<>" | "Z>" | "Z<" | "Z>=" | "Z<="
            )
        })
    })
}

fn coil(op: &str) -> bool {
    op == "OUT"
}

fn function_base(op: &str) -> bool {
    static FUNCTIONS: std::sync::OnceLock<std::collections::HashSet<&'static str>> =
        std::sync::OnceLock::new();
    comparison(op)
        || opendvp::numeric_contacts::parse_spec(op).is_some()
        || opendvp::float_ops::parse_spec(op).is_some()
        || matches!(
            op,
            "$MOV"
                | "XCMP"
                | "YOUT"
                | "DRAND"
                | "DWSUM"
                | "DSCLP"
                | "DCMPT"
                | "DRAMP"
                | "DSORT"
                | "DBOUT"
                | "DBSET"
                | "DBRST"
                | "DINCD"
                | "ASCII"
        )
        || FUNCTIONS
            .get_or_init(|| FUNCTION_MNEMONICS.split_ascii_whitespace().collect())
            .contains(op)
}

fn function(op: &str) -> bool {
    !contact(op) && (function_base(op) || op.strip_suffix('P').is_some_and(function_base))
}

fn classify(kind: DeviceType, op: &str) -> bool {
    match kind {
        DeviceType::Any => true,
        DeviceType::Contact => contact(op),
        DeviceType::Coil => coil(op),
        DeviceType::Parameter => {
            function(op) || matches!(op, "SET" | "RST" | "TMR" | "CNT" | "PLS" | "PLF")
        }
        DeviceType::Function => function(op),
        DeviceType::Comment => false,
    }
}

fn push(out: &mut Vec<Match>, line: usize, line_offset: usize, start: usize, end: usize) {
    out.push(Match {
        start: line_offset + start,
        end: line_offset + end,
        start_position: Position {
            line,
            column: start,
        },
        end_position: Position { line, column: end },
    });
}

/// Look only at the syntactically written device(s) of an operand, including
/// K1..K8 grouped operands and explicit E/F index suffixes. Do not infer the
/// dynamic value of an indexed address or expansion of a bit group.
fn operand_devices(token: &str) -> Vec<(Device, usize, usize)> {
    if !token.is_ascii() {
        return Vec::new();
    }
    let b = token.as_bytes();
    let mut begin = 0;
    if b.len() >= 3 && b[0].eq_ignore_ascii_case(&b'K') && matches!(b[1], b'1'..=b'8') {
        begin = 2;
    }
    let Some(&area) = b.get(begin) else {
        return Vec::new();
    };
    if !matches!(
        area.to_ascii_uppercase(),
        b'X' | b'Y' | b'M' | b'S' | b'T' | b'C' | b'D' | b'E' | b'F'
    ) {
        return Vec::new();
    }
    let mut end = begin + 1;
    while b.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let Some(base) = parse_device(&token[begin..end]) else {
        return Vec::new();
    };
    let mut found = vec![(base, begin, end)];
    if end != b.len() {
        let index_start = end;
        let index_area = b[end].to_ascii_uppercase();
        if !matches!(index_area, b'E' | b'F') {
            return Vec::new();
        }
        end += 1;
        let digit_start = end;
        while b.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end != b.len() {
            return Vec::new();
        }
        // E/F without a number denotes bank 0 in OpenDVP's indexed syntax.
        let index = if digit_start == end {
            Device {
                area: index_area,
                number: 0,
            }
        } else if let Some(index) = parse_device(&token[index_start..end]) {
            index
        } else {
            return Vec::new();
        };
        found.push((index, index_start, end));
    }
    found
}

/// Search explicit IL device operands and/or comments. Results point to the
/// exact address spelling. Source may be incomplete, invalid or uncompiled.
pub fn find_devices(source: &str, query: &str, kind: DeviceType) -> Result<Vec<Match>, String> {
    validate_device(query)?;
    let wanted = parse_device(query.trim()).expect("validated");
    let mut out = Vec::new();
    let mut line_offset = 0;
    for (line_number, raw) in source.split_inclusive('\n').enumerate() {
        let line = raw.trim_end_matches(['\r', '\n']);
        let bytes = line.as_bytes();
        let mut tokens = Vec::new();
        let mut comment = None;
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at].is_ascii_whitespace() {
                at += 1;
                continue;
            }
            if bytes[at] == b';' || (bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'/')) {
                comment = Some(at);
                break;
            }
            let start = at;
            if matches!(bytes[at], b'\'' | b'"') {
                let quote = bytes[at];
                at += 1;
                while at < bytes.len() && bytes[at] != quote {
                    at += 1;
                }
                if at < bytes.len() {
                    at += 1;
                }
                continue;
            }
            while at < bytes.len()
                && !bytes[at].is_ascii_whitespace()
                && bytes[at] != b';'
                && !(bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'/'))
            {
                at += 1;
            }
            tokens.push((start, at));
        }
        if let Some(&(start, end)) = tokens.first() {
            let op = line[start..end].to_ascii_uppercase();
            {
                for &(start, end) in tokens.iter().skip(1) {
                    if !classify(kind, &op) {
                        continue;
                    }
                    for (device, a, b) in operand_devices(&line[start..end]) {
                        if wanted == device {
                            push(&mut out, line_number, line_offset, start + a, start + b);
                        }
                    }
                }
            }
        }
        if matches!(kind, DeviceType::Any | DeviceType::Comment)
            && let Some(start) = comment
        {
            // Scan whole alphanumeric/underscore words; avoids matching D1 in D10 or fooD1.
            let mut word = None;
            for (relative, ch) in line[start..]
                .char_indices()
                .chain(std::iter::once((line.len() - start, ' ')))
            {
                let offset = start + relative;
                if ch.is_alphanumeric() || ch == '_' {
                    word.get_or_insert(offset);
                } else if let Some(begin) = word.take()
                    && parse_device(&line[begin..offset]) == Some(wanted)
                {
                    push(&mut out, line_number, line_offset, begin, offset);
                }
            }
        }
        line_offset += raw.len();
    }
    Ok(out)
}

// Names from the installed Delta editor function index; no runtime installation required.
const FUNCTION_MNEMONICS: &str = "ABS ABSD ABSP ADD ADDP ALT ALTP AND& AND^ AND< AND<= AND<> AND= AND> AND>= AND| ANDZ< ANDZ<= ANDZ<> ANDZ= ANDZ> ANDZ>= ANR ANRP ANS ARWS ASC ASCI ASCIP ASDRW BAND BANI BCD BCDP BIN BINP BLD BLDI BMOV BMOVP BON BONP BOR BORI BOUT BRST BSET CCD CCDP CML CMLP CMP CMPP CMPT CRC CRCP CSFO CVM DABS DABSD DABSP DABSR DACOS DACOSP DADD DADDP DADDR DADDRP DAND DAND& DAND^ DAND< DAND<= DAND<> DAND= DAND> DAND>= DAND| DANDP DANDZ< DANDZ<= DANDZ<> DANDZ= DANDZ> DANDZ>= DASIN DASINP DATAN DATANP DBCD DBCDP DBIN DBINP DBON DBONP DCIMA DCIMR DCLLM DCML DCMLP DCMP DCMPP DCOS DCOSH DCOSHP DCOSP DDEC DDECP DDEG DDEGP DDIV DDIVP DDIVR DDIVRP DDRVA DDRVI DEADD DEADDP DEBCD DEBCDP DEBIN DEBINP DEC DECMP DECMPP DECO DECOP DECP DEDIV DEDIVP DELAY DELAYP DEMUL DEMULP DESQR DESQRP DESUB DESUBP DEXP DEXPP DEZCP DEZCPP DFLT DFLTP DFMOV DFMOVP DFROM DFROMP DGBIN DGBINP DGRY DGRYP DHKY DHOUR DHSCR DHSCS DHSZ DICF DINC DINCP DINT DINTP DIV DIV16 DIV16P DIV32 DIV32P DIVP DLD& DLD^ DLD< DLD<> DLD= DLD> DLD>= DLD| DLDZ< DLDZ<= DLDZ<> DLDZ= DLDZ> DLDZ>= DLN DLNP DLOG DLOGP DMEAN DMEANP DMEMR DMEMRP DMEMW DMEMWP DMOV DMOVP DMOVR DMOVRP DMUL DMULP DMULR DMULRP DNEG DNEGP DOR DOR& DOR^ DOR< DOR<= DOR<> DOR= DOR> DOR>= DOR| DORP DORZ< DORZ<= DORZ<> DORZ= DORZ> DORZ>= DPID DPLSR DPLSV DPLSY DPOW DPOWP DPPMA DPPMR DPRUN DPRUNP DPTPO DRAD DRADP DRCL DRCLP DRCR DRCRP DROL DROLP DROR DRORP DRVA DRVI DSER DSERP DSIN DSINH DSINHP DSINP DSPA DSQR DSQRP DSUB DSUBP DSUBR DSUBRP DSUM DSUMP DSW DSWAP DSWAPP DTAN DTANH DTANHP DTANP DTKY DTM DTO DTOP DVSPO DXCH DXCHP DXOR DXORP DZCP DZCPP DZRN ENCO ENCOP ETHRW FAND< FAND<= FAND<> FAND= FAND> FAND>= FLD< FLD<= FLD<> FLD= FLD> FLD>= FLT FLTP FMOV FMOVP FOR< FOR<= FOR<> FOR= FOR> FOR>= FROM FROMP FTC FWD GBIN GBINP GPS GPWM GRY GRYP HEX HEXP HKY HOUR HST INC INCD INCP INT INTP IST LD& LD^ LD< LD<= LD<> LD= LD> LD>= LD| LDZ< LDZ<= LDZ<> LDZ= LDZ> LDZ>= LRC LRCP MAND MBC MBR MBRD MBS MBWR MCMP MEAN MEANP MEMR MEMRP MEMW MEMWP MINV MMOV MODRD MODRW MODWR MOR MOV MOVP MTR MUL MUL16 MUL16P MUL32 MUL32P MULP MVM MXNR MXOR NEG NEGP OR& OR^ OR< OR<= OR<> OR= OR> OR>= OR| ORZ< ORZ<= ORZ<> ORZ= ORZ> ORZ>= PID PLSR PLSV PLSY PR PRUN PRUNP PWD PWM RAMP RAND RANDP RCL RCLP RCR RCRP RDST REF REFF REFFP REFP REV ROL ROLP ROR RORP RS RSTEF RTMD RTMU SCAL SCLP SCLPP SEGD SEGDP SEGL SER SERP SFRD SFRDP SFTL SFTLP SFTR SFTRP SFWR SFWRP SMOV SMOVP SORT SPD SQR SQRP STMR STOP SUB SUBP SUM SUMP SWAP SWAPP SWRD SWRDP TADD TADDP TCMP TCMPP TKY TO TOP TRD TRDP TSUB TSUBP TTMR TWR TWRP TZCP TZCPP VRRD VRRDP VRSC VRSCP WAND WANDP WOR WORP WSFL WSFLP WSFR WSFRP WSUM WXOR WXORP XCH XCHP ZCP ZCPP ZRN ZRST ZRSTP";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_function_extensions_and_pulse_variants_are_searchable() {
        let source = "LD X0\n$MOV \"abc\" D100\nXCMP X4 C251 D0 D500 D2000\nYOUT C251 D0 D2000 Y0\nMMOVP K-1 D0\nDRANDP K0 K9 D0\nWSUMP D0 K2 D20\nEND";
        for kind in [DeviceType::Parameter, DeviceType::Function] {
            assert_eq!(find_devices(source, "D100", kind).unwrap().len(), 1);
            assert_eq!(find_devices(source, "C251", kind).unwrap().len(), 2);
            assert_eq!(find_devices(source, "D0", kind).unwrap().len(), 5);
        }
        // Basic SET is a parameter category but not a function.
        assert_eq!(
            find_devices("SET M0", "M0", DeviceType::Parameter)
                .unwrap()
                .len(),
            1
        );
        assert!(
            find_devices("SET M0", "M0", DeviceType::Function)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn bare_index_aliases_match_bank_zero() {
        let source = "HOUR E D0 Y0\nHOUR F D1 Y1\nMOV D1E D2\n";
        let low = find_devices(source, "E0", DeviceType::Any).unwrap();
        let high = find_devices(source, "F0", DeviceType::Any).unwrap();
        assert_eq!(selected(source, &low), ["E", "E"]);
        assert_eq!(selected(source, &high), ["F"]);
        assert_eq!(find_devices(source, "E", DeviceType::Any).unwrap(), low);
        assert_eq!(find_devices(source, "F", DeviceType::Any).unwrap(), high);
    }

    #[test]
    fn numeric_float_and_indexed_comparisons_are_parameters() {
        let source = "LD& D1 K1\nFLD= D1 D2\nDBLD D1 K1\nMOV D1 D2\n";
        let contacts = find_devices(source, "D1", DeviceType::Parameter).unwrap();
        assert_eq!(
            contacts
                .iter()
                .map(|m| m.start_position.line)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        assert!(
            find_devices(source, "D1", DeviceType::Contact)
                .unwrap()
                .is_empty()
        );
    }

    fn selected<'a>(source: &'a str, hits: &[Match]) -> Vec<&'a str> {
        hits.iter().map(|hit| &source[hit.start..hit.end]).collect()
    }

    #[test]
    fn documented_wpl_example_and_exact_categories() {
        let source = "LD Y1\nOUT Y1\nINC K4Y1\nSET Y1\n";
        assert_eq!(
            selected(
                source,
                &find_devices(source, "y1", DeviceType::Any).unwrap()
            ),
            ["Y1", "Y1", "Y1", "Y1"]
        );
        assert_eq!(
            selected(
                source,
                &find_devices(source, "Y1", DeviceType::Function).unwrap()
            ),
            ["Y1"]
        );
        assert_eq!(
            find_devices(source, "Y1", DeviceType::Contact).unwrap()[0]
                .start_position
                .line,
            0
        );
        assert_eq!(
            find_devices(source, "Y1", DeviceType::Coil)
                .unwrap()
                .iter()
                .map(|m| m.start_position.line)
                .collect::<Vec<_>>(),
            [1]
        );
    }

    #[test]
    fn no_prefix_or_literal_false_positives_and_comment_filter() {
        let source = "LD D10 ; D1 and D10\r\nMOV \"D1 ; //\" D1 // D1\nNOTANOP D1\n";
        let any = find_devices(source, "D1", DeviceType::Any).unwrap();
        assert_eq!(
            any.iter()
                .map(|m| m.start_position.line)
                .collect::<Vec<_>>(),
            [0, 1, 1, 2]
        );
        assert_eq!(
            find_devices(source, "D1", DeviceType::Comment)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            find_devices(source, "D1", DeviceType::Parameter)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn addresses_use_numeric_identity_and_index_positions() {
        let source = "LD X01\nMOV D12E1 K4Y001\nMOV D1E D2\n";
        assert_eq!(
            selected(
                source,
                &find_devices(source, "X1", DeviceType::Any).unwrap()
            ),
            ["X01"]
        );
        assert_eq!(
            selected(
                source,
                &find_devices(source, "Y1", DeviceType::Any).unwrap()
            ),
            ["Y001"]
        );
        assert_eq!(
            selected(
                source,
                &find_devices(source, "E1", DeviceType::Any).unwrap()
            ),
            ["E1"]
        );
        assert_eq!(
            selected(
                source,
                &find_devices(source, "E0", DeviceType::Any).unwrap()
            ),
            ["E"]
        );
        assert!(find_devices(source, "X8", DeviceType::Any).is_err());
        assert!(find_devices(source, "D", DeviceType::Any).is_err());
        assert!(find_devices(source, "D1M2", DeviceType::Any).is_err());
    }

    #[test]
    fn source_is_searchable_when_other_lines_are_invalid() {
        let source = "broken unknown\nADD D20 D0 D2\nMOV \"unterminated D20\nLD D20\n";
        let hits = find_devices(source, "D20", DeviceType::Any).unwrap();
        assert_eq!(
            hits.iter()
                .map(|m| m.start_position.line)
                .collect::<Vec<_>>(),
            [1, 3]
        );
        assert_eq!(hits[0].start_position.column, 4);
        assert!(find_devices(source, "D20", DeviceType::Function).is_ok());
    }

    #[test]
    fn wpl_parameter_categories_include_timer_counter_and_set_reset() {
        let source = "TMR T0 D0\nCNT C0 D0\nSET M0\nRST M0\n";
        assert_eq!(
            find_devices(source, "T0", DeviceType::Parameter)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            find_devices(source, "C0", DeviceType::Parameter)
                .unwrap()
                .len(),
            1
        );
        assert!(
            find_devices(source, "T0", DeviceType::Coil)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            find_devices(source, "D0", DeviceType::Parameter)
                .unwrap()
                .len(),
            2
        );
        assert!(
            find_devices(source, "D0", DeviceType::Coil)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            find_devices(source, "M0", DeviceType::Parameter)
                .unwrap()
                .len(),
            2
        );
    }
}
