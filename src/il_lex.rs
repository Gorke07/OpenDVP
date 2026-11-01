//! IL operand boundaries. Quotes preserve literal bytes; comments apply only
//! outside literals. Instruction-specific parsers decide permitted characters.
pub(crate) fn words(line: &str) -> Result<Vec<String>, String> {
    let mut chars = line.chars().peekable();
    let mut result = Vec::new();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let Some(first) = chars.next() else { break };
        if first == ';' || (first == '/' && chars.peek() == Some(&'/')) {
            break;
        }
        let mut token = String::from(first);
        if matches!(first, '\'' | '"') {
            let mut closed = false;
            for c in chars.by_ref() {
                token.push(c);
                if c == first {
                    closed = true;
                    break;
                }
            }
            if !closed {
                return Err("Metin sabitinin kapanış tırnağı eksik".into());
            }
            if chars
                .peek()
                .is_some_and(|c| !c.is_whitespace() && *c != ';' && *c != '/')
            {
                return Err("Metin sabitinden sonra operand ayırıcı gerekli".into());
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == ';' {
                    break;
                }
                if c == '/' {
                    let mut next = chars.clone();
                    next.next();
                    if next.peek() == Some(&'/') {
                        break;
                    }
                }
                if matches!(c, '\'' | '"') {
                    return Err("Tırnaklı metin ayrı bir operand olmalı".into());
                }
                token.push(chars.next().unwrap());
            }
        }
        result.push(token);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::words;

    #[test]
    fn literals_keep_spaces_case_and_comment_markers() {
        assert_eq!(
            words("$mov \"Hello; // PLC\" d0 ; outside").unwrap(),
            ["$mov", "\"Hello; // PLC\"", "d0"]
        );
        assert_eq!(
            words("ASC 'AbCdEfGh' D10//outside").unwrap(),
            ["ASC", "'AbCdEfGh'", "D10"]
        );
        assert_eq!(words("$MOV \"\" D0").unwrap(), ["$MOV", "\"\"", "D0"]);
    }

    #[test]
    fn unterminated_or_joined_literals_are_not_silently_repaired() {
        for line in [
            "$MOV \"abc D0",
            "$MOV 'abc D0",
            "$MOV \"abc\"D0",
            "$MOV x\"abc\" D0",
        ] {
            assert!(words(line).is_err(), "{line}");
        }
    }

    #[test]
    fn ordinary_comments_and_ascii_forms_keep_existing_boundaries() {
        assert!(words(" ; comment").unwrap().is_empty());
        assert!(words("// comment").unwrap().is_empty());
        assert_eq!(words("LD\tX0;comment").unwrap(), ["LD", "X0"]);
        assert_eq!(words("ASC A b C d E f G h D0").unwrap().len(), 10);
        assert_eq!(words("MOV K12 D4//comment").unwrap(), ["MOV", "K12", "D4"]);
    }

    #[test]
    fn existing_asc_case_and_operandless_pulse_labels_roundtrip() {
        let p = crate::plc::compile_il("LD X0\nASC \"AbCdEfGh\" D0\nANRP\nEND").unwrap();
        let labels: Vec<_> = p
            .networks
            .iter()
            .flat_map(|n| n.actions.iter().map(|a| a.label()))
            .collect();
        assert!(labels.iter().any(|s| s.contains("AbCdEfGh")));
        assert!(labels.iter().any(|s| s == "ANRP"));
        let source = format!("LD X0\n{}\nEND", labels.join("\n"));
        assert!(crate::plc::compile_il(&source).is_ok());
    }
}
