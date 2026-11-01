use super::syntax::{Decl, Expr, ExprKind, Pos, Stmt, StmtKind, Unit, reserved};
use super::{DataType as Ty, LoweredSt, StSymbol};
use crate::plc::{Area, Device, Diagnostic};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
enum Repr {
    Num(String),
    Bit(String, bool),
    Bool(bool),
}
#[derive(Clone)]
struct Value {
    ty: Ty,
    repr: Repr,
}
impl Value {
    fn num(ty: Ty, operand: impl Into<String>) -> Self {
        Self {
            ty,
            repr: Repr::Num(operand.into()),
        }
    }
    fn bit(address: impl Into<String>) -> Self {
        Self {
            ty: Ty::Bool,
            repr: Repr::Bit(address.into(), false),
        }
    }
    fn operand(&self) -> &str {
        match &self.repr {
            Repr::Num(s) => s,
            _ => unreachable!("typed numeric operand"),
        }
    }
    fn inverted(mut self) -> Self {
        match &mut self.repr {
            Repr::Bit(_, inv) => *inv = !*inv,
            Repr::Bool(b) => *b = !*b,
            _ => unreachable!(),
        };
        self
    }
}
struct Pool {
    words: BTreeSet<u16>,
    bits: BTreeSet<u16>,
    temps: Vec<(bool, u16)>,
    alloc: crate::alloc::Alloc,
    retain: crate::alloc::Retain,
}
impl Pool {
    fn new(alloc: crate::alloc::Alloc, retain: crate::alloc::Retain) -> Self {
        Self {
            words: BTreeSet::new(),
            bits: BTreeSet::new(),
            temps: vec![],
            alloc,
            retain,
        }
    }
    fn reserve(&mut self, d: Device, width: usize) {
        if d.area == Area::D {
            for i in 0..width {
                self.words.insert(d.index + i as u16);
            }
        } else if d.area == Area::M {
            self.bits.insert(d.index);
        }
    }
    fn allocate(&mut self, ty: Ty, width: usize, temp: bool, p: Pos) -> Result<String, Diagnostic> {
        self.allocate_in(ty, width, temp, false, p)
    }
    /// `retain` selects the latched `VAR_RETAIN` range instead of the ordinary local pool.
    fn allocate_in(
        &mut self,
        ty: Ty,
        width: usize,
        temp: bool,
        retain: bool,
        p: Pos,
    ) -> Result<String, Diagnostic> {
        let bit = ty == Ty::Bool;
        let (lo, hi) = match (bit, retain) {
            (true, false) => self.alloc.m,
            (false, false) => self.alloc.d,
            (true, true) => self.retain.m,
            (false, true) => self.retain.d,
        };
        let range = lo..=hi;
        let occupied = if bit { &mut self.bits } else { &mut self.words };
        let n = range
            .clone()
            .find(|n| {
                (*n as usize + width - 1) <= *range.end() as usize
                    && (0..width).all(|i| !occupied.contains(&(*n + i as u16)))
            })
            .ok_or_else(|| {
                p.err(match (bit, retain) {
                    (true, false) => format!("BOOL yerel/geçici M{lo}..M{hi} alanı dolu"),
                    (false, false) => format!("Yerel/geçici D{lo}..D{hi} alanı dolu"),
                    (true, true) => format!("BOOL kalıcı (RETAIN) M{lo}..M{hi} alanı dolu"),
                    (false, true) => format!("Kalıcı (RETAIN) D{lo}..D{hi} alanı dolu"),
                })
            })?;
        for i in 0..width {
            let x = n + i as u16;
            occupied.insert(x);
            if temp {
                self.temps.push((bit, x));
            }
        }
        Ok(format!("{}{n}", if bit { 'M' } else { 'D' }))
    }
    fn release(&mut self, mark: usize) {
        for (bit, n) in self.temps.drain(mark..) {
            if bit {
                self.bits.remove(&n);
            } else {
                self.words.remove(&n);
            }
        }
    }
}

/// The percent-address dialect uses byte.bit for bits; Delta X/Y spellings
/// remain octal. %MWn maps directly to Dn, not a Modbus address.
fn device(raw: &str, p: Pos) -> Result<Option<Device>, Diagnostic> {
    let canonical = if let Some(s) = raw.strip_prefix('%') {
        let (kind, tail) = if let Some(t) = s.strip_prefix("IX") {
            ('X', t)
        } else if let Some(t) = s.strip_prefix("QX") {
            ('Y', t)
        } else if let Some(t) = s.strip_prefix("MX") {
            ('M', t)
        } else if let Some(t) = s.strip_prefix("MW") {
            ('D', t)
        } else {
            return Err(p.err(format!("Desteklenmeyen yüzde adresi: {raw}; %IXbyte.bit/%QXbyte.bit/%MXbyte.bit/%MWindex kullanın")));
        };
        let index = if kind == 'D' {
            tail.parse::<u16>()
                .map_err(|_| p.err("%MW indeksi geçersiz"))?
        } else {
            let (a, b) = tail
                .split_once('.')
                .ok_or_else(|| p.err("Bit adresi byte.bit olmalı"))?;
            let byte = a
                .parse::<u16>()
                .map_err(|_| p.err("Adres byte numarası geçersiz"))?;
            let bit = b
                .parse::<u16>()
                .map_err(|_| p.err("Adres bit numarası geçersiz"))?;
            if bit > 7 {
                return Err(p.err("Adres biti0..7 olmalı"));
            }
            byte.checked_mul(8)
                .and_then(|n| n.checked_add(bit))
                .ok_or_else(|| p.err("Adres taşıyor"))?
        };
        if matches!(kind, 'X' | 'Y') {
            format!("{kind}{index:o}")
        } else {
            format!("{kind}{index}")
        }
    } else {
        let b = raw.as_bytes();
        if b.len() < 2
            || !matches!(b[0], b'X' | b'Y' | b'M' | b'S' | b'D')
            || !b[1..].iter().all(u8::is_ascii_digit)
        {
            return Ok(None);
        }
        raw.into()
    };
    let d: Device = canonical.parse().map_err(|s: String| p.err(s))?;
    if d.area == Area::D && d.index > 4999 {
        return Err(p.err("ST ortak ES2/SS2 D aralığı D0..D4999"));
    }
    Ok(Some(d))
}
fn integer(e: &Expr) -> Option<i64> {
    match &e.kind {
        ExprKind::Number(n) if !is_real(n) => {
            let s = n.replace('_', "");
            if let Some((base, digits)) = s.split_once('#') {
                let base = base.parse().ok()?;
                if !matches!(base, 2 | 8 | 16) {
                    return None;
                }
                i64::from_str_radix(digits, base).ok()
            } else {
                s.parse().ok()
            }
        }
        ExprKind::Unary(op, a) if op == "-" => integer(a)?.checked_neg(),
        ExprKind::Unary(op, a) if op == "+" => integer(a),
        _ => None,
    }
}
fn is_real(s: &str) -> bool {
    !s.contains('#') && (s.contains('.') || s.contains('E'))
}
/// Element count and inclusive bounds of a declaration (`1` and `None` for a scalar).
fn array_bounds(d: &Decl, p: Pos) -> Result<(usize, Option<(i32, i32)>), Diagnostic> {
    let Some((lo, hi)) = &d.array else {
        return Ok((1, None));
    };
    let bound = |e: &Expr, what: &str| {
        integer(e)
            .filter(|n| (-32768..=32767).contains(n))
            .ok_or_else(|| {
                e.pos
                    .err(format!("Dizi {what} sınırı -32768..32767 tamsayı sabiti olmalı"))
            })
    };
    let (lo, hi) = (bound(lo, "alt")?, bound(hi, "üst")?);
    if hi < lo {
        return Err(p.err(format!("Dizi sınırları ters: {lo}..{hi}")));
    }
    let count = (hi - lo + 1) as usize;
    if count > 512 {
        return Err(p.err(format!(
            "Dizi {count} eleman içeriyor; Delta editor sınırı en fazla 512"
        )));
    }
    Ok((count, Some((lo as i32, hi as i32))))
}
fn offset(s: &str, n: u16) -> String {
    format!("D{}", s[1..].parse::<u16>().unwrap() + n)
}
fn constant_only(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Name(_) | ExprKind::Index(..) => false,
        ExprKind::Unary(_, a) => constant_only(a),
        ExprKind::Binary(_, a, b) => constant_only(a) && constant_only(b),
        _ => true,
    }
}

struct Emitter {
    symbols: Vec<StSymbol>,
    names: BTreeMap<String, usize>,
    /// Project-wide symbols; a local declaration of the same name shadows them.
    globals: BTreeMap<String, StSymbol>,
    pool: Pool,
    lines: Vec<String>,
    line_map: Vec<usize>,
    labels: u16,
    /// Label numbers taken by subroutine pointers of other programs.
    avoid: BTreeSet<u8>,
    exits: Vec<u8>,
    /// Label RETURN jumps to and the program ends with: P0 when the program stands alone,
    /// a label of its own when several ST programs are joined (`label_start` > 0).
    exit_label: u8,
}
impl Emitter {
    fn writes_control(&self, body: &[Stmt], control: &Value) -> Result<bool, Diagnostic> {
        let target = control.operand()[1..].parse::<u16>().unwrap();
        let hits = |name: &str, p: Pos| -> Result<bool, Diagnostic> {
            let v = self.lookup(name, p)?;
            if v.ty == Ty::Bool {
                return Ok(false);
            }
            let start = v.operand()[1..].parse::<u16>().unwrap();
            Ok((start..start + v.ty.words() as u16).contains(&target))
        };
        for s in body {
            let found = match &s.kind {
                StmtKind::Assign(n, _) => hits(n, s.pos)?,
                StmtKind::For(n, _, _, _, b) => {
                    hits(n, s.pos)? || self.writes_control(b, control)?
                }
                StmtKind::If(arms, other) => {
                    let mut found = self.writes_control(other, control)?;
                    for (_, b) in arms {
                        found |= self.writes_control(b, control)?;
                    }
                    found
                }
                StmtKind::Case(_, arms, other) => {
                    let mut found = self.writes_control(other, control)?;
                    for (_, b) in arms {
                        found |= self.writes_control(b, control)?;
                    }
                    found
                }
                StmtKind::While(_, b) | StmtKind::Repeat(b, _) => {
                    self.writes_control(b, control)?
                }
                _ => false,
            };
            if found {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn line(&mut self, p: Pos, text: impl Into<String>) {
        self.lines.push(text.into());
        self.line_map.push(p.line);
    }
    fn label(&mut self, p: Pos) -> Result<u8, Diagnostic> {
        while self.labels < 256 && self.avoid.contains(&(self.labels as u8)) {
            self.labels += 1;
        }
        if self.labels >= 256 {
            return Err(p.err("ST kontrol akışı P0..P255 etiket alanını aşıyor"));
        }
        let n = self.labels as u8;
        self.labels += 1;
        Ok(n)
    }
    fn mark(&mut self, p: Pos, n: u8) {
        self.line(p, format!("P{n}"));
    }
    fn go(&mut self, p: Pos, n: u8) {
        self.line(p, "LD M1000");
        self.line(p, format!("CJ P{n}"));
    }
    fn action(&mut self, p: Pos, s: impl Into<String>) {
        self.line(p, "LD M1000");
        self.line(p, s);
    }
    fn bool_contact(&mut self, p: Pos, v: &Value, prefix: &str) {
        let (bit, invert) = match &v.repr {
            Repr::Bit(s, b) => (s.as_str(), *b),
            Repr::Bool(b) => ("M1000", !*b),
            _ => unreachable!(),
        };
        let op = match (prefix, invert) {
            ("LD", true) => "LDI",
            ("AND", true) => "ANI",
            ("OR", true) => "ORI",
            (s, _) => s,
        };
        self.line(p, format!("{op} {bit}"));
    }
    fn branch(&mut self, p: Pos, v: &Value, when: bool, n: u8) {
        let val = if when {
            v.clone()
        } else {
            v.clone().inverted()
        };
        self.bool_contact(p, &val, "LD");
        self.line(p, format!("CJ P{n}"));
    }
    fn temp(&mut self, ty: Ty, p: Pos) -> Result<Value, Diagnostic> {
        let s = self.pool.allocate(ty, ty.words(), true, p)?;
        Ok(if ty == Ty::Bool {
            Value::bit(s)
        } else {
            Value::num(ty, s)
        })
    }
    fn lookup(&self, name: &str, p: Pos) -> Result<Value, Diagnostic> {
        if let Some(&n) = self.names.get(name) {
            let s = &self.symbols[n];
            if s.array.is_some() {
                return Err(p.err(format!(
                    "{name} bir dizidir; eleman olarak {name}[indeks] yazın"
                )));
            }
            return Ok(if s.data_type == Ty::Bool {
                Value::bit(&s.address)
            } else {
                Value::num(s.data_type, &s.address)
            });
        }
        if let Some(s) = self.globals.get(name) {
            return Ok(if s.data_type == Ty::Bool {
                Value::bit(&s.address)
            } else {
                Value::num(s.data_type, &s.address)
            });
        }
        if let Some(d) = device(name, p)? {
            return Ok(if d.area == Area::D {
                Value::num(Ty::Int, d.to_string())
            } else {
                Value::bit(d.to_string())
            });
        }
        Err(p.err(format!("Tanımsız değişken: {name}")))
    }
    fn writable(&self, name: &str, p: Pos) -> Result<Value, Diagnostic> {
        let v = self.lookup(name, p)?;
        let addr = match &v.repr {
            Repr::Bit(a, _) | Repr::Num(a) => a,
            _ => unreachable!(),
        };
        let d: Device = addr.parse().map_err(|s: String| p.err(s))?;
        if d.area == Area::X || d.area != Area::D && !d.writable_bit() {
            return Err(p.err(format!("{addr} salt okunur; ST ataması yapılamaz")));
        }
        Ok(v)
    }
    fn ty_of(&self, e: &Expr) -> Result<Option<Ty>, Diagnostic> {
        Ok(match &e.kind {
            ExprKind::Bool(_) => Some(Ty::Bool),
            ExprKind::Name(n) => Some(self.lookup(n, e.pos)?.ty),
            ExprKind::Index(n, _) => Some(self.array_symbol(n, e.pos)?.data_type),
            ExprKind::Number(n) => {
                if is_real(n) {
                    Some(Ty::Real)
                } else {
                    None
                }
            }
            ExprKind::Unary(_, a) => self.ty_of(a)?,
            ExprKind::Binary(op, a, b) => {
                if matches!(op.as_str(), "=" | "<>" | "<" | ">" | "<=" | ">=") {
                    Some(Ty::Bool)
                } else {
                    self.common(self.ty_of(a)?, self.ty_of(b)?, None, e.pos)?
                }
            }
        })
    }
    fn common(
        &self,
        a: Option<Ty>,
        b: Option<Ty>,
        hint: Option<Ty>,
        p: Pos,
    ) -> Result<Option<Ty>, Diagnostic> {
        match (a, b) {
            (Some(x), Some(y)) if x == y => Ok(Some(x)),
            (Some(Ty::Int), Some(Ty::Dint)) | (Some(Ty::Dint), Some(Ty::Int)) => Ok(Some(Ty::Dint)),
            (Some(Ty::Word), Some(Ty::Dword)) | (Some(Ty::Dword), Some(Ty::Word)) => {
                Ok(Some(Ty::Dword))
            }
            (Some(x), None) | (None, Some(x)) => Ok(Some(x)),
            (None, None) => Ok(hint),
            (Some(x), Some(y)) => Err(p.err(format!("{x} ve {y} örtük olarak birleştirilemez"))),
        }
    }
    fn literal(&mut self, e: &Expr, ty: Ty) -> Result<Value, Diagnostic> {
        if ty == Ty::Bool {
            return Err(e
                .pos
                .err("Sayısal sabit BOOL değildir; TRUE/FALSE veya karşılaştırma kullanın"));
        }
        if ty == Ty::Real {
            let text = match &e.kind {
                ExprKind::Number(s) => s.replace('_', ""),
                ExprKind::Unary(op, a) if matches!(op.as_str(), "-" | "+") => {
                    let ExprKind::Number(s) = &a.kind else {
                        return Err(e.pos.err("REAL sabiti geçersiz"));
                    };
                    format!("{op}{}", s.replace('_', ""))
                }
                _ => unreachable!(),
            };
            let n = if let Some(n) = integer(e) {
                n as f32
            } else {
                text.parse::<f32>()
                    .map_err(|_| e.pos.err("REAL sabiti geçersiz"))?
            };
            if !n.is_finite() {
                return Err(e.pos.err("REAL sabiti sonlu binary32 olmalı"));
            }
            return Ok(Value::num(ty, format!("F{n}")));
        }
        let n = integer(e).ok_or_else(|| {
            e.pos
                .err("Tamsayı sabiti geçersiz; ondalık veya 2#/8#/16# tabanı kullanın")
        })?;
        let hex = match &e.kind {
            ExprKind::Number(s) => s.contains('#'),
            _ => false,
        };
        let val = match ty {
            Ty::Int if (-32768..=32767).contains(&n) || hex && (0..=65535).contains(&n) => {
                n as i16 as i32
            }
            Ty::Dint
                if (i32::MIN as i64..=i32::MAX as i64).contains(&n)
                    || hex && (0..=u32::MAX as i64).contains(&n) =>
            {
                n as i32
            }
            Ty::Word if (0..=65535).contains(&n) => n as u16 as i16 as i32,
            Ty::Dword if (0..=u32::MAX as i64).contains(&n) => n as u32 as i32,
            _ => return Err(e.pos.err(format!("{n}, {ty} sabit aralığı dışında"))),
        };
        Ok(Value::num(ty, format!("K{val}")))
    }
    fn coerce(&mut self, v: Value, to: Ty, p: Pos) -> Result<Value, Diagnostic> {
        if v.ty == to {
            return Ok(v);
        }
        if matches!((v.ty, to), (Ty::Int, Ty::Dint) | (Ty::Word, Ty::Dword)) {
            let out = self.temp(to, p)?;
            self.action(p, format!("MOV {} {}", v.operand(), out.operand()));
            self.action(p, format!("MOV K0 {}", offset(out.operand(), 1)));
            if v.ty == Ty::Int {
                self.line(p, format!("LD< {} K0", v.operand()));
                self.line(p, format!("MOV K-1 {}", offset(out.operand(), 1)));
            }
            return Ok(out);
        }
        Err(p.err(format!("{0} -> {to} örtük dönüşümü desteklenmiyor", v.ty)))
    }
    fn store(&mut self, p: Pos, dest: &Value, value: Value) -> Result<(), Diagnostic> {
        let value = self.coerce(value, dest.ty, p)?;
        if dest.ty == Ty::Bool {
            self.bool_contact(p, &value, "LD");
            let Repr::Bit(d, _) = &dest.repr else {
                unreachable!()
            };
            self.line(p, format!("OUT {d}"));
        } else {
            let cmd = if dest.ty == Ty::Real && value.operand().starts_with('F') {
                "DMOVR"
            } else if dest.ty.words() == 2 {
                "DMOV"
            } else {
                "MOV"
            };
            self.action(p, format!("{cmd} {} {}", value.operand(), dest.operand()));
        }
        Ok(())
    }
    fn bool_binary(&mut self, p: Pos, op: &str, a: Value, b: Value) -> Result<Value, Diagnostic> {
        if a.ty != Ty::Bool || b.ty != Ty::Bool {
            return Err(p.err("BOOL işlemi BOOL operandlar gerektirir"));
        }
        let out = self.temp(Ty::Bool, p)?;
        self.bool_contact(p, &a, "LD");
        if op == "XOR" {
            self.bool_contact(p, &b.clone().inverted(), "AND");
            self.bool_contact(p, &a.inverted(), "LD");
            self.bool_contact(p, &b, "AND");
            self.line(p, "ORB");
        } else {
            self.bool_contact(p, &b, if op == "OR" { "OR" } else { "AND" });
        }
        let Repr::Bit(d, _) = &out.repr else {
            unreachable!()
        };
        self.line(p, format!("OUT {d}"));
        Ok(out)
    }
    fn compare(
        &mut self,
        p: Pos,
        op: &str,
        mut a: Value,
        mut b: Value,
    ) -> Result<Value, Diagnostic> {
        let ty = self.common(Some(a.ty), Some(b.ty), None, p)?.unwrap();
        a = self.coerce(a, ty, p)?;
        b = self.coerce(b, ty, p)?;
        if ty == Ty::Bool {
            if !matches!(op, "=" | "<>") {
                return Err(p.err("BOOL yalnız = ve <> ile karşılaştırılır"));
            }
            let v = self.bool_binary(p, "XOR", a, b)?;
            return Ok(if op == "=" { v.inverted() } else { v });
        }
        if ty.bits() && !matches!(op, "=" | "<>") {
            let x = self.temp(ty, p)?;
            let y = self.temp(ty, p)?;
            let (cmd, bias) = if ty == Ty::Word {
                ("WXOR", "H8000")
            } else {
                ("DXOR", "H80000000")
            };
            self.action(p, format!("{cmd} {} {bias} {}", a.operand(), x.operand()));
            self.action(p, format!("{cmd} {} {bias} {}", b.operand(), y.operand()));
            a = x;
            b = y;
        }
        let out = self.temp(Ty::Bool, p)?;
        let prefix = if ty == Ty::Real {
            "FLD"
        } else if ty.words() == 2 {
            "DLD"
        } else {
            "LD"
        };
        self.line(p, format!("{prefix}{op} {} {}", a.operand(), b.operand()));
        let Repr::Bit(d, _) = &out.repr else {
            unreachable!()
        };
        self.line(p, format!("OUT {d}"));
        Ok(out)
    }
    fn unsigned_half(&mut self, p: Pos, v: &Value, high: bool) -> Result<Value, Diagnostic> {
        if let Some(n) = v.operand().strip_prefix('K') {
            let bits = n.parse::<i32>().unwrap() as u32;
            return Ok(Value::num(
                Ty::Dint,
                format!("K{}", if high { bits >> 16 } else { bits & 65535 }),
            ));
        }
        let out = self.temp(Ty::Dint, p)?;
        let src = if high {
            offset(v.operand(), 1)
        } else {
            v.operand().into()
        };
        self.action(p, format!("MOV {src} {}", out.operand()));
        self.action(p, format!("MOV K0 {}", offset(out.operand(), 1)));
        Ok(out)
    }
    // Delta ADD/SUB normalize signed overflow instead of wrapping. Keep those
    // proven opcodes unchanged; construct ST two's-complement arithmetic from
    // wider safe sums or independently zero-extended half-word sums.
    fn wrapping_arithmetic(
        &mut self,
        p: Pos,
        op: &str,
        a: Value,
        b: Value,
    ) -> Result<Value, Diagnostic> {
        if a.ty == Ty::Int {
            let a = self.coerce(a, Ty::Dint, p)?;
            let b = self.coerce(b, Ty::Dint, p)?;
            let wide = self.temp(Ty::Dint, p)?;
            self.action(
                p,
                format!(
                    "{} {} {} {}",
                    if op == "+" { "DADD" } else { "DSUB" },
                    a.operand(),
                    b.operand(),
                    wide.operand()
                ),
            );
            let out = self.temp(Ty::Int, p)?;
            self.action(p, format!("MOV {} {}", wide.operand(), out.operand()));
            return Ok(out);
        }
        let al = self.unsigned_half(p, &a, false)?;
        let ah = self.unsigned_half(p, &a, true)?;
        let bl = self.unsigned_half(p, &b, false)?;
        let bh = self.unsigned_half(p, &b, true)?;
        let low = self.temp(Ty::Dint, p)?;
        let high = self.temp(Ty::Dint, p)?;
        let cmd = if op == "+" { "DADD" } else { "DSUB" };
        self.action(
            p,
            format!("{cmd} {} {} {}", al.operand(), bl.operand(), low.operand()),
        );
        self.action(
            p,
            format!("{cmd} {} {} {}", ah.operand(), bh.operand(), high.operand()),
        );
        self.line(
            p,
            format!(
                "{} {} {}",
                if op == "+" { "DLD>=" } else { "DLD<" },
                low.operand(),
                if op == "+" { "K65536" } else { "K0" }
            ),
        );
        self.line(p, format!("{cmd} {} K1 {}", high.operand(), high.operand()));
        let out = self.temp(Ty::Dint, p)?;
        self.action(p, format!("MOV {} {}", low.operand(), out.operand()));
        self.action(
            p,
            format!("MOV {} {}", high.operand(), offset(out.operand(), 1)),
        );
        Ok(out)
    }
    fn array_symbol(&self, name: &str, p: Pos) -> Result<&StSymbol, Diagnostic> {
        match self.names.get(name).map(|&n| &self.symbols[n]) {
            Some(s) if s.array.is_some() => Ok(s),
            Some(_) => Err(p.err(format!("{name} dizi değil; indekslenemez"))),
            None => Err(p.err(format!("Tanımsız dizi: {name}"))),
        }
    }
    /// An `ARRAY` element. A constant index is the fixed device `base + (i-lo)*width` and is
    /// range-checked here. Any other index is evaluated into E0 as `(i-lo)*width` and the access
    /// is the indexed operand `D<base>E0`; there is NO runtime bounds check, so an out-of-range
    /// index silently reads/writes a neighbouring device. A read is copied to a temporary right
    /// away (E0 is free again afterwards); a write returns the indexed operand itself, so the
    /// caller must store before emitting anything that evaluates another index.
    fn element(
        &mut self,
        name: &str,
        index: &Expr,
        p: Pos,
        read: bool,
    ) -> Result<Value, Diagnostic> {
        let s = self.array_symbol(name, p)?.clone();
        let (lo, hi) = s.array.expect("array symbol");
        let (lo, hi) = (i64::from(lo), i64::from(hi));
        let bit = s.data_type == Ty::Bool;
        let width = if bit { 1 } else { s.data_type.words() as i64 };
        let area = &s.address[..1];
        let base = s.address[1..].parse::<i64>().expect("array base");
        if let Some(k) = integer(index) {
            if !(lo..=hi).contains(&k) {
                return Err(index.pos.err(format!(
                    "{name}[{k}] indeksi dizi sınırları {lo}..{hi} dışında"
                )));
            }
            let address = format!("{area}{}", base + (k - lo) * width);
            return Ok(if bit {
                Value::bit(address)
            } else {
                Value::num(s.data_type, address)
            });
        }
        if bit {
            return Err(index
                .pos
                .err("BOOL dizisinde değişken indeks desteklenmiyor; sabit indeks kullanın"));
        }
        let mut i = self.eval(index, Some(Ty::Int))?;
        if i.ty != Ty::Int {
            return Err(index.pos.err("Dizi indeksi INT olmalı"));
        }
        // Fold the lower bound into the base address; only a positive offset beyond the
        // base itself (which would be a negative device number) needs a runtime SUB.
        let shift = lo * width;
        let base = if shift <= base {
            base - shift
        } else {
            let adjusted = self.temp(Ty::Int, p)?;
            self.action(
                p,
                format!("SUB {} K{lo} {}", i.operand(), adjusted.operand()),
            );
            i = adjusted;
            base
        };
        if width == 1 {
            self.action(p, format!("MOV {} E0", i.operand()));
        } else {
            self.action(p, format!("MUL16 {} K{width} E0", i.operand()));
        }
        let operand = format!("D{base}E0");
        if !read {
            return Ok(Value::num(s.data_type, operand));
        }
        let out = self.temp(s.data_type, p)?;
        let cmd = if width == 2 { "DMOV" } else { "MOV" };
        self.action(p, format!("{cmd} {operand} {}", out.operand()));
        Ok(out)
    }
    fn eval(&mut self, e: &Expr, hint: Option<Ty>) -> Result<Value, Diagnostic> {
        if integer(e).is_some()
            || matches!(&e.kind, ExprKind::Number(_))
            || matches!(&e.kind,ExprKind::Unary(op,a)if matches!(op.as_str(),"-"|"+")&&matches!(a.kind,ExprKind::Number(_)))
        {
            let ty = hint.or(self.ty_of(e)?).unwrap_or_else(|| {
                if integer(e).is_some_and(|n| !(-32768..=32767).contains(&n)) {
                    Ty::Dint
                } else {
                    Ty::Int
                }
            });
            return self.literal(e, ty);
        }
        match &e.kind {
            ExprKind::Bool(v) => Ok(Value {
                ty: Ty::Bool,
                repr: Repr::Bool(*v),
            }),
            ExprKind::Name(n) => self.lookup(n, e.pos),
            ExprKind::Index(n, index) => self.element(n, index, e.pos, true),
            ExprKind::Unary(op, a) => {
                let v = self.eval(a, hint)?;
                if op == "+" {
                    if matches!(v.ty, Ty::Bool | Ty::Word | Ty::Dword) {
                        return Err(e.pos.err("Tekli + sayısal INT/DINT/REAL gerektirir"));
                    }
                    return Ok(v);
                }
                if op == "NOT" && v.ty == Ty::Bool {
                    return Ok(v.inverted());
                }
                let out = self.temp(v.ty, e.pos)?;
                let instruction = match (op.as_str(), v.ty) {
                    ("NOT", Ty::Word) => format!("CML {} {}", v.operand(), out.operand()),
                    ("NOT", Ty::Dword) => format!("DCML {} {}", v.operand(), out.operand()),
                    ("-", Ty::Int) => format!("SUB K0 {} {}", v.operand(), out.operand()),
                    ("-", Ty::Dint) => format!("DSUB K0 {} {}", v.operand(), out.operand()),
                    ("-", Ty::Real) => format!("DSUBR F0 {} {}", v.operand(), out.operand()),
                    _ => return Err(e.pos.err(format!("{op}, {} için desteklenmiyor", v.ty))),
                };
                self.action(e.pos, instruction);
                if v.ty == Ty::Real {
                    self.real_fault_exit(e.pos, &v, None, false)?;
                }
                Ok(out)
            }
            ExprKind::Binary(op, a, b) => {
                let comparison = matches!(op.as_str(), "=" | "<>" | "<" | ">" | "<=" | ">=");
                let types = self.common(
                    self.ty_of(a)?,
                    self.ty_of(b)?,
                    if comparison { None } else { hint },
                    e.pos,
                )?;
                let ty = types.unwrap_or_else(|| {
                    if integer(a)
                        .into_iter()
                        .chain(integer(b))
                        .any(|n| !(-32768..=32767).contains(&n))
                    {
                        Ty::Dint
                    } else {
                        Ty::Int
                    }
                });
                let av = self.eval(a, Some(ty))?;
                let av = self.coerce(av, ty, a.pos)?;
                let bv = self.eval(b, Some(ty))?;
                let bv = self.coerce(bv, ty, b.pos)?;
                if comparison {
                    return self.compare(e.pos, op, av, bv);
                }
                if matches!(op.as_str(), "AND" | "&" | "OR" | "XOR") {
                    if ty == Ty::Bool {
                        return self.bool_binary(e.pos, op, av, bv);
                    }
                    if !ty.bits() {
                        return Err(e
                            .pos
                            .err("AND/OR/XOR sayısal INT yerine BOOL/WORD/DWORD gerektirir"));
                    }
                    let cmd = match (op.as_str(), ty) {
                        ("OR", Ty::Word) => "WOR",
                        ("XOR", Ty::Word) => "WXOR",
                        (_, Ty::Word) => "WAND",
                        ("OR", _) => "DOR",
                        ("XOR", _) => "DXOR",
                        _ => "DAND",
                    };
                    let out = self.temp(ty, e.pos)?;
                    self.action(
                        e.pos,
                        format!("{cmd} {} {} {}", av.operand(), bv.operand(), out.operand()),
                    );
                    return Ok(out);
                }
                if matches!(ty, Ty::Bool | Ty::Word | Ty::Dword) {
                    return Err(e
                        .pos
                        .err("Aritmetik INT/DINT/REAL gerektirir; WORD/DWORD bit dizisidir"));
                }
                if ty == Ty::Real {
                    let cmd = match op.as_str() {
                        "+" => "DADDR",
                        "-" => "DSUBR",
                        "*" => "DMULR",
                        "/" => "DDIVR",
                        _ => return Err(e.pos.err("REAL MOD desteklenmiyor")),
                    };
                    let out = self.temp(ty, e.pos)?;
                    self.action(
                        e.pos,
                        format!("{cmd} {} {} {}", av.operand(), bv.operand(), out.operand()),
                    );
                    self.real_fault_exit(e.pos, &av, Some(&bv), op == "/")?;
                    return Ok(out);
                }
                if matches!(op.as_str(), "+" | "-") {
                    return self.wrapping_arithmetic(e.pos, op, av, bv);
                }
                let wide = ty == Ty::Dint;
                if op == "MOD" {
                    let width = if wide { 4 } else { 2 };
                    let base = self.pool.allocate(Ty::Int, width, true, e.pos)?;
                    let cmd = if wide { "DDIV" } else { "DIV" };
                    self.action(
                        e.pos,
                        format!("{cmd} {} {} {base}", av.operand(), bv.operand()),
                    );
                    self.zero_divisor_exit(e.pos, &bv);
                    return Ok(Value::num(ty, offset(&base, if wide { 2 } else { 1 })));
                }
                let cmd = match (op.as_str(), wide) {
                    ("+", false) => "ADD",
                    ("-", false) => "SUB",
                    ("*", false) => "MUL16",
                    ("/", false) => "DIV16",
                    ("+", true) => "DADD",
                    ("-", true) => "DSUB",
                    ("*", true) => "MUL32",
                    ("/", true) => "DIV32",
                    _ => return Err(e.pos.err(format!("Desteklenmeyen işlem: {op}"))),
                };
                let out = self.temp(ty, e.pos)?;
                self.action(
                    e.pos,
                    format!("{cmd} {} {} {}", av.operand(), bv.operand(), out.operand()),
                );
                if op == "/" {
                    self.zero_divisor_exit(e.pos, &bv);
                }
                Ok(out)
            }
            ExprKind::Number(_) => unreachable!(),
        }
    }
    fn zero_divisor_exit(&mut self, p: Pos, b: &Value) {
        self.line(
            p,
            format!(
                "{}= {} K0",
                if b.ty.words() == 2 { "DLD" } else { "LD" },
                b.operand()
            ),
        );
        self.line(p, "CJ P0");
    }
    fn real_fault_exit(
        &mut self,
        p: Pos,
        a: &Value,
        b: Option<&Value>,
        division: bool,
    ) -> Result<(), Diagnostic> {
        // R handlers diagnose NaN sources and hold their result. Test the raw
        // source fields after that genuine instruction, before committing any
        // ST assignment, even when a previous PLC error was already latched.
        for v in std::iter::once(a).chain(b) {
            if !v.operand().starts_with('F') {
                let magnitude = self.temp(Ty::Dword, p)?;
                self.action(
                    p,
                    format!("DAND {} H7FFFFFFF {}", v.operand(), magnitude.operand()),
                );
                self.line(p, format!("DLD> {} H7F800000", magnitude.operand()));
                self.line(p, "CJ P0");
            }
        }
        if division {
            let b = b.expect("binary REAL division");
            if let Some(n) = b.operand().strip_prefix('F') {
                let n = n.parse::<f32>().expect("validated finite literal");
                if n.to_bits() & 0x7f80_0000 == 0 {
                    self.go(p, self.exit_label);
                }
            } else {
                let exponent = self.temp(Ty::Dword, p)?;
                self.action(
                    p,
                    format!("DAND {} H7F800000 {}", b.operand(), exponent.operand()),
                );
                self.line(p, format!("DLD= {} K0", exponent.operand()));
                self.line(p, "CJ P0");
            }
        }
        Ok(())
    }
    fn block(&mut self, body: &[Stmt]) -> Result<(), Diagnostic> {
        for s in body {
            self.stmt(s)?;
        }
        Ok(())
    }
    fn stmt(&mut self, s: &Stmt) -> Result<(), Diagnostic> {
        let mark = self.pool.temps.len();
        let p = s.pos;
        match &s.kind {
            StmtKind::Assign(name, e) => {
                let dest = self.writable(name, p)?;
                let v = self.eval(e, Some(dest.ty))?;
                self.store(p, &dest, v)?;
            }
            StmtKind::AssignIndex(name, index, e) => {
                let ty = self.array_symbol(name, p)?.data_type;
                let v = self.eval(e, Some(ty))?;
                let v = self.coerce(v, ty, p)?;
                let dest = self.element(name, index, p, false)?;
                self.store(p, &dest, v)?;
            }
            StmtKind::If(arms, other) => {
                let end = self.label(p)?;
                for (e, body) in arms {
                    let next = self.label(e.pos)?;
                    let before = self.pool.temps.len();
                    let v = self.eval(e, Some(Ty::Bool))?;
                    if v.ty != Ty::Bool {
                        return Err(e.pos.err("IF koşulu BOOL olmalı"));
                    }
                    self.branch(e.pos, &v, false, next);
                    self.pool.release(before);
                    self.block(body)?;
                    self.go(p, end);
                    self.mark(e.pos, next);
                }
                self.block(other)?;
                self.mark(p, end);
            }
            StmtKind::Case(expr, arms, other) => {
                let v = self.eval(expr, None)?;
                if matches!(v.ty, Ty::Bool | Ty::Real) {
                    return Err(expr.pos.err("CASE seçicisi INT/DINT/WORD/DWORD olmalı"));
                }
                let captured = self.temp(v.ty, p)?;
                self.store(p, &captured, v)?;
                let end = self.label(p)?;
                let mut ranges = Vec::<(i64, i64)>::new();
                for (labels, body) in arms {
                    let next = self.label(p)?;
                    let before = self.pool.temps.len();
                    let mut cond = None;
                    for (a, b) in labels {
                        let av = self.literal(a, captured.ty)?;
                        let bv = b
                            .as_ref()
                            .map(|b| self.literal(b, captured.ty))
                            .transpose()?;
                        let canonical = |v: &Value| {
                            let n = v.operand()[1..].parse::<i32>().unwrap();
                            match v.ty {
                                Ty::Word => i64::from(n as u16),
                                Ty::Dword => i64::from(n as u32),
                                _ => i64::from(n),
                            }
                        };
                        let lo = canonical(&av);
                        let hi = bv.as_ref().map(canonical).unwrap_or(lo);
                        if hi < lo || ranges.iter().any(|(x, y)| lo <= *y && *x <= hi) {
                            return Err(a
                                .pos
                                .err("CASE etiket aralığı ters veya önceki etiketle çakışıyor"));
                        }
                        ranges.push((lo, hi));
                        let c = if let Some(bv) = bv {
                            let lower = self.compare(a.pos, ">=", captured.clone(), av)?;
                            let upper = self.compare(a.pos, "<=", captured.clone(), bv)?;
                            self.bool_binary(a.pos, "AND", lower, upper)?
                        } else {
                            self.compare(a.pos, "=", captured.clone(), av)?
                        };
                        cond = Some(if let Some(old) = cond {
                            self.bool_binary(a.pos, "OR", old, c)?
                        } else {
                            c
                        });
                    }
                    let cond = cond.ok_or_else(|| p.err("CASE kolu etiketsiz"))?;
                    self.branch(p, &cond, false, next);
                    self.pool.release(before);
                    self.block(body)?;
                    self.go(p, end);
                    self.mark(p, next);
                }
                self.block(other)?;
                self.mark(p, end);
            }
            StmtKind::While(condition, body) => {
                let start = self.label(p)?;
                let end = self.label(p)?;
                self.mark(p, start);
                let before = self.pool.temps.len();
                let v = self.eval(condition, Some(Ty::Bool))?;
                if v.ty != Ty::Bool {
                    return Err(condition.pos.err("WHILE koşulu BOOL olmalı"));
                }
                self.branch(p, &v, false, end);
                self.pool.release(before);
                self.exits.push(end);
                self.block(body)?;
                self.exits.pop();
                self.go(p, start);
                self.mark(p, end);
            }
            StmtKind::Repeat(body, condition) => {
                let start = self.label(p)?;
                let end = self.label(p)?;
                self.mark(p, start);
                self.exits.push(end);
                self.block(body)?;
                self.exits.pop();
                let v = self.eval(condition, Some(Ty::Bool))?;
                if v.ty != Ty::Bool {
                    return Err(condition.pos.err("UNTIL koşulu BOOL olmalı"));
                }
                self.branch(p, &v, false, start);
                self.mark(p, end);
            }
            StmtKind::Exit => {
                let end = *self
                    .exits
                    .last()
                    .ok_or_else(|| p.err("EXIT yalnız döngü içinde kullanılabilir"))?;
                self.go(p, end);
            }
            StmtKind::Return => self.go(p, self.exit_label),
            StmtKind::For(name, start, end, by, body) => {
                let control = self.writable(name, p)?;
                if control.ty != Ty::Int {
                    return Err(p.err(
                        "FOR kontrolü bu profilde INT olmalı; DINT için açık WHILE kullanın",
                    ));
                }
                if self.writes_control(body, &control)? {
                    return Err(p.err("FOR gövdesi kontrol değişkenine atama yapamaz"));
                }
                let step = if let Some(e) = by {
                    integer(e)
                        .ok_or_else(|| e.pos.err("FOR BY sıfır olmayan tamsayı sabiti olmalı"))?
                } else {
                    1
                };
                if step == 0 || !(-32768..=32767).contains(&step) {
                    return Err(p.err("FOR BY -32768..32767 aralığında ve sıfırdan farklı olmalı"));
                }
                let a = self.eval(start, Some(Ty::Int))?;
                let saved = self.temp(Ty::Int, p)?;
                self.store(p, &saved, a)?;
                let b = self.eval(end, Some(Ty::Int))?;
                let limit = self.temp(Ty::Int, p)?;
                self.store(p, &limit, b)?;
                self.store(p, &control, saved.clone())?;
                let exit = self.label(p)?;
                let begin = self.label(p)?;
                let valid = self.compare(
                    p,
                    if step > 0 { "<=" } else { ">=" },
                    saved.clone(),
                    limit.clone(),
                )?;
                self.branch(p, &valid, false, exit);
                let a = self.coerce(saved, Ty::Dint, p)?;
                let b = self.coerce(limit, Ty::Dint, p)?;
                let count = self.temp(Ty::Dint, p)?;
                let (left, right) = if step > 0 { (b, a) } else { (a, b) };
                self.action(
                    p,
                    format!(
                        "DSUB {} {} {}",
                        left.operand(),
                        right.operand(),
                        count.operand()
                    ),
                );
                self.action(
                    p,
                    format!(
                        "DIV32 {} K{} {}",
                        count.operand(),
                        step.abs(),
                        count.operand()
                    ),
                );
                self.action(
                    p,
                    format!("DADD {} K1 {}", count.operand(), count.operand()),
                );
                self.mark(p, begin);
                self.exits.push(exit);
                self.block(body)?;
                self.exits.pop();
                let next = self.wrapping_arithmetic(
                    p,
                    "+",
                    control.clone(),
                    Value::num(Ty::Int, format!("K{step}")),
                )?;
                self.store(p, &control, next)?;
                self.action(
                    p,
                    format!("DSUB {} K1 {}", count.operand(), count.operand()),
                );
                self.line(p, format!("DLD> {} K0", count.operand()));
                self.line(p, format!("CJ P{begin}"));
                self.mark(p, exit);
            }
        }
        self.pool.release(mark);
        Ok(())
    }
    fn declarations(&mut self, decls: &[Decl]) -> Result<(), Diagnostic> {
        for d in decls {
            for (name, p) in &d.names {
                if reserved(name) || device(name, *p)?.is_some() {
                    return Err(
                        p.err("Değişken adı anahtar sözcük veya doğrudan PLC adresi olamaz")
                    );
                }
                if self.names.contains_key(name) {
                    return Err(p.err(format!("Yinelenen değişken: {name}")));
                }
                let (count, array) = array_bounds(d, *p)?;
                let width = if d.ty == Ty::Bool { 1 } else { d.ty.words() } * count;
                if d.retain && d.init.is_some() {
                    return Err(p.err(
                        "VAR_RETAIN başlangıç değeri desteklenmiyor (her RUN'da değeri ezerdi)",
                    ));
                }
                if array.is_some() && d.init.is_some() {
                    return Err(p.err("Dizi başlangıç değeri bu ST profilinde desteklenmiyor"));
                }
                let address = if let Some((a, pos)) = &d.mapping {
                    let dev =
                        device(a, *pos)?.ok_or_else(|| pos.err("AT gerçek PLC adresi olmalı"))?;
                    if (d.ty == Ty::Bool) == (dev.area == Area::D) {
                        return Err(pos.err("BOOL bit adresine, sayısal tip D adresine eşlenmeli"));
                    }
                    if array.is_some() && d.ty == Ty::Bool && dev.area != Area::M {
                        return Err(pos.err("BOOL dizisi yalnız M adresine eşlenebilir"));
                    }
                    if dev.area == Area::D && usize::from(dev.index) + width > 5000 {
                        return Err(pos.err(if array.is_some() {
                            "Dizi eşlemesi D4999 sınırını aşıyor"
                        } else {
                            "Çift word eşlemesi D4999 sınırını aşıyor"
                        }));
                    }
                    if dev.area == Area::M && usize::from(dev.index) + width > 4096 {
                        return Err(pos.err("Dizi eşlemesi M4095 sınırını aşıyor"));
                    }
                    if d.retain {
                        let latched = if d.ty == Ty::Bool {
                            dev.area == Area::M
                                && (0..width as u16)
                                    .all(|i| crate::alloc::Retain::bit_is_latched(dev.index + i))
                        } else {
                            crate::alloc::Retain::word_is_latched(dev.index, width)
                        };
                        if !latched {
                            return Err(pos.err(format!(
                                "VAR_RETAIN AT {dev} latched alanın dışında (D408..D599, D2000..D3919, M512..M767, M2048..M4095)"
                            )));
                        }
                    }
                    dev.to_string()
                } else {
                    self.pool.allocate_in(d.ty, width, false, d.retain, *p)?
                };
                let idx = self.symbols.len();
                self.names.insert(name.clone(), idx);
                self.symbols.push(StSymbol {
                    name: name.clone(),
                    data_type: d.ty,
                    address,
                    line: p.line,
                    explicitly_mapped: d.mapping.is_some(),
                    retained: d.retain,
                    array,
                });
            }
        }
        if decls.iter().any(|d| d.init.is_some()) {
            let p = decls
                .iter()
                .find_map(|d| d.init.as_ref().map(|e| e.pos))
                .unwrap();
            let end = self.label(p)?;
            self.line(p, "LDI M1002");
            self.line(p, format!("CJ P{end}"));
            for d in decls {
                if let Some(init) = &d.init {
                    if !constant_only(init) {
                        return Err(init.pos.err("VAR başlangıç değeri sabit ifade olmalı"));
                    }
                    for (name, p) in &d.names {
                        let mark = self.pool.temps.len();
                        let dest = self.writable(name, *p)?;
                        let v = self.eval(init, Some(d.ty))?;
                        self.store(*p, &dest, v)?;
                        self.pool.release(mark);
                    }
                }
            }
            self.mark(p, end);
        }
        Ok(())
    }
}

pub(super) fn lower(unit: Unit, options: &super::LowerOptions) -> Result<LoweredSt, Diagnostic> {
    let mut e = Emitter {
        symbols: vec![],
        names: BTreeMap::new(),
        globals: BTreeMap::new(),
        pool: Pool::new(options.alloc, options.retain),
        lines: vec![],
        line_map: vec![],
        labels: options.label_start.max(1),
        avoid: options
            .avoid_labels
            .iter()
            .filter_map(|n| u8::try_from(*n).ok())
            .collect(),
        exits: vec![],
        exit_label: 0,
    };
    // Global symbols keep their devices; locals and temporaries are placed around them.
    for global in &options.globals {
        if let Ok(dev) = global.address.parse::<Device>() {
            e.pool.reserve(dev, global.data_type.words());
        }
        // ST identifiers are case-insensitive and arrive upper-cased from the parser.
        e.globals.insert(
            global.name.to_ascii_uppercase(),
            StSymbol {
                name: global.name.to_ascii_uppercase(),
                data_type: global.data_type,
                address: global.address.clone(),
                line: 0,
                explicitly_mapped: true,
                retained: false,
                array: None,
            },
        );
    }
    for (device, width) in &options.reserved {
        e.pool.reserve(*device, *width);
    }
    if options.label_start > 0 {
        e.exit_label = e.label(Pos { line: 1, col: 1 })?;
    }
    // Reserve every explicit/direct address before placing any local or temp.
    for (name, p) in &unit.direct_names {
        if let Some(d) = device(name, *p)? {
            e.pool.reserve(
                d,
                if d.area == Area::D && d.index < 4999 {
                    2
                } else {
                    1
                },
            );
        }
    }
    if let Some(d) = unit.decls.iter().find(|d| d.retain && d.mapping.is_none()) {
        let p = d.names[0].1;
        options
            .retain
            .validate(&options.alloc)
            .map_err(|m| p.err(m))?;
    }
    for d in &unit.decls {
        if let Some((name, p)) = &d.mapping
            && let Some(dev) = device(name, *p)?
        {
            let (count, _) = array_bounds(d, *p)?;
            if dev.area == Area::M {
                for i in 0..count as u16 {
                    e.pool.reserve(
                        Device {
                            area: Area::M,
                            index: dev.index + i,
                        },
                        1,
                    );
                }
            } else {
                e.pool.reserve(dev, d.ty.words() * count);
            }
        }
    }
    e.declarations(&unit.decls)?;
    e.block(&unit.body)?;
    let p = unit
        .body
        .last()
        .map(|s| s.pos)
        .unwrap_or(Pos { line: 1, col: 1 });
    e.mark(p, e.exit_label);
    e.line(p, "END");
    Ok(LoweredSt {
        il: e.lines.join("\n") + "\n",
        line_map: e.line_map,
        symbols: e.symbols,
        next_label: e.labels,
    })
}
