use super::{DataType, error};
use crate::plc::Diagnostic;

#[derive(Debug, Clone, Copy)]
pub(super) struct Pos {
    pub line: usize,
    pub col: usize,
}
impl Pos {
    pub fn err(self, message: impl Into<String>) -> Diagnostic {
        error(self.line, self.col, message)
    }
}
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Id(String),
    Num(String),
    Str(String),
    Sym(String),
    End,
}
#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
    pos: Pos,
}

/// Length of a `T#` / `TIME#` prefix starting at `i`, if there is one.
fn time_prefix(c: &[char], i: usize) -> Option<usize> {
    if i > 0 && (c[i - 1].is_ascii_alphanumeric() || c[i - 1] == '_') {
        return None;
    }
    let word: String = c[i..]
        .iter()
        .take(5)
        .collect::<String>()
        .to_ascii_uppercase();
    if word.starts_with("TIME#") {
        Some(5)
    } else if word.starts_with("T#") {
        Some(2)
    } else {
        None
    }
}
/// `2s500ms`, `1.5s`, `1h30m`, `100ms`, `1d` -> whole milliseconds.
fn parse_time(text: &str) -> Option<i64> {
    let c: Vec<char> = text.to_ascii_lowercase().chars().collect();
    let mut i = 0;
    let mut total = 0f64;
    if c.is_empty() {
        return None;
    }
    while i < c.len() {
        let start = i;
        while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.' || c[i] == '_') {
            i += 1;
        }
        let number: f64 = c[start..i]
            .iter()
            .filter(|x| **x != '_')
            .collect::<String>()
            .parse()
            .ok()?;
        let unit_start = i;
        while i < c.len() && c[i].is_ascii_alphabetic() {
            i += 1;
        }
        let unit: String = c[unit_start..i].iter().collect();
        total += number
            * match unit.as_str() {
                "d" => 86_400_000.0,
                "h" => 3_600_000.0,
                "m" => 60_000.0,
                "s" => 1000.0,
                "ms" => 1.0,
                _ => return None,
            };
    }
    let ms = total.round();
    ((total - ms).abs() < 1e-6 && (0.0..=2_000_000_000.0).contains(&ms)).then_some(ms as i64)
}

fn lex(source: &str) -> Result<Vec<Token>, Diagnostic> {
    let c: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut line = 1;
    let mut col = 1;
    let mut out = Vec::new();
    fn step(c: char, line: &mut usize, col: &mut usize) {
        if c == '\n' {
            *line += 1;
            *col = 1;
        } else {
            *col += 1;
        }
    }
    while i < c.len() {
        if out.len() > 50_000 {
            return Err(error(line, col, "Kaynak 50000 token sınırını aşıyor"));
        }
        if c[i].is_whitespace() {
            step(c[i], &mut line, &mut col);
            i += 1;
            continue;
        }
        let pos = Pos { line, col };
        if c[i] == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                step(c[i], &mut line, &mut col);
                i += 1;
            }
            continue;
        }
        if c[i] == '(' && c.get(i + 1) == Some(&'*') {
            let mut depth = 1;
            i += 2;
            col += 2;
            while i < c.len() && depth > 0 {
                if c[i] == '(' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                    col += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&')') {
                    depth -= 1;
                    i += 2;
                    col += 2;
                } else {
                    step(c[i], &mut line, &mut col);
                    i += 1;
                }
            }
            if depth != 0 {
                return Err(pos.err("Kapanmamış (* yorum *)"));
            }
            continue;
        }
        if matches!(c[i], '\'' | '"') {
            let quote = c[i];
            i += 1;
            col += 1;
            let mut closed = false;
            let mut text = String::new();
            while i < c.len() {
                let v = c[i];
                step(v, &mut line, &mut col);
                i += 1;
                if v == quote {
                    if c.get(i) == Some(&quote) {
                        i += 1;
                        col += 1;
                        text.push(quote);
                    } else {
                        closed = true;
                        break;
                    }
                } else {
                    text.push(v);
                }
            }
            if !closed {
                return Err(pos.err("Kapanmamış metin sabiti"));
            }
            out.push(Token {
                kind: Kind::Str(text),
                pos,
            });
            continue;
        }
        if let Some(prefix) = time_prefix(&c, i) {
            let start = i + prefix;
            let mut end = start;
            while end < c.len()
                && (c[end].is_ascii_alphanumeric() || c[end] == '.' || c[end] == '_')
            {
                end += 1;
            }
            let text: String = c[start..end].iter().collect();
            let ms = parse_time(&text).ok_or_else(|| {
                pos.err(format!(
                    "Süre sabiti geçersiz: T#{text}; örnek T#2s500ms, T#1h30m, T#100ms"
                ))
            })?;
            col += end - i;
            i = end;
            out.push(Token {
                kind: Kind::Num(ms.to_string()),
                pos,
            });
            continue;
        }
        if c[i].is_ascii_alphabetic() || c[i] == '_' || c[i] == '%' {
            let start = i;
            let address = c[i] == '%';
            i += 1;
            col += 1;
            while i < c.len()
                && (c[i].is_ascii_alphanumeric() || c[i] == '_' || address && c[i] == '.')
            {
                i += 1;
                col += 1;
            }
            out.push(Token {
                kind: Kind::Id(c[start..i].iter().collect::<String>().to_ascii_uppercase()),
                pos,
            });
            continue;
        }
        if c[i].is_ascii_digit() {
            let start = i;
            i += 1;
            col += 1;
            while i < c.len() {
                let accept = c[i].is_ascii_alphanumeric()
                    || c[i] == '_'
                    || c[i] == '#'
                    || c[i] == '.' && c.get(i + 1) != Some(&'.') && (i == start || c[i - 1] != '.')
                    || matches!(c[i], '+' | '-')
                        && matches!(c[i - 1], 'E' | 'e')
                        && !c[start..i].contains(&'#');
                if !accept {
                    break;
                }
                i += 1;
                col += 1;
            }
            out.push(Token {
                kind: Kind::Num(c[start..i].iter().collect::<String>().to_ascii_uppercase()),
                pos,
            });
            continue;
        }
        let pair = c.get(i + 1).map(|b| format!("{}{b}", c[i]));
        if let Some(pair) = pair.filter(|s| matches!(s.as_str(), ":=" | "<=" | ">=" | "<>" | ".."))
        {
            i += 2;
            col += 2;
            out.push(Token {
                kind: Kind::Sym(pair),
                pos,
            });
            continue;
        }
        if ";:,()[]+-*/=<> &.".contains(c[i]) {
            let value = c[i].to_string();
            i += 1;
            col += 1;
            out.push(Token {
                kind: Kind::Sym(value),
                pos,
            });
            continue;
        }
        return Err(pos.err(format!("Desteklenmeyen karakter: {}", c[i])));
    }
    out.push(Token {
        kind: Kind::End,
        pos: Pos { line, col },
    });
    Ok(out)
}

#[derive(Debug, Clone)]
pub(super) struct Expr {
    pub kind: ExprKind,
    pub pos: Pos,
}
#[derive(Debug, Clone)]
pub(super) enum ExprKind {
    Number(String),
    Bool(bool),
    Name(String),
    /// `name[index]` read of an `ARRAY` element.
    Index(String, Box<Expr>),
    Unary(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    /// `instance.member`, e.g. `timer.Q`.
    Member(String, String),
    /// `'text'`
    Str(String),
    /// `[a, b, 3(0)]` array initializer (only in declarations).
    List(Vec<Expr>),
    /// `NAME(args)`; arguments are positional or `param := value`.
    Call(String, Vec<Arg>),
}
pub(super) type Arg = (Option<String>, Expr);
/// Standard function blocks backed by the target's timers and counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FbKind {
    Ton,
    Tof,
    Tp,
    Ctu,
    Ctd,
    Ctud,
    /// On-delay with PT in 100 ms ticks and ET = the timer's own count (INT); one native
    /// `TMR`. Library-internal (`TON_DSEC` is built on it).
    TonTicks,
}
impl FbKind {
    pub(super) fn is_timer(self) -> bool {
        matches!(self, Self::Ton | Self::Tof | Self::Tp | Self::TonTicks)
    }
    /// Kinds that run a native `TMR` / `CNT`, which cannot be index-addressed: in a shared
    /// block body each of their calls is a gap filled per instance.
    pub(super) fn needs_device(self) -> bool {
        self.is_timer() || self == Self::Ctu
    }
    pub(super) fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "TON_TICKS" => Self::TonTicks,
            "TON" => Self::Ton,
            "TOF" => Self::Tof,
            "TP" => Self::Tp,
            "CTU" => Self::Ctu,
            "CTD" => Self::Ctd,
            "CTUD" => Self::Ctud,
            _ => return None,
        })
    }
}
#[derive(Debug, Clone)]
pub(super) struct Decl {
    pub names: Vec<(String, Pos)>,
    pub mapping: Option<(String, Pos)>,
    pub ty: DataType,
    pub init: Option<Expr>,
    /// Declared in `VAR_RETAIN` (or `VAR RETAIN`).
    pub retain: bool,
    /// `ARRAY[lo..hi] OF ty`: the unevaluated bound expressions.
    pub array: Option<(Expr, Expr)>,
    /// `name : TON;` — a function-block instance instead of a plain variable.
    pub fb: Option<FbKind>,
    /// `STRING[n]`: capacity in characters (stored as an INT array: length word + packed bytes).
    pub string: Option<usize>,
    /// Name of a `TYPE ... STRUCT` the variable is an instance of (expanded after parsing).
    pub strukt: Option<String>,
}
#[derive(Debug, Clone)]
pub(super) struct Stmt {
    pub kind: StmtKind,
    pub pos: Pos,
}
pub(super) type CaseArms = Vec<(Vec<(Expr, Option<Expr>)>, Vec<Stmt>)>;
#[derive(Debug, Clone)]
pub(super) enum StmtKind {
    Assign(String, Expr),
    /// `name[index] := value;`
    AssignIndex(String, Expr, Expr),
    If(Vec<(Expr, Vec<Stmt>)>, Vec<Stmt>),
    Case(Expr, CaseArms, Vec<Stmt>),
    For(String, Expr, Expr, Option<Expr>, Vec<Stmt>),
    While(Expr, Vec<Stmt>),
    Repeat(Vec<Stmt>, Expr),
    Exit,
    Return,
    /// `instance(IN := x, PT := T#2s);`
    Call(String, Vec<Arg>),
    /// Run the shared body of block type `.1` for instance `.0` (inputs already assigned).
    Shared(String, String),
}
/// The top-level instances of one block type, sharing a single body (see `blocks.rs`).
#[derive(Debug, Clone)]
pub(super) struct SharedBlock {
    pub block: String,
    /// Name of the template instance the body is written for (`__S_<BLOCK>`).
    pub template: String,
    /// The template's variables, flattened (`__S_<BLOCK>.member`).
    pub members: Vec<Decl>,
    pub body: Vec<Stmt>,
    /// The instances, in declaration order.
    pub instances: Vec<String>,
}
/// A user `FUNCTION_BLOCK` (stateful, one instance per variable) or `FUNCTION` (returns a value).
#[derive(Debug, Clone)]
pub(super) struct FbDef {
    pub decls: Vec<Decl>,
    /// `VAR_INPUT` names in declaration order (positional call arguments follow it).
    pub inputs: Vec<String>,
    pub body: Vec<Stmt>,
    /// `Some` for a `FUNCTION`: the result type (the result variable carries the function's name).
    pub returns: Option<DataType>,
    /// `VAR_OUTPUT` names in declaration order.
    pub outputs: Vec<String>,
    /// Defined by the built-in standard library: inlined code reports the call-site line.
    pub library: bool,
}
/// Parsed standard-library definitions every ST program can use.
#[derive(Debug, Clone, Default)]
pub(super) struct LibraryDefs {
    pub types: std::collections::BTreeMap<String, Vec<Decl>>,
    pub fbs: std::collections::BTreeMap<String, FbDef>,
}
pub(super) struct Unit {
    pub decls: Vec<Decl>,
    pub body: Vec<Stmt>,
    pub direct_names: Vec<(String, Pos)>,
    pub shared: Vec<SharedBlock>,
}
struct Parser {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
    expr_level: usize,
    expr_nodes: usize,
    types: std::collections::BTreeMap<String, Vec<Decl>>,
    fbs: std::collections::BTreeMap<String, FbDef>,
    /// Library names a program may redefine (its own definition then wins).
    overridable: std::collections::BTreeSet<String>,
    /// Parsing a library file: definitions are marked as library code.
    in_library: bool,
}
impl Parser {
    fn new(tokens: Vec<Token>, lib: &LibraryDefs, in_library: bool) -> Self {
        let overridable = if in_library {
            Default::default()
        } else {
            lib.types.keys().chain(lib.fbs.keys()).cloned().collect()
        };
        Self {
            tokens,
            at: 0,
            depth: 0,
            expr_level: 0,
            expr_nodes: 0,
            types: lib.types.clone(),
            fbs: lib.fbs.clone(),
            overridable,
            in_library,
        }
    }
    /// A program definition replacing a library one of the same name.
    fn claim(&mut self, name: &str) {
        if self.overridable.remove(name) {
            self.types.remove(name);
            self.fbs.remove(name);
        }
    }
    fn tok(&self) -> &Token {
        &self.tokens[self.at]
    }
    fn is(&self, s: &str) -> bool {
        matches!(&self.tok().kind,Kind::Id(x)|Kind::Sym(x) if x==s)
    }
    fn take(&mut self, s: &str) -> bool {
        if self.is(s) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, s: &str) -> Result<(), Diagnostic> {
        if self.take(s) {
            Ok(())
        } else {
            Err(self.tok().pos.err(format!("{s} bekleniyor")))
        }
    }
    fn ident(&mut self) -> Result<(String, Pos), Diagnostic> {
        match self.tok().clone() {
            Token {
                kind: Kind::Id(s),
                pos,
            } => {
                self.at += 1;
                Ok((s, pos))
            }
            t => Err(t.pos.err("Ad veya adres bekleniyor")),
        }
    }
    fn expr(&mut self, min: u8) -> Result<Expr, Diagnostic> {
        if self.expr_level == 0 {
            self.expr_nodes = 0;
        }
        self.expr_level += 1;
        self.expr_nodes += 1;
        if self.expr_nodes > 128 {
            return Err(self.tok().pos.err("Bir ifade en fazla128 düğüm içerebilir"));
        }
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.tok().pos.err("İfade iç içe sınırı128 aşıldı"));
        }
        let t = self.tok().clone();
        let mut left = if self.take("(") {
            let e = self.expr(0)?;
            self.expect(")")?;
            e
        } else if self.take("NOT") || self.take("-") || self.take("+") {
            let op = match t.kind {
                Kind::Id(s) | Kind::Sym(s) => s,
                _ => unreachable!(),
            };
            Expr {
                kind: ExprKind::Unary(op, Box::new(self.expr(8)?)),
                pos: t.pos,
            }
        } else {
            self.at += 1;
            match t.kind {
                Kind::Num(n) => Expr {
                    kind: ExprKind::Number(n),
                    pos: t.pos,
                },
                Kind::Id(n) if n == "TRUE" || n == "FALSE" => Expr {
                    kind: ExprKind::Bool(n == "TRUE"),
                    pos: t.pos,
                },
                Kind::Id(n) => {
                    if self.take("(") {
                        let args = self.args()?;
                        Expr {
                            kind: ExprKind::Call(n, args),
                            pos: t.pos,
                        }
                    } else if self.is(".") {
                        let mut full = n;
                        let mut last = None;
                        while self.take(".") {
                            let (member, _) = self.ident()?;
                            last = Some((full.clone(), member.clone()));
                            full = format!("{full}.{member}");
                        }
                        if self.take("[") {
                            let index = self.expr(0)?;
                            self.expect("]")?;
                            Expr {
                                kind: ExprKind::Index(full, Box::new(index)),
                                pos: t.pos,
                            }
                        } else {
                            let (prefix, member) = last.expect("one member");
                            Expr {
                                kind: ExprKind::Member(prefix, member),
                                pos: t.pos,
                            }
                        }
                    } else if self.take("[") {
                        let index = self.expr(0)?;
                        self.expect("]")?;
                        Expr {
                            kind: ExprKind::Index(n, Box::new(index)),
                            pos: t.pos,
                        }
                    } else {
                        Expr {
                            kind: ExprKind::Name(n),
                            pos: t.pos,
                        }
                    }
                }
                Kind::Str(text) => Expr {
                    kind: ExprKind::Str(text),
                    pos: t.pos,
                },
                _ => return Err(t.pos.err("İfade bekleniyor")),
            }
        };
        while let Kind::Id(s) | Kind::Sym(s) = &self.tok().kind {
            let op = s.clone();
            let prec = match op.as_str() {
                "OR" => 1,
                "XOR" => 2,
                "AND" | "&" => 3,
                "=" | "<>" => 4,
                "<" | ">" | "<=" | ">=" => 5,
                "+" | "-" => 6,
                "*" | "/" | "MOD" => 7,
                _ => break,
            };
            if prec < min {
                break;
            }
            self.expr_nodes += 1;
            if self.expr_nodes > 128 {
                return Err(self.tok().pos.err("Bir ifade en fazla128 düğüm içerebilir"));
            }
            let p = self.tok().pos;
            self.at += 1;
            let right = self.expr(prec + 1)?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                pos: p,
            };
        }
        self.depth -= 1;
        self.expr_level -= 1;
        Ok(left)
    }
    /// `[1, 2, 3(0)]`: values, with `n(v)` repeating v n times.
    fn array_init(&mut self) -> Result<Expr, Diagnostic> {
        let pos = self.tok().pos;
        self.expect("[")?;
        let mut items = Vec::new();
        loop {
            let e = self.expr(0)?;
            if self.take("(") {
                let times = match &e.kind {
                    ExprKind::Number(n) => n.parse::<usize>().ok(),
                    _ => None,
                }
                .filter(|n| (1..=4096).contains(n))
                .ok_or_else(|| e.pos.err("Tekrar sayısı 1..4096 arası bir sabit olmalı"))?;
                let v = self.expr(0)?;
                self.expect(")")?;
                for _ in 0..times {
                    items.push(v.clone());
                }
            } else {
                items.push(e);
            }
            if !self.take(",") {
                break;
            }
        }
        self.expect("]")?;
        Ok(Expr {
            kind: ExprKind::List(items),
            pos,
        })
    }
    /// `TYPE name : STRUCT fields END_STRUCT END_TYPE` (after `TYPE`).
    fn struct_type(&mut self) -> Result<(), Diagnostic> {
        let (name, p) = self.ident()?;
        self.claim(&name);
        if reserved(&name) || self.types.contains_key(&name) {
            return Err(p.err(format!("Tür adı geçersiz veya yinelenen: {name}")));
        }
        self.expect(":")?;
        self.expect("STRUCT")?;
        let mut fields = Vec::new();
        while !self.is("END_STRUCT") {
            if self.tok().kind == Kind::End {
                return Err(self.tok().pos.err("END_STRUCT bekleniyor"));
            }
            fields.push(self.decl(false)?);
        }
        self.expect("END_STRUCT")?;
        self.take(";");
        self.expect("END_TYPE")?;
        self.take(";");
        self.types.insert(name, fields);
        Ok(())
    }
    /// `FUNCTION_BLOCK name ... END_FUNCTION_BLOCK` or `FUNCTION name : TYPE ... END_FUNCTION`.
    fn fb_def(&mut self, function: bool) -> Result<(), Diagnostic> {
        let (name, p) = self.ident()?;
        self.claim(&name);
        if reserved(&name) || self.types.contains_key(&name) || self.fbs.contains_key(&name) {
            return Err(p.err(format!("Blok adı geçersiz veya yinelenen: {name}")));
        }
        let returns = if function {
            self.expect(":")?;
            Some(self.data_type()?)
        } else {
            None
        };
        let mut decls = Vec::new();
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        if let Some(ty) = returns {
            decls.push(Decl {
                names: vec![(name.clone(), p)],
                mapping: None,
                ty,
                init: None,
                retain: false,
                array: None,
                fb: None,
                string: None,
                strukt: None,
            });
        }
        while self.is("VAR")
            || self.is("VAR_INPUT")
            || self.is("VAR_OUTPUT")
            || self.is("VAR_RETAIN")
        {
            let section_pos = self.tok().pos;
            let input = self.is("VAR_INPUT");
            let output = self.is("VAR_OUTPUT");
            let retain = self.is("VAR_RETAIN");
            self.at += 1;
            let retain = self.take("RETAIN") || retain;
            while !self.is("END_VAR") {
                if self.tok().kind == Kind::End {
                    return Err(self.tok().pos.err("END_VAR bekleniyor"));
                }
                let d = self.decl(retain)?;
                if d.mapping.is_some() {
                    return Err(d.names[0].1.err("Blok değişkeni AT ile eşlenemez"));
                }
                if input {
                    if d.fb.is_some() || d.strukt.is_some() || d.array.is_some() {
                        return Err(section_pos.err("VAR_INPUT yalnız basit tipler alır"));
                    }
                    inputs.extend(d.names.iter().map(|(n, _)| n.clone()));
                }
                if output {
                    outputs.extend(d.names.iter().map(|(n, _)| n.clone()));
                }
                decls.push(d);
            }
            self.expect("END_VAR")?;
            self.take(";");
        }
        if self.is("VAR_IN_OUT") {
            return Err(self
                .tok()
                .pos
                .err("VAR_IN_OUT desteklenmiyor; VAR_INPUT / VAR_OUTPUT kullanın"));
        }
        let end = if function {
            "END_FUNCTION"
        } else {
            "END_FUNCTION_BLOCK"
        };
        let body = self.block(&[end], false)?;
        self.expect(end)?;
        self.take(";");
        if !function {
            self.types.insert(name.clone(), decls.clone());
        }
        self.fbs.insert(
            name,
            FbDef {
                decls,
                inputs,
                body,
                returns,
                outputs,
                library: self.in_library,
            },
        );
        Ok(())
    }
    /// Arguments after `(`; consumes the closing `)`.
    fn args(&mut self) -> Result<Vec<Arg>, Diagnostic> {
        let mut args = Vec::new();
        if self.take(")") {
            return Ok(args);
        }
        loop {
            let named = matches!(&self.tokens[self.at].kind, Kind::Id(_))
                && matches!(&self.tokens[self.at + 1].kind, Kind::Sym(s) if s == ":=");
            let name = if named {
                let (n, _) = self.ident()?;
                self.expect(":=")?;
                Some(n)
            } else {
                None
            };
            args.push((name, self.expr(0)?));
            if self.take(",") {
                continue;
            }
            self.expect(")")?;
            return Ok(args);
        }
    }
    fn data_type(&mut self) -> Result<DataType, Diagnostic> {
        let (ty, p) = self.ident()?;
        Ok(match ty.as_str() {
            "BOOL" => DataType::Bool,
            "INT" => DataType::Int,
            "DINT" => DataType::Dint,
            "WORD" => DataType::Word,
            "DWORD" => DataType::Dword,
            "REAL" => DataType::Real,
            _ => return Err(p.err(format!("Desteklenmeyen veri tipi: {ty}"))),
        })
    }
    fn decl(&mut self, retain: bool) -> Result<Decl, Diagnostic> {
        let mut names = vec![self.ident()?];
        while self.take(",") {
            names.push(self.ident()?);
        }
        let mapping = if self.take("AT") {
            if names.len() != 1 {
                return Err(names[0].1.err("AT tek değişkene uygulanır"));
            }
            Some(self.ident()?)
        } else {
            None
        };
        self.expect(":")?;
        let mut array = if self.take("ARRAY") {
            self.expect("[")?;
            let lo = self.expr(0)?;
            self.expect("..")?;
            let hi = self.expr(0)?;
            self.expect("]")?;
            self.expect("OF")?;
            Some((lo, hi))
        } else {
            None
        };
        let fb = match &self.tok().kind {
            Kind::Id(n) => FbKind::parse(n).filter(|k| *k != FbKind::TonTicks || self.in_library),
            _ => None,
        };
        if fb.is_some() {
            if array.is_some() {
                return Err(self.tok().pos.err("Fonksiyon bloğu dizisi desteklenmiyor"));
            }
            self.at += 1;
        }
        let mut strukt = None;
        let mut string = None;
        if fb.is_none() {
            let word = match &self.tok().kind {
                Kind::Id(n) => Some(n.clone()),
                _ => None,
            };
            if let Some(word) = word {
                let here = self.tok().pos;
                if self.types.contains_key(&word) {
                    if array.is_some() {
                        return Err(here.err("Yapı dizisi desteklenmiyor"));
                    }
                    self.at += 1;
                    strukt = Some(word);
                } else if word == "STRING" {
                    if array.is_some() {
                        return Err(here.err("STRING dizisi desteklenmiyor"));
                    }
                    self.at += 1;
                    let cap = if self.take("[") || self.take("(") {
                        let at = self.tok().pos;
                        let digits = match &self.tok().kind {
                            Kind::Num(n) => n.parse::<usize>().ok(),
                            _ => None,
                        };
                        self.at += 1;
                        let cap = digits.filter(|n| (1..=250).contains(n)).ok_or_else(|| {
                            at.err("STRING uzunluğu 1..250 arası bir sabit olmalı")
                        })?;
                        if !self.take("]") {
                            self.expect(")")?;
                        }
                        cap
                    } else {
                        32
                    };
                    string = Some(cap);
                    let number = |n: usize| Expr {
                        kind: ExprKind::Number(n.to_string()),
                        pos: here,
                    };
                    array = Some((number(0), number(cap.div_ceil(2))));
                }
            }
        }
        let ty = if fb.is_some() || strukt.is_some() || string.is_some() {
            DataType::Int
        } else {
            self.data_type()?
        };
        let init = if self.take(":=") {
            if self.is("[") {
                Some(self.array_init()?)
            } else {
                Some(self.expr(0)?)
            }
        } else {
            None
        };
        self.expect(";")?;
        Ok(Decl {
            names,
            mapping,
            ty,
            init,
            retain,
            array,
            fb,
            string,
            strukt,
        })
    }
    fn block(&mut self, stops: &[&str], case: bool) -> Result<Vec<Stmt>, Diagnostic> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(self.tok().pos.err("Blok iç içe sınırı128 aşıldı"));
        }
        let mut result = Vec::new();
        while !stops.iter().any(|s| self.is(s)) && !(case && self.case_label()) {
            if self.tok().kind == Kind::End {
                return Err(self
                    .tok()
                    .pos
                    .err(format!("Kapanmamış blok; {} bekleniyor", stops.join(" / "))));
            }
            if self.take(";") {
                continue;
            }
            result.push(self.stmt()?);
        }
        self.depth -= 1;
        Ok(result)
    }
    fn case_label(&self) -> bool {
        if !matches!(&self.tok().kind, Kind::Num(_)) && !self.is("-") && !self.is("+") {
            return false;
        }
        self.tokens[self.at..]
            .iter()
            .take(64)
            .take_while(|t| !matches!(&t.kind,Kind::Sym(s)if s==";"||s==":="))
            .any(|t| matches!(&t.kind,Kind::Sym(s)if s==":"))
    }
    fn stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let pos = self.tok().pos;
        let kind = if self.take("IF") {
            let e = self.expr(0)?;
            self.expect("THEN")?;
            let mut arms = vec![(e, self.block(&["ELSIF", "ELSE", "END_IF"], false)?)];
            while self.take("ELSIF") {
                let e = self.expr(0)?;
                self.expect("THEN")?;
                arms.push((e, self.block(&["ELSIF", "ELSE", "END_IF"], false)?));
            }
            let other = if self.take("ELSE") {
                self.block(&["END_IF"], false)?
            } else {
                vec![]
            };
            self.expect("END_IF")?;
            self.expect(";")?;
            StmtKind::If(arms, other)
        } else if self.take("CASE") {
            let value = self.expr(0)?;
            self.expect("OF")?;
            let mut arms = Vec::new();
            while !self.is("ELSE") && !self.is("END_CASE") {
                let mut labels = Vec::new();
                loop {
                    let a = self.expr(0)?;
                    let b = if self.take("..") {
                        Some(self.expr(0)?)
                    } else {
                        None
                    };
                    labels.push((a, b));
                    if !self.take(",") {
                        break;
                    }
                }
                self.expect(":")?;
                arms.push((labels, self.block(&["ELSE", "END_CASE"], true)?));
            }
            let other = if self.take("ELSE") {
                self.block(&["END_CASE"], false)?
            } else {
                vec![]
            };
            self.expect("END_CASE")?;
            self.expect(";")?;
            StmtKind::Case(value, arms, other)
        } else if self.take("FOR") {
            let (name, _) = self.ident()?;
            self.expect(":=")?;
            let start = self.expr(0)?;
            self.expect("TO")?;
            let end = self.expr(0)?;
            let step = if self.take("BY") {
                Some(self.expr(0)?)
            } else {
                None
            };
            self.expect("DO")?;
            let body = self.block(&["END_FOR"], false)?;
            self.expect("END_FOR")?;
            self.expect(";")?;
            StmtKind::For(name, start, end, step, body)
        } else if self.take("WHILE") {
            let condition = self.expr(0)?;
            self.expect("DO")?;
            let body = self.block(&["END_WHILE"], false)?;
            self.expect("END_WHILE")?;
            self.expect(";")?;
            StmtKind::While(condition, body)
        } else if self.take("REPEAT") {
            let body = self.block(&["UNTIL"], false)?;
            self.expect("UNTIL")?;
            let condition = self.expr(0)?;
            self.expect("END_REPEAT")?;
            self.expect(";")?;
            StmtKind::Repeat(body, condition)
        } else if self.take("EXIT") {
            self.expect(";")?;
            StmtKind::Exit
        } else if self.take("RETURN") {
            self.expect(";")?;
            StmtKind::Return
        } else {
            let (name, p) = self.ident()?;
            if reserved(&name) {
                return Err(p.err(format!("Bu ST profilinde yapı desteklenmiyor: {name}")));
            }
            let mut name = name;
            while self.take(".") {
                let (member, _) = self.ident()?;
                name = format!("{name}.{member}");
            }
            if self.take("(") {
                let args = self.args()?;
                self.expect(";")?;
                return Ok(Stmt {
                    kind: StmtKind::Call(name, args),
                    pos,
                });
            }
            let index = if self.take("[") {
                let index = self.expr(0)?;
                self.expect("]")?;
                Some(index)
            } else {
                None
            };
            self.expect(":=")?;
            let value = self.expr(0)?;
            self.expect(";")?;
            match index {
                Some(index) => StmtKind::AssignIndex(name, index, value),
                None => StmtKind::Assign(name, value),
            }
        };
        Ok(Stmt { kind, pos })
    }
}
pub(super) fn reserved(s: &str) -> bool {
    matches!(
        s,
        "PROGRAM"
            | "END_PROGRAM"
            | "VAR"
            | "END_VAR"
            | "VAR_INPUT"
            | "VAR_OUTPUT"
            | "VAR_IN_OUT"
            | "VAR_RETAIN"
            | "ARRAY"
            | "RETAIN"
            | "PERSISTENT"
            | "CONSTANT"
            | "TON"
            | "TOF"
            | "TP"
            | "CTU"
            | "CTD"
            | "CTUD"
            | "STRING"
            | "TYPE"
            | "END_TYPE"
            | "STRUCT"
            | "END_STRUCT"
            | "FUNCTION"
            | "END_FUNCTION"
            | "FUNCTION_BLOCK"
            | "END_FUNCTION_BLOCK"
            | "IF"
            | "THEN"
            | "ELSIF"
            | "ELSE"
            | "END_IF"
            | "CASE"
            | "OF"
            | "END_CASE"
            | "FOR"
            | "TO"
            | "BY"
            | "DO"
            | "END_FOR"
            | "WHILE"
            | "END_WHILE"
            | "REPEAT"
            | "UNTIL"
            | "END_REPEAT"
            | "EXIT"
            | "RETURN"
            | "TRUE"
            | "FALSE"
            | "AND"
            | "OR"
            | "XOR"
            | "NOT"
            | "MOD"
            | "AT"
            | "BOOL"
            | "INT"
            | "DINT"
            | "WORD"
            | "DWORD"
            | "REAL"
    )
}
pub(super) fn parse(
    source: &str,
    share: super::Share,
    exclude: &std::collections::BTreeSet<String>,
) -> Result<Unit, Diagnostic> {
    let tokens = lex(source)?;
    let direct_names = tokens
        .iter()
        .filter_map(|t| {
            if let Kind::Id(s) = &t.kind {
                Some((s.clone(), t.pos))
            } else {
                None
            }
        })
        .collect();
    let mut p = Parser::new(tokens, super::library::defs(), false);
    loop {
        if p.take("TYPE") {
            p.struct_type()?;
        } else if p.take("FUNCTION_BLOCK") {
            p.fb_def(false)?;
        } else if p.take("FUNCTION") {
            p.fb_def(true)?;
        } else {
            break;
        }
    }
    let wrapped = p.take("PROGRAM");
    if wrapped
        && matches!(&p.tok().kind,Kind::Id(n)if !reserved(n))
        && !matches!(p.tokens.get(p.at+1).map(|t|&t.kind),Some(Kind::Sym(s))if s==":=")
    {
        let _ = p.ident()?;
    }
    let mut decls = Vec::new();
    // `VAR_RETAIN` (also spelled `VAR RETAIN`) declares latched, power-fail retentive variables.
    while p.is("VAR") || p.is("VAR_RETAIN") || p.is("TYPE") {
        if p.take("TYPE") {
            p.struct_type()?;
            continue;
        }
        let retain = p.is("VAR_RETAIN");
        p.at += 1;
        let retain = p.take("RETAIN") || retain;
        while !p.is("END_VAR") {
            if p.tok().kind == Kind::End {
                return Err(p.tok().pos.err("END_VAR bekleniyor"));
            }
            decls.push(p.decl(retain)?);
        }
        p.expect("END_VAR")?;
        p.take(";");
    }
    let body = if wrapped {
        let b = p.block(&["END_PROGRAM"], false)?;
        p.expect("END_PROGRAM")?;
        p.take(";");
        b
    } else {
        let mut b = vec![];
        while p.tok().kind != Kind::End {
            if p.take(";") {
                continue;
            }
            b.push(p.stmt()?);
        }
        b
    };
    if p.tok().kind != Kind::End {
        return Err(p.tok().pos.err("END_PROGRAM sonrasında beklenmeyen kaynak"));
    }
    let (mut decls, body, mut shared) =
        super::blocks::inline(decls, body, &p.types, &p.fbs, share, exclude)?;
    decls = expand_structs(decls, &p.types, 0)?;
    for block in &mut shared {
        block.members = expand_structs(std::mem::take(&mut block.members), &p.types, 0)?;
    }
    Ok(Unit {
        decls,
        body,
        direct_names,
        shared,
    })
}
/// Parses one standard-library file (only `TYPE`, `FUNCTION_BLOCK` and `FUNCTION`)
/// on top of `lib`, which receives the new definitions only if the whole file parses.
pub(super) fn parse_library(source: &str, lib: &mut LibraryDefs) -> Result<(), Diagnostic> {
    let mut p = Parser::new(lex(source)?, lib, true);
    loop {
        if p.take("TYPE") {
            p.struct_type()?;
        } else if p.take("FUNCTION_BLOCK") {
            p.fb_def(false)?;
        } else if p.take("FUNCTION") {
            p.fb_def(true)?;
        } else if p.tok().kind == Kind::End {
            break;
        } else {
            return Err(p
                .tok()
                .pos
                .err("Kütüphane dosyası yalnız TYPE / FUNCTION_BLOCK / FUNCTION içerebilir"));
        }
    }
    lib.types = p.types;
    lib.fbs = p.fbs;
    Ok(())
}
pub(super) fn is_st_source(source: &str) -> bool {
    // Inspect only the prefix: an invalid character later in a PROGRAM must
    // still receive an ST diagnostic rather than silently selecting IL.
    fn skip(c: &[char], i: &mut usize) -> bool {
        loop {
            while c.get(*i).is_some_and(|x| x.is_whitespace()) {
                *i += 1;
            }
            if c.get(*i) == Some(&'/') && c.get(*i + 1) == Some(&'/') {
                while c.get(*i).is_some_and(|x| *x != '\n') {
                    *i += 1;
                }
                continue;
            }
            if c.get(*i) == Some(&'(') && c.get(*i + 1) == Some(&'*') {
                *i += 2;
                let mut depth = 1usize;
                while *i < c.len() && depth > 0 {
                    if c.get(*i) == Some(&'(') && c.get(*i + 1) == Some(&'*') {
                        depth += 1;
                        *i += 2;
                    } else if c.get(*i) == Some(&'*') && c.get(*i + 1) == Some(&')') {
                        depth -= 1;
                        *i += 2;
                    } else {
                        *i += 1;
                    }
                }
                if depth > 0 {
                    return false;
                }
                continue;
            }
            return true;
        }
    }
    let c: Vec<char> = source.chars().collect();
    let mut i = 0;
    if !skip(&c, &mut i)
        || !c
            .get(i)
            .is_some_and(|x| x.is_ascii_alphabetic() || matches!(*x, '_' | '%'))
    {
        return false;
    }
    let start = i;
    i += 1;
    while c
        .get(i)
        .is_some_and(|x| x.is_ascii_alphanumeric() || matches!(*x, '_' | '.'))
    {
        i += 1;
    }
    let name = c[start..i].iter().collect::<String>().to_ascii_uppercase();
    if matches!(
        name.as_str(),
        "PROGRAM" | "TYPE" | "VAR" | "FUNCTION_BLOCK" | "FUNCTION"
    ) || name.starts_with("VAR_")
    {
        return true;
    }
    skip(&c, &mut i) && c.get(i) == Some(&':') && c.get(i + 1) == Some(&'=')
}

/// Struct variables become one ordinary variable per field, named `instance.field`.
fn expand_structs(
    decls: Vec<Decl>,
    types: &std::collections::BTreeMap<String, Vec<Decl>>,
    depth: usize,
) -> Result<Vec<Decl>, Diagnostic> {
    let mut out = Vec::new();
    for d in decls {
        let Some(type_name) = d.strukt.clone() else {
            out.push(d);
            continue;
        };
        let first = d.names[0].1;
        if depth > 8 {
            return Err(first.err("Yapılar en çok 8 düzey iç içe olabilir"));
        }
        if d.mapping.is_some() || d.init.is_some() {
            return Err(
                first.err("Yapı değişkeni AT veya başlangıç değeri almaz; alanlarda tanımlayın")
            );
        }
        for (name, p) in &d.names {
            let mut fields = Vec::new();
            for field in &types[&type_name] {
                if field.mapping.is_some() {
                    return Err(p.err("Yapı alanı AT ile eşlenemez"));
                }
                let mut f = field.clone();
                f.names = f
                    .names
                    .iter()
                    .map(|(n, _)| (format!("{name}.{n}"), *p))
                    .collect();
                f.retain |= d.retain;
                fields.push(f);
            }
            out.extend(expand_structs(fields, types, depth + 1)?);
        }
    }
    Ok(out)
}
