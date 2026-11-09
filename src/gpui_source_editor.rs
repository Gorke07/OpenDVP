//! Offline language support for the PLC source editor. No external language server.
use super::*;
use gpui_component::input::{
    CompletionProvider, FoldRange, HighlightStyleResolver, HoverProvider, InputEdit,
    InputHighlighter, Rope, RopeExt,
};
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    Documentation, Hover, HoverContents, MarkupContent, MarkupKind, TextEdit,
};
use std::{ops::Range, rc::Rc, sync::OnceLock};

const ST_WORDS: &str = "PROGRAM END_PROGRAM VAR VAR_INPUT VAR_OUTPUT VAR_IN_OUT VAR_GLOBAL VAR_TEMP END_VAR RETAIN CONSTANT AT IF THEN ELSE ELSIF END_IF CASE OF END_CASE FOR TO BY DO END_FOR WHILE END_WHILE REPEAT UNTIL END_REPEAT EXIT RETURN FUNCTION END_FUNCTION FUNCTION_BLOCK END_FUNCTION_BLOCK TYPE END_TYPE STRUCT END_STRUCT ARRAY AND OR XOR NOT MOD TRUE FALSE";
const TYPES: &str = "BOOL INT DINT WORD DWORD REAL STRING";
const BUILTIN_BLOCKS: &str = "TON TOF TP CTU CTD CTUD";

pub(super) fn language_name(language: SourceLanguage) -> &'static str {
    if language == SourceLanguage::St {
        "opendvp-st"
    } else {
        "opendvp-il"
    }
}

fn entries() -> &'static [opendvp::st::LibraryEntry] {
    static ENTRIES: OnceLock<Vec<opendvp::st::LibraryEntry>> = OnceLock::new();
    ENTRIES.get_or_init(opendvp::st::library_entries)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Comment,
    String,
    Number,
    Keyword,
    Type,
    Device,
    Name,
    Operator,
}

#[derive(Debug)]
struct Token {
    range: Range<usize>,
    kind: Kind,
}

fn in_words(word: &str, words: &str) -> bool {
    words
        .split_whitespace()
        .any(|w| word.eq_ignore_ascii_case(w))
}

fn ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Keeps byte ranges intact, including Unicode comments and unfinished source.
fn tokens(text: &str, language: SourceLanguage) -> Vec<Token> {
    let mut result = Vec::new();
    let mut i = 0;
    let st = language == SourceLanguage::St;
    while i < text.len() {
        let start = i;
        let rest = &text[i..];
        let c = rest.chars().next().unwrap();
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        let kind = if rest.starts_with("//") || (!st && c == ';') {
            i += rest.find('\n').unwrap_or(rest.len());
            Kind::Comment
        } else if rest.starts_with("(*") || rest.starts_with("/*") {
            let (open, close) = if rest.starts_with("(*") {
                ("(*", "*)")
            } else {
                ("/*", "*/")
            };
            i += 2;
            let mut depth = 1;
            while i < text.len() && depth > 0 {
                if text[i..].starts_with(open) {
                    depth += 1;
                    i += 2;
                } else if text[i..].starts_with(close) {
                    depth -= 1;
                    i += 2;
                } else {
                    i += text[i..].chars().next().unwrap().len_utf8();
                }
            }
            Kind::Comment
        } else if c == '\'' || c == '"' {
            i += 1;
            while i < text.len() {
                let next = text[i..].chars().next().unwrap();
                i += next.len_utf8();
                if next == c {
                    if text[i..].starts_with(c) {
                        i += 1;
                    } else {
                        break;
                    }
                } else if next == '$' && i < text.len() {
                    i += text[i..].chars().next().unwrap().len_utf8();
                }
            }
            Kind::String
        } else if ident(c) {
            i += c.len_utf8();
            while i < text.len() && text[i..].chars().next().is_some_and(ident) {
                i += text[i..].chars().next().unwrap().len_utf8();
            }
            let word = &text[start..i];
            if c.is_ascii_digit() {
                Kind::Number
            } else if st && in_words(word, TYPES) {
                Kind::Type
            } else if (st && in_words(word, ST_WORDS))
                || (!st && il_words().iter().any(|w| word.eq_ignore_ascii_case(w)))
            {
                Kind::Keyword
            } else if word.parse::<opendvp::plc::Device>().is_ok() {
                Kind::Device
            } else if !st
                && word.len() > 1
                && matches!(c.to_ascii_uppercase(), 'K' | 'H')
                && word[1..].chars().all(|c| c.is_ascii_hexdigit())
            {
                Kind::Number
            } else {
                Kind::Name
            }
        } else {
            i += c.len_utf8();
            Kind::Operator
        };
        result.push(Token {
            range: start..i,
            kind,
        });
    }
    result
}

fn il_words() -> &'static [String] {
    static WORDS: OnceLock<Vec<String>> = OnceLock::new();
    WORDS.get_or_init(|| {
        let mut words: Vec<_> = crate::palette::ITEMS
            .iter()
            .flat_map(crate::palette::variants)
            .collect();
        words.extend(
            crate::palette::ITEMS
                .iter()
                .map(|i| i.mnemonic().to_owned()),
        );
        words.extend(
            [
                "END", "FEND", "SRET", "IRET", "CALL", "MPS", "MRD", "MPP", "ANB", "ORB", "NOP",
            ]
            .map(str::to_owned),
        );
        words.sort();
        words.dedup();
        words
    })
}

struct PlcHighlighter {
    language: SourceLanguage,
    tokens: Vec<Token>,
}

impl InputHighlighter for PlcHighlighter {
    fn language(&self) -> SharedString {
        language_name(self.language).into()
    }
    fn update(
        &mut self,
        _: Option<InputEdit>,
        text: &Rope,
        _: bool,
        _: &mut Window,
        _: &mut Context<EditorState>,
    ) {
        self.tokens = tokens(&text.to_string(), self.language);
    }
    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut runs = Vec::new();
        let mut at = range.start;
        for token in self
            .tokens
            .iter()
            .skip_while(|t| t.range.end <= range.start)
            .take_while(|t| t.range.start < range.end)
        {
            let start = token.range.start.max(range.start);
            let end = token.range.end.min(range.end);
            if at < start {
                runs.push((at..start, HighlightStyle::default()));
            }
            let name = match token.kind {
                Kind::Comment => "comment",
                Kind::String => "string",
                Kind::Number => "number",
                Kind::Keyword => "keyword",
                Kind::Type => "type",
                Kind::Device => "constant",
                Kind::Operator => "operator",
                Kind::Name => "variable",
            };
            let mut style = resolver.style(name).unwrap_or_default();
            let dark = colors::is_dark();
            let color = match token.kind {
                Kind::Comment => {
                    if dark {
                        0x97a4b4
                    } else {
                        MUTED
                    }
                }
                Kind::Keyword => {
                    if dark {
                        0x79b8ff
                    } else {
                        0x005cc5
                    }
                }
                Kind::Type => {
                    if dark {
                        0xb392f0
                    } else {
                        0x6f42c1
                    }
                }
                Kind::String => {
                    if dark {
                        0x9ecbff
                    } else {
                        0x24663b
                    }
                }
                Kind::Number => {
                    if dark {
                        0x79b8ff
                    } else {
                        0x005cc5
                    }
                }
                Kind::Device => {
                    if dark {
                        0xffab70
                    } else {
                        0x9b3d16
                    }
                }
                _ => {
                    if dark {
                        0xe6ebf1
                    } else {
                        TEXT
                    }
                }
            };
            // These are already theme-specific syntax colors; do not remap the
            // light tones a second time through the general UI palette.
            style.color = Some(gpui::rgb(color).into());
            runs.push((start..end, style));
            at = end;
        }
        if at < range.end {
            runs.push((at..range.end, HighlightStyle::default()));
        }
        runs
    }
    fn fold_ranges(&self, _: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

/// Configure only at creation or when the active program's language changes.
pub(super) fn configure(
    editor: &mut EditorState,
    language: SourceLanguage,
    cx: &mut Context<EditorState>,
) {
    editor.set_highlighter(language_name(language), cx);
    editor.set_highlighter_factory(
        Rc::new(move |_| {
            Some(Box::new(PlcHighlighter {
                language,
                tokens: Vec::new(),
            }))
        }),
        cx,
    );
    editor.lsp_mut().completion_provider = Some(Rc::new(PlcLanguage { language }));
    editor.lsp_mut().hover_provider = Some(Rc::new(PlcLanguage { language }));
    editor.lsp_mut().completion_menu.max_width = px(480.);
}

struct PlcLanguage {
    language: SourceLanguage,
}

fn word_range(text: &str, offset: usize) -> Range<usize> {
    let offset = text.floor_char_boundary(offset.min(text.len()));
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| ident(*c))
        .last()
        .map_or(offset, |(i, _)| i);
    let end = text[offset..]
        .char_indices()
        .take_while(|(_, c)| ident(*c))
        .last()
        .map_or(offset, |(i, c)| offset + i + c.len_utf8());
    start..end
}

fn signature(entry: &opendvp::st::LibraryEntry) -> String {
    let inputs = entry
        .inputs
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect::<Vec<_>>()
        .join(", ");
    let outputs = entry
        .outputs
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{}({inputs}){}{}",
        entry.name,
        entry
            .returns
            .as_ref()
            .map(|t| format!(" : {t}"))
            .unwrap_or_default(),
        if outputs.is_empty() {
            String::new()
        } else {
            format!(" → {outputs}")
        }
    )
}

/// Read declarations without requiring an otherwise complete/compilable program.
fn declarations(text: &str) -> BTreeMap<String, (String, String)> {
    let lexed = tokens(text, SourceLanguage::St);
    let lexed: Vec<_> = lexed.iter().filter(|t| t.kind != Kind::Comment).collect();
    let mut out = BTreeMap::new();
    let mut in_var = false;
    let mut start = 0;
    for (i, token) in lexed.iter().enumerate() {
        let word = &text[token.range.clone()];
        if in_words(
            word,
            "VAR VAR_INPUT VAR_OUTPUT VAR_IN_OUT VAR_GLOBAL VAR_TEMP",
        ) {
            in_var = true;
            start = i + 1;
        } else if word.eq_ignore_ascii_case("END_VAR") {
            in_var = false;
        } else if in_var && word == ";" {
            start = i + 1;
        } else if in_var && word == ":" && i + 1 < lexed.len() {
            let ty = &text[lexed[i + 1].range.clone()];
            if ty == "=" {
                continue;
            }
            for name in lexed[start..i]
                .iter()
                .take_while(|t| !text[t.range.clone()].eq_ignore_ascii_case("AT"))
            {
                if name.kind == Kind::Name {
                    let label = &text[name.range.clone()];
                    out.insert(
                        label.to_ascii_uppercase(),
                        (label.to_owned(), ty.to_owned()),
                    );
                }
            }
        }
    }
    out
}

fn members(ty: &str) -> Vec<(String, String)> {
    if let Some(entry) = entries().iter().find(|e| e.name.eq_ignore_ascii_case(ty)) {
        return entry
            .inputs
            .iter()
            .chain(&entry.outputs)
            .map(|p| (p.name.clone(), p.ty.clone()))
            .collect();
    }
    let fields = match ty.to_ascii_uppercase().as_str() {
        "TON" | "TOF" | "TP" => "Q:BOOL ET:DINT",
        "CTU" | "CTD" => "Q:BOOL CV:INT",
        "CTUD" => "QU:BOOL QD:BOOL CV:INT",
        _ => "",
    };
    fields
        .split_whitespace()
        .filter_map(|p| p.split_once(':'))
        .map(|(name, ty)| (name.to_owned(), ty.to_owned()))
        .collect()
}

fn completions(text: &str, offset: usize, language: SourceLanguage) -> Vec<CompletionItem> {
    let offset = text.floor_char_boundary(offset.min(text.len()));
    let range = word_range(text, offset);
    if tokens(text, language).iter().any(|t| {
        t.range.start < offset
            && offset <= t.range.end
            && matches!(t.kind, Kind::String | Kind::Comment)
    }) {
        return Vec::new();
    }
    let prefix = text[range.start..offset].to_ascii_uppercase();
    let rope = Rope::from_str(text);
    let replacement = lsp_types::Range::new(
        rope.offset_to_position(range.start),
        rope.offset_to_position(range.end),
    );
    let mut candidates = BTreeMap::new();
    let mut add = |label: String, insert: String, detail: String, kind, doc: Option<String>| {
        if label.to_ascii_uppercase().starts_with(&prefix) {
            candidates
                .entry(label.to_ascii_uppercase())
                .or_insert(CompletionItem {
                    label,
                    kind: Some(kind),
                    detail: Some(detail),
                    documentation: doc.map(Documentation::String),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: replacement,
                        new_text: insert,
                    })),
                    ..Default::default()
                });
        }
    };
    if language == SourceLanguage::St {
        let declarations = declarations(text);
        if range.start > 0 && text[..range.start].ends_with('.') {
            let object = word_range(text, range.start - 1);
            if let Some((_, ty)) = declarations.get(&text[object].to_ascii_uppercase()) {
                for (name, ty) in members(ty) {
                    add(name.clone(), name, ty, CompletionItemKind::FIELD, None);
                }
            }
            return candidates.into_values().collect();
        }
        for (name, ty) in declarations.values() {
            add(
                name.clone(),
                name.clone(),
                ty.clone(),
                CompletionItemKind::VARIABLE,
                None,
            );
        }
        for entry in entries() {
            // A function is an expression. FBs require an instance declaration: complete
            // their type name only; the library browser inserts a complete instance.
            let insert =
                if entry.returns.is_some() && !text[range.end..].trim_start().starts_with('(') {
                    entry.call("")
                } else {
                    entry.name.clone()
                };
            add(
                entry.name.clone(),
                insert,
                signature(entry),
                CompletionItemKind::FUNCTION,
                Some(entry.doc_in(!opendvp::i18n::is_en()).into()),
            );
        }
        for word in ST_WORDS.split_whitespace().chain(TYPES.split_whitespace()) {
            add(
                word.into(),
                word.into(),
                "ST".into(),
                CompletionItemKind::KEYWORD,
                None,
            );
        }
        for word in BUILTIN_BLOCKS.split_whitespace() {
            add(
                word.into(),
                word.into(),
                "FUNCTION_BLOCK".into(),
                CompletionItemKind::CLASS,
                Some(
                    members(word)
                        .iter()
                        .map(|(n, t)| format!("{n}: {t}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            );
        }
        for (name, detail) in [
            ("ABS", "ABS(IN)"),
            ("MIN", "MIN(IN1, IN2)"),
            ("MAX", "MAX(IN1, IN2)"),
            ("LIMIT", "LIMIT(MN, IN, MX)"),
            ("SEL", "SEL(G, IN0, IN1)"),
            ("SQRT", "SQRT(IN: REAL) : REAL"),
            ("LEN", "LEN(IN: STRING) : INT"),
        ] {
            add(
                name.into(),
                name.into(),
                detail.into(),
                CompletionItemKind::FUNCTION,
                None,
            );
        }
        for from in "BOOL INT DINT WORD DWORD REAL".split_whitespace() {
            for to in "BOOL INT DINT WORD DWORD REAL".split_whitespace() {
                let name = format!("{from}_TO_{to}");
                add(
                    name.clone(),
                    name,
                    format!("({from}) → {to}"),
                    CompletionItemKind::FUNCTION,
                    None,
                );
            }
        }
        for (label, body) in [
            ("IF…THEN", "IF TRUE THEN\n    \nEND_IF;"),
            ("FOR…DO", "FOR i := 0 TO 9 DO\n    \nEND_FOR;"),
            ("CASE…OF", "CASE 0 OF\n    0: ;\nELSE\n    ;\nEND_CASE;"),
            ("WHILE…DO", "WHILE FALSE DO\n    \nEND_WHILE;"),
        ] {
            add(
                label.into(),
                body.into(),
                tr("Kod şablonu").into(),
                CompletionItemKind::SNIPPET,
                None,
            );
        }
    } else {
        for word in il_words() {
            let item = crate::palette::ITEMS.iter().find(|i| i.mnemonic() == word);
            add(
                word.clone(),
                word.clone(),
                item.map_or("IL", |i| tr(i.name)).into(),
                CompletionItemKind::KEYWORD,
                item.map(|i| format!("{}\n{}", tr(i.name), i.template)),
            );
        }
    }
    candidates.into_values().take(120).collect()
}

impl CompletionProvider for PlcLanguage {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _: CompletionContext,
        _: &mut Window,
        _: &mut App,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        Task::ready(Ok(CompletionResponse::Array(completions(
            &text.to_string(),
            offset,
            self.language,
        ))))
    }
    fn is_completion_trigger(&self, _: usize, new_text: &str, _: &mut App) -> bool {
        new_text
            .chars()
            .last()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '.')
    }
}

impl HoverProvider for PlcLanguage {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _: &mut Window,
        _: &mut App,
    ) -> Task<anyhow::Result<Option<Hover>>> {
        let source = text.to_string();
        let range = word_range(&source, offset);
        let word = &source[range.clone()];
        let content = if self.language == SourceLanguage::St {
            let declarations = declarations(&source);
            let ty = declarations
                .get(&word.to_ascii_uppercase())
                .map(|(_, ty)| ty.as_str())
                .unwrap_or(word);
            entries()
                .iter()
                .find(|e| e.name.eq_ignore_ascii_case(ty))
                .map(|e| format!("{}\n\n{}", signature(e), e.doc_in(!opendvp::i18n::is_en())))
                .or_else(|| {
                    declarations
                        .get(&word.to_ascii_uppercase())
                        .map(|(name, ty)| format!("{name}: {ty}"))
                })
        } else {
            crate::palette::ITEMS
                .iter()
                .find(|i| i.mnemonic().eq_ignore_ascii_case(word))
                .map(|i| format!("{}\n{}", tr(i.name), i.template))
        };
        Task::ready(Ok(content.map(|value| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::PlainText,
                value,
            }),
            range: Some(lsp_types::Range::new(
                text.offset_to_position(range.start),
                text.offset_to_position(range.end),
            )),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::{CompletionTextEdit, Kind, Rope, RopeExt, SourceLanguage, completions, tokens};
    #[test]
    fn comments_strings_and_unicode_keep_byte_boundaries() {
        let text = "(* Türkçe (* iç *) yorum *)\nIF X0 THEN name := 'a''b'; END_IF; // son";
        let found = tokens(text, SourceLanguage::St);
        assert_eq!(found.iter().filter(|t| t.kind == Kind::Comment).count(), 2);
        assert!(
            found
                .iter()
                .any(|t| t.kind == Kind::String && &text[t.range.clone()] == "'a''b'")
        );
        assert!(
            found
                .iter()
                .any(|t| t.kind == Kind::Device && &text[t.range.clone()] == "X0")
        );
        assert!(found.iter().all(|t| text.get(t.range.clone()).is_some()));
    }
    #[test]
    fn completion_replaces_whole_word_using_editor_positions() {
        let text = "(* 😀 *) moTOR";
        let items = completions(text, text.len() - 2, SourceLanguage::St);
        let motor = items.iter().find(|i| i.label == "MOTOR").unwrap();
        let Some(CompletionTextEdit::Edit(edit)) = &motor.text_edit else {
            panic!()
        };
        // The embedded editor's Position columns count Unicode scalar values.
        // Round-trip through its own conversion rather than assuming LSP UTF-16.
        let rope = Rope::from_str(text);
        assert_eq!(
            rope.position_to_offset(&edit.range.start),
            text.find("moTOR").unwrap()
        );
        assert_eq!(rope.position_to_offset(&edit.range.end), text.len());
        assert_eq!(edit.new_text, "MOTOR");
    }
    #[test]
    fn suggestions_are_suppressed_in_comments_and_strings() {
        assert!(completions("// MO", 5, SourceLanguage::St).is_empty());
        assert!(completions("'MO", 3, SourceLanguage::St).is_empty());
        assert!(completions("; LD", 4, SourceLanguage::Il).is_empty());
        assert!(
            completions("mo", 2, SourceLanguage::St)
                .iter()
                .any(|i| i.label == "MOTOR")
        );
    }
    #[test]
    fn completes_declared_variables_and_block_members_in_incomplete_source() {
        let source = "VAR start, stop AT X0 : BOOL; timer : TON; END_VAR\nsta";
        assert!(
            completions(source, source.len(), SourceLanguage::St)
                .iter()
                .any(|i| i.label == "start")
        );
        let source = "VAR timer : TON; END_VAR\ntimer.";
        let items = completions(source, source.len(), SourceLanguage::St);
        assert!(items.iter().any(|i| i.label == "Q"));
        assert!(items.iter().any(|i| i.label == "ET"));
        assert!(!items.iter().any(|i| i.label == "IF"));
        assert!(
            completions("ld", 2, SourceLanguage::Il)
                .iter()
                .any(|i| i.label == "LD")
        );
    }
}
