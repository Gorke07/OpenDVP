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
    String,
    Sym(String),
    End,
}
#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
    pos: Pos,
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
            while i < c.len() {
                let v = c[i];
                step(v, &mut line, &mut col);
                i += 1;
                if v == quote {
                    if c.get(i) == Some(&quote) {
                        i += 1;
                        col += 1;
                    } else {
                        closed = true;
                        break;
                    }
                }
            }
            if !closed {
                return Err(pos.err("Kapanmamış metin sabiti"));
            }
            out.push(Token {
                kind: Kind::String,
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
        if ";:,()[]+-*/=<> &".contains(c[i]) {
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
}
pub(super) struct Unit {
    pub decls: Vec<Decl>,
    pub body: Vec<Stmt>,
    pub direct_names: Vec<(String, Pos)>,
}
struct Parser {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
    expr_level: usize,
    expr_nodes: usize,
}
impl Parser {
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
                    if self.is("(") {
                        return Err(t.pos.err(format!(
                            "Fonksiyon/FB çağrısı bu ST profilinde desteklenmiyor: {n}"
                        )));
                    }
                    if self.take("[") {
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
                Kind::String => return Err(t.pos.err("STRING bu ST profilinde desteklenmiyor")),
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
        let array = if self.take("ARRAY") {
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
        let ty = self.data_type()?;
        let init = if self.take(":=") {
            Some(self.expr(0)?)
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
pub(super) fn parse(source: &str) -> Result<Unit, Diagnostic> {
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
    let mut p = Parser {
        tokens,
        at: 0,
        depth: 0,
        expr_level: 0,
        expr_nodes: 0,
    };
    let wrapped = p.take("PROGRAM");
    if wrapped
        && matches!(&p.tok().kind,Kind::Id(n)if !reserved(n))
        && !matches!(p.tokens.get(p.at+1).map(|t|&t.kind),Some(Kind::Sym(s))if s==":=")
    {
        let _ = p.ident()?;
    }
    let mut decls = Vec::new();
    // `VAR_RETAIN` (also spelled `VAR RETAIN`) declares latched, power-fail retentive variables.
    while p.is("VAR") || p.is("VAR_RETAIN") {
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
    Ok(Unit {
        decls,
        body,
        direct_names,
    })
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
    if name == "PROGRAM" || name == "VAR" || name.starts_with("VAR_") {
        return true;
    }
    skip(&c, &mut i) && c.get(i) == Some(&':') && c.get(i + 1) == Some(&'=')
}
