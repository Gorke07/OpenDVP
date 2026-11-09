use super::syntax::{
    Arg, Decl, Expr, ExprKind, FbKind, Pos, SharedBlock, Stmt, StmtKind, Unit, reserved,
};
use super::{DataType as Ty, LoweredSt, StSymbol};
use crate::plc::{Area, Device, Diagnostic};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
enum Repr {
    Num(String),
    Bit(String, bool),
    Bool(bool),
    /// A BOOL expression not yet emitted: a contact network loaded with `LD`.
    Logic(Box<Logic>),
}
/// One contact of a contact network.
#[derive(Clone)]
enum Leaf {
    Bit(String),
    /// Comparison contact `{width}{LD|AND|OR}{op} a b`, width `""`, `"D"` or `"F"`.
    Cmp(&'static str, &'static str, String, String),
}
/// A BOOL expression as a series-parallel contact network. `NOT` is pushed down to the
/// contacts (De Morgan; a comparison flips its relation), so no `INV` is needed.
#[derive(Clone)]
enum Logic {
    Leaf(Leaf, bool),
    And(Box<Logic>, Box<Logic>),
    Or(Box<Logic>, Box<Logic>),
}
fn flip(op: &str) -> &'static str {
    match op {
        ">" => "<=",
        "<=" => ">",
        "<" => ">=",
        ">=" => "<",
        "=" => "<>",
        _ => "=",
    }
}
fn relation(op: &str) -> &'static str {
    match op {
        ">" => ">",
        "<=" => "<=",
        "<" => "<",
        ">=" => ">=",
        "=" => "=",
        _ => "<>",
    }
}
impl Logic {
    fn not(self) -> Logic {
        match self {
            Logic::Leaf(Leaf::Cmp(w, op, a, b), _) => Logic::Leaf(Leaf::Cmp(w, flip(op), a, b), false),
            Logic::Leaf(l, inv) => Logic::Leaf(l, !inv),
            Logic::And(a, b) => Logic::Or(Box::new(a.not()), Box::new(b.not())),
            Logic::Or(a, b) => Logic::And(Box::new(a.not()), Box::new(b.not())),
        }
    }
    fn contacts(&self) -> usize {
        match self {
            Logic::Leaf(..) => 1,
            Logic::And(a, b) | Logic::Or(a, b) => a.contacts() + b.contacts(),
        }
    }
    /// LD blocks open at once while this network is loaded with `LD`.
    fn blocks(&self) -> usize {
        match self {
            Logic::Leaf(..) => 1,
            Logic::And(a, b) => a.blocks().max(1 + b.series_blocks(true)),
            Logic::Or(a, b) => a.blocks().max(1 + b.series_blocks(false)),
        }
    }
    /// Extra LD blocks while this network is joined in series (`and`) or in parallel.
    fn series_blocks(&self, and: bool) -> usize {
        match (self, and) {
            (Logic::Leaf(..), _) => 0,
            (Logic::And(a, b), true) | (Logic::Or(a, b), false) => {
                a.series_blocks(and).max(b.series_blocks(and))
            }
            _ => self.blocks(),
        }
    }
}
/// Nested LD blocks a network may need (the PLC allows 8) and contacts per network.
const MAX_LOGIC_BLOCKS: usize = 6;
const MAX_LOGIC_CONTACTS: usize = 48;
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
            Repr::Logic(l) => **l = (**l).clone().not(),
            Repr::Num(_) => unreachable!(),
        };
        self
    }
    fn logic(&self) -> Logic {
        match &self.repr {
            Repr::Bit(s, inv) => Logic::Leaf(Leaf::Bit(s.clone()), *inv),
            Repr::Bool(b) => Logic::Leaf(Leaf::Bit("M1000".into()), !*b),
            Repr::Logic(l) => (**l).clone(),
            Repr::Num(_) => unreachable!("BOOL operand"),
        }
    }
    fn from_logic(l: Logic) -> Self {
        match l {
            Logic::Leaf(Leaf::Bit(s), inv) => Self {
                ty: Ty::Bool,
                repr: Repr::Bit(s, inv),
            },
            l => Self {
                ty: Ty::Bool,
                repr: Repr::Logic(Box::new(l)),
            },
        }
    }
}
struct Pool {
    words: BTreeSet<u16>,
    bits: BTreeSet<u16>,
    timers: BTreeSet<u16>,
    counters: BTreeSet<u16>,
    temps: Vec<(bool, u16)>,
    /// Every word / bit ever handed out as a temporary (released or not).
    used_temps: BTreeSet<(bool, u16)>,
    alloc: crate::alloc::Alloc,
    retain: crate::alloc::Retain,
}
impl Pool {
    fn new(alloc: crate::alloc::Alloc, retain: crate::alloc::Retain) -> Self {
        Self {
            words: BTreeSet::new(),
            bits: BTreeSet::new(),
            timers: BTreeSet::new(),
            counters: BTreeSet::new(),
            temps: vec![],
            used_temps: BTreeSet::new(),
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
        } else if d.area == Area::T {
            self.timers.insert(d.index);
        } else if d.area == Area::C {
            self.counters.insert(d.index);
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
                self.used_temps.insert((bit, x));
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
/// `T5` / `C3` for a timer or counter instance pinned with `AT`.
fn fb_device(raw: &str, timer: bool, p: Pos) -> Result<u16, Diagnostic> {
    let (letter, max) = if timer {
        ('T', MAX_TIMER)
    } else {
        ('C', MAX_COUNTER)
    };
    raw.strip_prefix(letter)
        .and_then(|n| n.parse::<u16>().ok())
        .filter(|n| *n <= max)
        .ok_or_else(|| {
            p.err(format!(
                "AT {raw} geçersiz: {} için {letter}0..{letter}{max} kullanın",
                if timer { "TON/TOF/TP" } else { "CTU/CTD" }
            ))
        })
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
                e.pos.err(format!(
                    "Dizi {what} sınırı -32768..32767 tamsayı sabiti olmalı"
                ))
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
/// `D<n>` (or an index-addressed `D<n>E6`) -> the word `n` further on.
fn offset(s: &str, n: u16) -> String {
    let (index, suffix) = word_index(s).expect("word operand");
    format!("D{}{suffix}", index + n)
}
/// `D12` -> (12, ""), `D12E6` -> (12, "E6").
fn word_index(s: &str) -> Option<(u16, &str)> {
    let rest = s.strip_prefix('D')?;
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    Some((rest[..end].parse().ok()?, &rest[end..]))
}
/// An operand addressed through an index register (`D3E6`, `M5E7`).
fn indexed(s: &str) -> bool {
    s.len() > 1 && s[1..].contains(['E', 'F'])
}
/// Extra words / bits a timer or counter instance keeps besides its device (`true` = bit),
/// in the order of `Fb::extra`.
fn fb_extras(kind: FbKind) -> &'static [(bool, &'static str)] {
    match kind {
        FbKind::Tof => &[(true, "Q")],
        FbKind::Tp => &[(true, "Q"), (true, "PREV")],
        FbKind::Ctd => &[(false, "CV"), (true, "PREV")],
        FbKind::Ctud => &[(false, "CV"), (false, "PV"), (true, "PREVU"), (true, "PREVD")],
        _ => &[],
    }
}
fn constant_only(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Name(_) | ExprKind::Index(..) | ExprKind::Member(..) | ExprKind::Call(..) => {
            false
        }
        ExprKind::List(items) => items.iter().all(constant_only),
        ExprKind::Unary(_, a) => constant_only(a),
        ExprKind::Binary(_, a, b) => constant_only(a) && constant_only(b),
        _ => true,
    }
}

/// A function-block instance: the timer/counter it owns plus its private state devices.
/// Where the variables of a shared block sit relative to an instance's word base (E6) and
/// bit base (E7). Every instance uses the same offsets.
#[derive(Clone, Default)]
struct SharedLayout {
    words: usize,
    bits: usize,
    /// Member suffix (`.NAME`), type, offset.
    plain: Vec<(String, Ty, usize)>,
    fbs: Vec<FbLayout>,
}
#[derive(Clone)]
struct FbLayout {
    suffix: String,
    kind: FbKind,
    /// Offsets of `Fb::extra` (`true` = bit).
    extra: Vec<(bool, usize)>,
    /// Bit holding a copy of the device contact (TON / TON_TICKS / CTU `Q`).
    q: Option<usize>,
    /// Word(s) holding a copy of ET (DINT ms, or INT ticks) or of a CTU's CV.
    value: Option<(usize, Ty)>,
}
impl SharedLayout {
    fn new(members: &[Decl], template: &str) -> Self {
        let mut l = Self::default();
        for d in members {
            for (name, _) in &d.names {
                let suffix = name[template.len()..].to_owned();
                let Some(kind) = d.fb else {
                    if d.ty == Ty::Bool {
                        l.plain.push((suffix, d.ty, l.bits));
                        l.bits += 1;
                    } else {
                        l.plain.push((suffix, d.ty, l.words));
                        l.words += d.ty.words();
                    }
                    continue;
                };
                let mut extra = Vec::new();
                for (bit, _) in fb_extras(kind) {
                    extra.push((*bit, if *bit { l.bits } else { l.words }));
                    if *bit {
                        l.bits += 1;
                    } else {
                        l.words += 1;
                    }
                }
                let q = matches!(kind, FbKind::Ton | FbKind::TonTicks | FbKind::Ctu).then(|| {
                    l.bits += 1;
                    l.bits - 1
                });
                let value = match kind {
                    FbKind::Ton | FbKind::Tof | FbKind::Tp => Some(Ty::Dint),
                    FbKind::TonTicks | FbKind::Ctu => Some(Ty::Int),
                    _ => None,
                }
                .map(|ty| {
                    l.words += ty.words();
                    (l.words - ty.words(), ty)
                });
                l.fbs.push(FbLayout {
                    suffix,
                    kind,
                    extra,
                    q,
                    value,
                });
            }
        }
        l
    }
}
/// A shared block body, emitted once after `FEND`.
struct Template {
    layout: SharedLayout,
    /// Subroutine label of each segment; a device call sits between two segments.
    segments: Vec<u8>,
    /// The device calls between the segments: member suffix and evaluated arguments.
    holes: Vec<(String, FbArgs)>,
    /// Bit set by a fault exit inside the body.
    abort: String,
    /// Segments with fault exits. Only these clear the abort bit on entry, so the bit is
    /// tested after their CALL only (elsewhere it may hold another body's temporary).
    faulting: Vec<bool>,
    /// Device copies the body reads: (member suffix, `true` = Q / `false` = ET or CV).
    reads: BTreeSet<(String, bool)>,
}
/// The evaluated arguments of a timer / counter call.
#[derive(Clone)]
struct FbArgs {
    input: Value,
    second: Option<Value>,
    preset: String,
    down: Option<Value>,
    reset: Option<Value>,
    load: Option<Value>,
}
/// Fault exits inside a shared body set `abort` and jump to the segment's end.
struct FaultExit {
    abort: String,
    end: Option<u8>,
}
#[derive(Clone)]
struct Fb {
    kind: FbKind,
    /// `T126` / `C110`.
    dev: String,
    /// TOF: [Q]; TP: [Q, previous IN]; CTD: [CV word, previous CD]; CTUD: [CV, PV, previous CU, previous CD]; others: none.
    extra: Vec<String>,
}
const FB_PARAMS_TIMER: &[&str] = &["IN", "PT"];
const FB_PARAMS_CTU: &[&str] = &["CU", "R", "PV"];
const FB_PARAMS_CTD: &[&str] = &["CD", "LD", "PV"];
const FB_PARAMS_CTUD: &[&str] = &["CU", "CD", "R", "LD", "PV"];
/// `[length, packed characters...]` as signed 16-bit words, two characters per word (low byte first).
fn string_words(text: &str, cap: usize, p: Pos) -> Result<Vec<i16>, Diagnostic> {
    let mut bytes = Vec::new();
    for ch in text.chars() {
        let c = u32::from(ch);
        if c > 255 {
            return Err(p.err(format!("STRING sabitinde desteklenmeyen karakter: {ch}")));
        }
        bytes.push(c as u8);
    }
    if bytes.len() > cap {
        return Err(p.err(format!(
            "Metin {} karakter; STRING kapasitesi {cap}",
            bytes.len()
        )));
    }
    let mut words = vec![bytes.len() as i16];
    for w in 0..cap.div_ceil(2) {
        let low = u16::from(*bytes.get(2 * w).unwrap_or(&0));
        let high = u16::from(*bytes.get(2 * w + 1).unwrap_or(&0));
        words.push((low | high << 8) as i16);
    }
    Ok(words)
}
/// Constant PT (milliseconds) / PV (count) as a `K` operand.
fn constant_preset(n: i64, timer: bool, p: Pos) -> Result<String, Diagnostic> {
    if timer {
        if n % 100 != 0 || !(0..=3_276_700).contains(&n) {
            return Err(p.err(
                "PT, 100 ms'nin katı olmalı ve en çok T#3276s700ms olabilir (T0..T126 100 ms taban); değişken PT her değeri kabul eder",
            ));
        }
        Ok(format!("K{}", n / 100))
    } else if (1..=32767).contains(&n) {
        Ok(format!("K{n}"))
    } else {
        Err(p.err("PV 1..32767 olmalı"))
    }
}
/// Automatic timers come from the 100 ms class T0..T126, counters from C0..C111.
const MAX_TIMER: u16 = 126;
const MAX_COUNTER: u16 = 111;

fn type_by_name(name: &str) -> Option<Ty> {
    Some(match name {
        "INT" => Ty::Int,
        "DINT" => Ty::Dint,
        "WORD" => Ty::Word,
        "DWORD" => Ty::Dword,
        "REAL" => Ty::Real,
        _ => return None,
    })
}
/// `INT_TO_REAL` -> (INT, REAL)
fn conversion(name: &str) -> Option<(Ty, Ty)> {
    let (a, b) = name.split_once("_TO_")?;
    Some((type_by_name(a)?, type_by_name(b)?))
}
fn move_cmd(ty: Ty, src: &Value) -> &'static str {
    if ty == Ty::Real && src.operand().starts_with('F') {
        "DMOVR"
    } else if ty.words() == 2 {
        "DMOV"
    } else {
        "MOV"
    }
}

struct Emitter {
    fbs: BTreeMap<String, Fb>,
    /// STRING variables: name -> capacity in characters (stored as an INT array).
    strings: BTreeMap<String, usize>,
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
    /// Two DINT words pairs whose high words the program clears every scan: INT `+`/`-`
    /// zero-extends its operands into them (see `wrapping_arithmetic`).
    int_scratch: Option<(String, String)>,
    /// `lines.len()` right after the last `action`, and how many actions share its `LD M1000`.
    action_tail: usize,
    action_run: usize,
    /// Shared block bodies by block type.
    templates: BTreeMap<String, Template>,
    /// Shared instance -> (word base, bit base).
    bases: BTreeMap<String, (u16, u16)>,
    /// Template variables, as index-addressed operands.
    indexed: BTreeMap<String, Value>,
    /// Template timer / counter instance -> (template name, member suffix, layout).
    template_fbs: BTreeMap<String, (String, FbLayout)>,
    /// Instance variables placed by a shared layout (their declarations allocate nothing).
    preplaced: BTreeSet<String>,
    /// IL of the shared bodies (after `FEND`) and its source lines.
    sub_lines: Vec<String>,
    sub_map: Vec<usize>,
    /// Set while a shared body is emitted.
    fault: Option<FaultExit>,
    /// Device copies the shared body being emitted reads (see `Template::reads`).
    reads: BTreeSet<(String, bool)>,
}
/// Actions that may share one `LD M1000`; a fresh one keeps networks well below the
/// 512-instruction network limit.
const ACTION_RUN: usize = 32;
impl Emitter {
    fn writes_control(&self, body: &[Stmt], control: &Value) -> Result<bool, Diagnostic> {
        let target = word_index(control.operand());
        let hits = |name: &str, p: Pos| -> Result<bool, Diagnostic> {
            let v = self.lookup(name, p)?;
            if v.ty == Ty::Bool {
                return Ok(false);
            }
            Ok(match (word_index(v.operand()), target) {
                (Some((start, a)), Some((t, b))) => {
                    a == b && (start..start + v.ty.words() as u16).contains(&t)
                }
                _ => true,
            })
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
        // Consecutive actions with nothing in between run under the same always-on contact.
        if self.action_tail == self.lines.len() && self.action_run < ACTION_RUN {
            self.action_run += 1;
        } else {
            self.line(p, "LD M1000");
            self.action_run = 1;
        }
        self.line(p, s);
        self.action_tail = self.lines.len();
    }
    /// Emit a contact network joined with `prefix` (`LD`, `AND` or `OR`).
    fn logic_contacts(&mut self, p: Pos, l: &Logic, prefix: &str) {
        match (l, prefix) {
            (Logic::Leaf(Leaf::Cmp(w, op, a, b), _), _) => {
                self.line(p, format!("{w}{prefix}{op} {a} {b}"));
            }
            (Logic::Leaf(Leaf::Bit(bit), inv), _) => {
                let op = match (prefix, inv) {
                    ("LD", true) => "LDI",
                    ("AND", true) => "ANI",
                    ("OR", true) => "ORI",
                    (s, _) => s,
                };
                self.line(p, format!("{op} {bit}"));
            }
            (Logic::And(a, b), "LD" | "AND") => {
                self.logic_contacts(p, a, prefix);
                self.logic_contacts(p, b, "AND");
            }
            (Logic::Or(a, b), "LD" | "OR") => {
                self.logic_contacts(p, a, prefix);
                self.logic_contacts(p, b, "OR");
            }
            (block, _) => {
                self.logic_contacts(p, block, "LD");
                self.line(p, if prefix == "AND" { "ANB" } else { "ORB" });
            }
        }
    }
    /// A BOOL value held in a bit: a pending network is evaluated into a temporary.
    fn bit_value(&mut self, p: Pos, v: Value) -> Result<Value, Diagnostic> {
        let Repr::Logic(l) = &v.repr else {
            return Ok(v);
        };
        let l = (**l).clone();
        let out = self.temp(Ty::Bool, p)?;
        self.logic_contacts(p, &l, "LD");
        let Repr::Bit(d, _) = &out.repr else {
            unreachable!()
        };
        self.line(p, format!("OUT {d}"));
        Ok(out)
    }
    fn bool_contact(&mut self, p: Pos, v: &Value, prefix: &str) {
        if let Repr::Logic(l) = &v.repr {
            let l = (**l).clone();
            self.logic_contacts(p, &l, prefix);
            return;
        }
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
        if let Some(v) = self.indexed.get(name) {
            return Ok(v.clone());
        }
        if let Some(&n) = self.names.get(name) {
            let s = &self.symbols[n];
            if self.strings.contains_key(name) {
                return Err(p.err(format!(
                    "{name} bir STRING; yalnız atama, = / <> karşılaştırması ve LEN({name}) ile kullanılır"
                )));
            }
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
        if let Some((instance, _)) = name.rsplit_once('.')
            && self.fbs.contains_key(instance)
        {
            return Err(p.err(format!("{name} bir fonksiyon bloğu çıkışıdır; salt okunur")));
        }
        if self.fbs.contains_key(name) {
            return Err(p.err(format!(
                "{name} bir fonksiyon bloğudur; çıkışları {name}.Q / {name}.ET / {name}.CV ile okuyun"
            )));
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
        if let Some(v) = self.indexed.get(name) {
            return Ok(v.clone());
        }
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
            ExprKind::Str(_) | ExprKind::List(_) => None,
            ExprKind::Member(n, m) => Some(self.member_type(n, m, e.pos)?),
            ExprKind::Call(n, args) => self.call_type(n, args, e.pos)?,
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
            (Some(Ty::Int | Ty::Dint | Ty::Word), Some(Ty::Real))
            | (Some(Ty::Real), Some(Ty::Int | Ty::Dint | Ty::Word)) => Ok(Some(Ty::Real)),
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
        if to == Ty::Real && matches!(v.ty, Ty::Int | Ty::Dint | Ty::Word) {
            return self.convert(v, to, p);
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
        let (mut a, mut b) = (a, b);
        if op == "XOR" {
            // Each side appears twice; a network side is evaluated once into a bit.
            a = self.bit_value(p, a)?;
            b = self.bit_value(p, b)?;
        }
        let join = |a: &Value, b: &Value| {
            let (x, y) = (a.logic(), b.logic());
            match op {
                "XOR" => Logic::Or(
                    Box::new(Logic::And(Box::new(x.clone()), Box::new(y.clone().not()))),
                    Box::new(Logic::And(Box::new(x.not()), Box::new(y))),
                ),
                "OR" => Logic::Or(Box::new(x), Box::new(y)),
                _ => Logic::And(Box::new(x), Box::new(y)),
            }
        };
        let fits = |l: &Logic| l.blocks() <= MAX_LOGIC_BLOCKS && l.contacts() <= MAX_LOGIC_CONTACTS;
        let mut l = join(&a, &b);
        if !fits(&l) {
            b = self.bit_value(p, b)?;
            l = join(&a, &b);
        }
        if !fits(&l) {
            a = self.bit_value(p, a)?;
            l = join(&a, &b);
        }
        Ok(Value::from_logic(l))
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
        let width = if ty == Ty::Real {
            "F"
        } else if ty.words() == 2 {
            "D"
        } else {
            ""
        };
        Ok(Value::from_logic(Logic::Leaf(
            Leaf::Cmp(width, relation(op), a.operand().to_owned(), b.operand().to_owned()),
            false,
        )))
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
    /// The program's two INT scratch pairs, allocated on first use.
    fn int_scratch(&mut self, p: Pos) -> Result<(String, String), Diagnostic> {
        if let Some(pair) = &self.int_scratch {
            return Ok(pair.clone());
        }
        let a = self.pool.allocate(Ty::Dint, 2, false, p)?;
        let b = self.pool.allocate(Ty::Dint, 2, false, p)?;
        for (address, label) in [(&a, "A"), (&b, "B")] {
            // Listed so other programs of the project keep clear of these words.
            self.symbols.push(StSymbol {
                name: format!("__INT_SCRATCH_{label}"),
                data_type: Ty::Dint,
                address: address.clone(),
                line: p.line,
                explicitly_mapped: false,
                retained: false,
                array: None,
            });
        }
        self.int_scratch = Some((a.clone(), b.clone()));
        Ok((a, b))
    }
    /// A 16-bit operand as a 32-bit one with a zero high word: a constant directly, anything
    /// else copied into the low word of `pair` (whose high word is kept at zero).
    fn zero_extended(&mut self, p: Pos, v: &Value, pair: &str) -> String {
        if let Some(n) = v.operand().strip_prefix('K').and_then(|n| n.parse::<i32>().ok()) {
            return format!("K{}", n as u16);
        }
        self.action(p, format!("MOV {} {pair}", v.operand()));
        pair.to_owned()
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
            // The low 16 bits of a sum or difference depend only on the low 16 bits of the
            // operands, so zero extension is enough: the 32-bit result cannot overflow, and its
            // low word is the INT result with two's-complement wrap-around.
            let (sa, sb) = self.int_scratch(p)?;
            let za = self.zero_extended(p, &a, &sa);
            let zb = self.zero_extended(p, &b, &sb);
            let wide = self.temp(Ty::Dint, p)?;
            self.action(
                p,
                format!(
                    "{} {za} {zb} {}",
                    if op == "+" { "DADD" } else { "DSUB" },
                    wide.operand()
                ),
            );
            return Ok(Value::num(Ty::Int, wide.operand()));
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
            Some(_) if self.strings.contains_key(name) => Err(p.err(format!(
                "{name} bir STRING; karakter indeksi desteklenmiyor"
            ))),
            Some(s) if s.array.is_some() => Ok(s),
            Some(_) => Err(p.err(format!("{name} dizi değil; indekslenemez"))),
            None => Err(p.err(format!("Tanımsız dizi: {name}"))),
        }
    }
    /// An `ARRAY` element (BOOL arrays use `M<base>E0`). A constant index is the fixed device `base + (i-lo)*width` and is
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
        let operand = format!("{area}{base}E0");
        if bit {
            if !read {
                return Ok(Value::bit(operand));
            }
            let out = self.temp(Ty::Bool, p)?;
            let Repr::Bit(d, _) = &out.repr else {
                unreachable!()
            };
            self.line(p, format!("LD {operand}"));
            self.line(p, format!("OUT {d}"));
            return Ok(out);
        }
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
                if matches!(op.as_str(), "=" | "<>") && (self.is_string(a) || self.is_string(b)) {
                    return self.string_compare(op, a, b, e.pos);
                }
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
                if ty == Ty::Real && op == "MOD" {
                    // a - b * trunc(a / b); the quotient must fit a DINT.
                    let q = self.temp(ty, e.pos)?;
                    self.action(
                        e.pos,
                        format!("DDIVR {} {} {}", av.operand(), bv.operand(), q.operand()),
                    );
                    self.real_fault_exit(e.pos, &av, Some(&bv), true)?;
                    let whole = self.temp(Ty::Dint, e.pos)?;
                    self.action(e.pos, format!("DINT {} {}", q.operand(), whole.operand()));
                    self.real_fault_exit(e.pos, &q, None, false)?;
                    let back = self.temp(ty, e.pos)?;
                    self.action(
                        e.pos,
                        format!("DFLT {} {}", whole.operand(), back.operand()),
                    );
                    let product = self.temp(ty, e.pos)?;
                    self.action(
                        e.pos,
                        format!(
                            "DMULR {} {} {}",
                            bv.operand(),
                            back.operand(),
                            product.operand()
                        ),
                    );
                    let out = self.temp(ty, e.pos)?;
                    self.action(
                        e.pos,
                        format!(
                            "DSUBR {} {} {}",
                            av.operand(),
                            product.operand(),
                            out.operand()
                        ),
                    );
                    return Ok(out);
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
                    self.zero_divisor_exit(e.pos, &bv)?;
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
                    self.zero_divisor_exit(e.pos, &bv)?;
                }
                Ok(out)
            }
            ExprKind::Member(n, m) => self.member(n, m, e.pos),
            ExprKind::Call(n, args) => self.call_expr(n, args, e.pos, hint),
            ExprKind::Str(_) => Err(e
                .pos
                .err("Metin yalnız STRING ataması ve = / <> karşılaştırmasında kullanılır")),
            ExprKind::List(_) => Err(e.pos.err("[...] listesi yalnız dizi başlangıç değeridir")),
            ExprKind::Number(_) => unreachable!(),
        }
    }
    fn zero_divisor_exit(&mut self, p: Pos, b: &Value) -> Result<(), Diagnostic> {
        self.line(
            p,
            format!(
                "{}= {} K0",
                if b.ty.words() == 2 { "DLD" } else { "LD" },
                b.operand()
            ),
        );
        self.fault_jump(p)
    }
    /// Leave the scan when the condition just loaded holds. Inside a shared body the fault is
    /// recorded in its abort bit instead; the caller leaves the scan after the `CALL`.
    fn fault_jump(&mut self, p: Pos) -> Result<(), Diagnostic> {
        if self.fault.is_none() {
            self.line(p, "CJ P0");
            return Ok(());
        }
        let end = match self.fault.as_ref().and_then(|f| f.end) {
            Some(end) => end,
            None => {
                let end = self.label(p)?;
                self.fault.as_mut().unwrap().end = Some(end);
                end
            }
        };
        let abort = self.fault.as_ref().unwrap().abort.clone();
        self.line(p, format!("SET {abort}"));
        self.line(p, format!("CJ P{end}"));
        Ok(())
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
                self.fault_jump(p)?;
            }
        }
        if division {
            let b = b.expect("binary REAL division");
            if let Some(n) = b.operand().strip_prefix('F') {
                let n = n.parse::<f32>().expect("validated finite literal");
                if n.to_bits() & 0x7f80_0000 == 0 {
                    if self.fault.is_some() {
                        self.line(p, "LD M1000");
                        self.fault_jump(p)?;
                    } else {
                        self.go(p, self.exit_label);
                    }
                }
            } else {
                let exponent = self.temp(Ty::Dword, p)?;
                self.action(
                    p,
                    format!("DAND {} H7F800000 {}", b.operand(), exponent.operand()),
                );
                self.line(p, format!("DLD= {} K0", exponent.operand()));
                self.fault_jump(p)?;
            }
        }
        Ok(())
    }
    fn add_fb(
        &mut self,
        name: &str,
        kind: FbKind,
        mapping: Option<&(String, Pos)>,
        p: Pos,
    ) -> Result<(), Diagnostic> {
        self.add_fb_with(name, kind, mapping, p, None)
    }
    /// `extras`: the instance's extra words / bits, already placed by a shared layout.
    fn add_fb_with(
        &mut self,
        name: &str,
        kind: FbKind,
        mapping: Option<&(String, Pos)>,
        p: Pos,
        extras: Option<Vec<String>>,
    ) -> Result<(), Diagnostic> {
        let timer = kind.is_timer();
        let number = match mapping {
            Some((raw, mp)) => fb_device(raw, timer, *mp)?,
            None => {
                let (set, top) = if timer {
                    (&self.pool.timers, MAX_TIMER)
                } else {
                    (&self.pool.counters, MAX_COUNTER)
                };
                let n = (0..=top).rev().find(|n| !set.contains(n)).ok_or_else(|| {
                    p.err(if timer {
                        "T0..T126 zamanlayıcıları dolu"
                    } else {
                        "C0..C111 sayaçları dolu"
                    })
                })?;
                if timer {
                    self.pool.timers.insert(n);
                } else {
                    self.pool.counters.insert(n);
                }
                n
            }
        };
        let dev = format!("{}{number}", if timer { 'T' } else { 'C' });
        let labels: Vec<&str> = fb_extras(kind).iter().map(|(_, l)| *l).collect();
        let extra = match extras {
            Some(extra) => extra,
            None => {
                let mut extra = Vec::new();
                for (bit, _) in fb_extras(kind) {
                    let ty = if *bit { Ty::Bool } else { Ty::Int };
                    extra.push(self.pool.allocate(ty, 1, false, p)?);
                }
                extra
            }
        };
        // Listed as symbols so other programs of the project keep clear of these devices.
        self.symbols.push(StSymbol {
            name: name.to_owned(),
            data_type: Ty::Int,
            address: dev.clone(),
            line: p.line,
            explicitly_mapped: mapping.is_some(),
            retained: false,
            array: None,
        });
        for (address, label) in extra.iter().zip(labels) {
            self.symbols.push(StSymbol {
                name: format!("{name}.{label}"),
                data_type: if address.starts_with('D') {
                    Ty::Int
                } else {
                    Ty::Bool
                },
                address: address.clone(),
                line: p.line,
                explicitly_mapped: false,
                retained: false,
                array: None,
            });
        }
        self.fbs.insert(name.to_owned(), Fb { kind, dev, extra });
        Ok(())
    }
    fn member_type(&self, name: &str, member: &str, p: Pos) -> Result<Ty, Diagnostic> {
        let full = format!("{name}.{member}");
        if self.names.contains_key(&full) || self.indexed.contains_key(&full) {
            return Ok(self.lookup(&format!("{name}.{member}"), p)?.ty);
        }
        let fb = self
            .fbs
            .get(name)
            .ok_or_else(|| p.err(format!("{name} bir fonksiyon bloğu örneği değil")))?;
        let timer = fb.kind.is_timer();
        let up_down = fb.kind == FbKind::Ctud;
        match (member, timer) {
            ("Q", _) if !up_down => Ok(Ty::Bool),
            ("QU" | "QD", false) if up_down => Ok(Ty::Bool),
            ("ET", true) if fb.kind == FbKind::TonTicks => Ok(Ty::Int),
            ("ET", true) => Ok(Ty::Dint),
            ("CV", false) => Ok(Ty::Int),
            _ => Err(p.err(format!(
                "{name}.{member} yok; kullanılabilir çıkışlar: {}",
                if timer {
                    "Q, ET"
                } else if up_down {
                    "QU, QD, CV"
                } else {
                    "Q, CV"
                }
            ))),
        }
    }
    fn member(&mut self, name: &str, member: &str, p: Pos) -> Result<Value, Diagnostic> {
        let full = format!("{name}.{member}");
        if self.names.contains_key(&full) || self.indexed.contains_key(&full) {
            return self.lookup(&format!("{name}.{member}"), p);
        }
        self.member_type(name, member, p)?;
        // In a shared body a device's contact and value are read from the instance's copies.
        if let Some((_, l)) = self.template_fbs.get(name).cloned() {
            let q = member == "Q";
            let copy = match (q, l.q, l.value) {
                (true, Some(q), _) => Some(Value::bit(format!("M{q}E7"))),
                (false, _, Some((at, ty))) if matches!(member, "ET" | "CV") => {
                    Some(Value::num(ty, format!("D{at}E6")))
                }
                _ => None,
            };
            if let Some(v) = copy {
                self.reads.insert((l.suffix.clone(), q));
                return Ok(v);
            }
        }
        let fb = self.fbs.get(name).cloned().unwrap();
        Ok(match (fb.kind, member) {
            (FbKind::Ton | FbKind::TonTicks | FbKind::Ctu, "Q") => Value::bit(fb.dev),
            // ET in 100 ms ticks: the timer word itself.
            (FbKind::TonTicks, _) => Value::num(Ty::Int, fb.dev),
            (FbKind::Tof | FbKind::Tp, "Q") => Value::bit(fb.extra[0].clone()),
            (FbKind::Ton | FbKind::Tof | FbKind::Tp, _) => {
                // ET in milliseconds: elapsed 100 ms ticks x 100.
                let out = self.temp(Ty::Dint, p)?;
                self.action(p, format!("MUL {} K100 {}", fb.dev, out.operand()));
                out
            }
            (FbKind::Ctu, _) => Value::num(Ty::Int, fb.dev),
            (FbKind::Ctud, "CV") => Value::num(Ty::Int, fb.extra[0].clone()),
            (FbKind::Ctud, name) => {
                let out = self.temp(Ty::Bool, p)?;
                if name == "QU" {
                    self.line(p, format!("LD>= {} {}", fb.extra[0], fb.extra[1]));
                } else {
                    self.line(p, format!("LD<= {} K0", fb.extra[0]));
                }
                let Repr::Bit(d, _) = &out.repr else {
                    unreachable!()
                };
                self.line(p, format!("OUT {d}"));
                out
            }
            (FbKind::Ctd, "CV") => Value::num(Ty::Int, fb.extra[0].clone()),
            (FbKind::Ctd, _) => {
                let out = self.temp(Ty::Bool, p)?;
                self.line(p, format!("LD<= {} K0", fb.extra[0]));
                let Repr::Bit(d, _) = &out.repr else {
                    unreachable!()
                };
                self.line(p, format!("OUT {d}"));
                out
            }
        })
    }
    fn bool_param(&mut self, e: &Expr) -> Result<Value, Diagnostic> {
        let v = self.eval(e, Some(Ty::Bool))?;
        if v.ty != Ty::Bool {
            return Err(e.pos.err("Bu parametre BOOL olmalı"));
        }
        self.bit_value(e.pos, v)
    }
    fn stmt_call(&mut self, name: &str, args: &[Arg], p: Pos) -> Result<(), Diagnostic> {
        let (fb, a) = self.fb_args(name, args, p)?;
        self.fb_emit(p, &fb, &a)
    }
    /// Evaluate the arguments of a timer / counter call (code for them is emitted here).
    fn fb_args(&mut self, name: &str, args: &[Arg], p: Pos) -> Result<(Fb, FbArgs), Diagnostic> {
        let Some(fb) = self.fbs.get(name).cloned() else {
            return Err(p.err(format!(
                "{name} bir fonksiyon bloğu örneği değil; fonksiyonlar ifade içinde kullanılır (x := {name}(...))"
            )));
        };
        let params = match fb.kind {
            FbKind::Ton | FbKind::Tof | FbKind::Tp | FbKind::TonTicks => FB_PARAMS_TIMER,
            FbKind::Ctu => FB_PARAMS_CTU,
            FbKind::Ctd => FB_PARAMS_CTD,
            FbKind::Ctud => FB_PARAMS_CTUD,
        };
        let mut given: Vec<Option<&Expr>> = vec![None; params.len()];
        for (i, (n, e)) in args.iter().enumerate() {
            let at = match n {
                Some(n) => params.iter().position(|x| x == n).ok_or_else(|| {
                    e.pos.err(format!(
                        "{name}: bilinmeyen parametre {n}; kullanılabilir: {}",
                        params.join(", ")
                    ))
                })?,
                None => i,
            };
            if at >= params.len() {
                return Err(e
                    .pos
                    .err(format!("{name}: en fazla {} parametre alır", params.len())));
            }
            if given[at].replace(e).is_some() {
                return Err(e.pos.err(format!("{name}: {} iki kez verildi", params[at])));
            }
        }
        let missing = |k: usize| p.err(format!("{name}: {} parametresi gerekli", params[k]));
        let first = given[0].ok_or_else(|| missing(0))?;
        let timer = fb.kind.is_timer();
        let preset_at = match fb.kind {
            FbKind::Ton | FbKind::Tof | FbKind::Tp | FbKind::TonTicks => 1,
            FbKind::Ctu | FbKind::Ctd => 2,
            FbKind::Ctud => 4,
        };
        let preset_expr = given[preset_at].ok_or_else(|| missing(preset_at))?;
        // A constant preset becomes a K operand; anything else is computed into a word each scan.
        let preset = if fb.kind == FbKind::TonTicks {
            // Ticks need no conversion; the caller keeps them in 0..32767.
            let v = self.eval(preset_expr, Some(Ty::Int))?;
            if v.ty != Ty::Int {
                return Err(preset_expr.pos.err("PT bir INT ifadesi olmalı (100 ms tik)"));
            }
            v.operand().to_owned()
        } else {
            match integer(preset_expr) {
                Some(n) => constant_preset(n, timer, preset_expr.pos)?,
                None => self.variable_preset(preset_expr, timer)?,
            }
        };
        let input = self.bool_param(first)?;
        let second = match (fb.kind, given[1]) {
            (FbKind::Ctu | FbKind::Ctd, Some(e)) => Some(self.bool_param(e)?),
            _ => None,
        };
        let (mut down, mut reset, mut load) = (None, None, None);
        if fb.kind == FbKind::Ctud {
            down = Some(self.bool_param(given[1].ok_or_else(|| missing(1))?)?);
            reset = match given[2] {
                Some(e) => Some(self.bool_param(e)?),
                None => None,
            };
            load = match given[3] {
                Some(e) => Some(self.bool_param(e)?),
                None => None,
            };
        }
        Ok((
            fb,
            FbArgs {
                input,
                second,
                preset,
                down,
                reset,
                load,
            },
        ))
    }
    /// The instructions of a timer / counter call, its arguments already evaluated.
    fn fb_emit(&mut self, p: Pos, fb: &Fb, a: &FbArgs) -> Result<(), Diagnostic> {
        let (input, second, preset) = (a.input.clone(), a.second.clone(), a.preset.clone());
        let dev = fb.dev.clone();
        match fb.kind {
            FbKind::Ton | FbKind::TonTicks => {
                self.bool_contact(p, &input, "LD");
                self.line(p, format!("TMR {dev} {preset}"));
            }
            FbKind::Tof => {
                let q = &fb.extra[0];
                // Q := IN OR (Q AND NOT timer-done); the timer runs while IN is low and Q still high.
                self.bool_contact(p, &input, "LD");
                self.line(p, format!("LD {q}"));
                self.line(p, format!("ANI {dev}"));
                self.line(p, "ORB");
                self.line(p, format!("OUT {q}"));
                self.bool_contact(p, &input.clone().inverted(), "LD");
                self.line(p, format!("AND {q}"));
                self.line(p, format!("TMR {dev} {preset}"));
            }
            FbKind::Tp => {
                let (q, prev) = (&fb.extra[0], &fb.extra[1]);
                // Pulse on a rising edge of IN; further edges are ignored while the pulse runs.
                self.bool_contact(p, &input, "LD");
                self.line(p, format!("ANI {prev}"));
                self.line(p, format!("ANI {q}"));
                self.line(p, format!("LD {q}"));
                self.line(p, format!("ANI {dev}"));
                self.line(p, "ORB");
                self.line(p, format!("OUT {q}"));
                self.bool_contact(p, &input, "LD");
                self.line(p, format!("OUT {prev}"));
                self.line(p, format!("LD {q}"));
                self.line(p, format!("TMR {dev} {preset}"));
            }
            FbKind::Ctu => {
                self.bool_contact(p, &input, "LD");
                self.line(p, format!("CNT {dev} {preset}"));
                if let Some(reset) = second {
                    self.bool_contact(p, &reset, "LD");
                    self.line(p, format!("RST {dev}"));
                }
            }
            FbKind::Ctd => {
                let (cv, prev) = (&fb.extra[0], &fb.extra[1]);
                // Count down once per rising edge of CD while CV > 0; LD reloads CV with PV.
                self.bool_contact(p, &input, "LD");
                self.line(p, format!("ANI {prev}"));
                self.line(p, format!("AND> {cv} K0"));
                self.line(p, format!("DEC {cv}"));
                self.bool_contact(p, &input, "LD");
                self.line(p, format!("OUT {prev}"));
                if let Some(load) = second {
                    self.bool_contact(p, &load, "LD");
                    self.line(p, format!("MOV {preset} {cv}"));
                }
            }
            FbKind::Ctud => {
                let (cv, pv, prev_up, prev_down) =
                    (&fb.extra[0], &fb.extra[1], &fb.extra[2], &fb.extra[3]);
                let down = a.down.clone().expect("CTUD CD");
                let (reset, load) = (a.reset.clone(), a.load.clone());
                self.action(p, format!("MOV {preset} {pv}"));
                // Up on a rising CU while CV < PV; down on a rising CD while CV > 0.
                self.bool_contact(p, &input, "LD");
                self.line(p, format!("ANI {prev_up}"));
                self.line(p, format!("AND< {cv} {pv}"));
                self.line(p, format!("INC {cv}"));
                self.bool_contact(p, &input, "LD");
                self.line(p, format!("OUT {prev_up}"));
                self.bool_contact(p, &down, "LD");
                self.line(p, format!("ANI {prev_down}"));
                self.line(p, format!("AND> {cv} K0"));
                self.line(p, format!("DEC {cv}"));
                self.bool_contact(p, &down, "LD");
                self.line(p, format!("OUT {prev_down}"));
                // LD reloads PV, R clears (R wins).
                if let Some(load) = load {
                    self.bool_contact(p, &load, "LD");
                    self.line(p, format!("MOV {pv} {cv}"));
                }
                if let Some(reset) = reset {
                    self.bool_contact(p, &reset, "LD");
                    self.line(p, format!("MOV K0 {cv}"));
                }
            }
        }
        Ok(())
    }
    /// PT/PV given as a variable: 100 ms ticks (PT) or the count, as a word operand.
    fn variable_preset(&mut self, e: &Expr, timer: bool) -> Result<String, Diagnostic> {
        let v = self.eval(e, Some(Ty::Int))?;
        if !matches!(v.ty, Ty::Int | Ty::Word) {
            return Err(e.pos.err(if timer {
                "PT bir INT ifadesi olmalı (milisaniye); DINT için DINT_TO_INT(...) kullanın"
            } else {
                "PV bir INT ifadesi olmalı; DINT için DINT_TO_INT(...) kullanın"
            }));
        }
        let operand = v.operand().to_owned();
        if let Some(n) = operand
            .strip_prefix('K')
            .and_then(|n| n.parse::<i64>().ok())
        {
            return constant_preset(n, timer, e.pos);
        }
        if !timer {
            return Ok(operand);
        }
        let ticks = self.temp(Ty::Dint, e.pos)?;
        self.action(e.pos, format!("DIV {operand} K100 {}", ticks.operand()));
        Ok(ticks.operand().to_owned())
    }
    /// Is the value held in a temporary or device (not a K/H/F literal)? Literals are copied first.
    fn materialize(&mut self, v: Value, p: Pos) -> Result<Value, Diagnostic> {
        let Repr::Num(s) = &v.repr else {
            return Ok(v);
        };
        if s.starts_with(['K', 'H', 'F']) {
            let out = self.temp(v.ty, p)?;
            self.action(p, format!("{} {} {}", move_cmd(v.ty, &v), s, out.operand()));
            return Ok(out);
        }
        Ok(v)
    }
    fn alias(v: Value, to: Ty) -> Value {
        Value {
            ty: to,
            repr: v.repr,
        }
    }
    fn convert(&mut self, v: Value, to: Ty, p: Pos) -> Result<Value, Diagnostic> {
        use Ty::*;
        if v.ty == Bool || to == Bool {
            return Err(p.err("BOOL dönüşümü yok; karşılaştırma (x <> 0) kullanın"));
        }
        if v.ty == to {
            return Ok(v);
        }
        Ok(match (v.ty, to) {
            (Int, Dint) | (Word, Dword) => self.coerce(v, to, p)?,
            (Int, Word) | (Word, Int) | (Dint, Dword) | (Dword, Dint) => Self::alias(v, to),
            (Word, Dint) => {
                let wide = self.coerce(v, Dword, p)?;
                Self::alias(wide, Dint)
            }
            (Int, Dword) => {
                let wide = self.coerce(v, Dint, p)?;
                Self::alias(wide, Dword)
            }
            (Dint | Dword, Int | Word) => {
                // The low word; the high word is discarded.
                let src = self.materialize(v, p)?;
                let out = self.temp(to, p)?;
                self.action(p, format!("MOV {} {}", src.operand(), out.operand()));
                out
            }
            (Int, Real) | (Dint, Real) => {
                let src = self.materialize(v.clone(), p)?;
                let out = self.temp(Real, p)?;
                let cmd = if v.ty == Int { "FLT" } else { "DFLT" };
                self.action(p, format!("{cmd} {} {}", src.operand(), out.operand()));
                out
            }
            (Word, Real) => {
                let wide = self.coerce(v, Dword, p)?;
                return self.convert(Self::alias(wide, Dint), Real, p);
            }
            (Real, Int) | (Real, Dint) => {
                let src = self.materialize(v, p)?;
                let out = self.temp(to, p)?;
                let cmd = if to == Int { "INT" } else { "DINT" };
                self.action(p, format!("{cmd} {} {}", src.operand(), out.operand()));
                self.real_fault_exit(p, &src, None, false)?;
                out
            }
            (Real, Word) => {
                let i = self.convert(v, Int, p)?;
                Self::alias(i, Word)
            }
            (Real, Dword) => {
                let i = self.convert(v, Dint, p)?;
                Self::alias(i, Dword)
            }
            (Dword, Real) => {
                return Err(p.err(
                    "DWORD_TO_REAL desteklenmiyor (işaretsiz 32 bit); önce DWORD_TO_DINT kullanın",
                ));
            }
            _ => return Err(p.err(format!("{} -> {to} dönüşümü desteklenmiyor", v.ty))),
        })
    }
    fn arith_type(&self, args: &[&Expr], hint: Option<Ty>, p: Pos) -> Result<Ty, Diagnostic> {
        let mut ty = None;
        for a in args {
            ty = self.common(ty, self.ty_of(a)?, None, p)?;
        }
        let wide = args
            .iter()
            .filter_map(|a| integer(a))
            .any(|n| !(-32768..=32767).contains(&n));
        Ok(ty.or(hint).unwrap_or(if wide { Ty::Dint } else { Ty::Int }))
    }
    fn call_type(&self, name: &str, args: &[Arg], p: Pos) -> Result<Option<Ty>, Diagnostic> {
        if self.fbs.contains_key(name) {
            return Err(p.err(format!(
                "{name} fonksiyon bloğudur; çağrıyı ifade dışında yapın ve {name}.Q okuyun"
            )));
        }
        let a: Vec<&Expr> = args.iter().map(|(_, e)| e).collect();
        if let Some((_, to)) = conversion(name) {
            return Ok(Some(to));
        }
        Ok(match name {
            "ABS" | "MIN" | "MAX" | "LIMIT" => Some(self.arith_type(&a, None, p)?),
            "SEL" if a.len() == 3 => Some(self.arith_type(&a[1..], None, p)?),
            "SQRT" => Some(Ty::Real),
            "LEN" => Some(Ty::Int),
            _ => return Err(p.err(format!("Bilinmeyen fonksiyon: {name}"))),
        })
    }
    fn call_expr(
        &mut self,
        name: &str,
        args: &[Arg],
        p: Pos,
        hint: Option<Ty>,
    ) -> Result<Value, Diagnostic> {
        self.call_type(name, args, p)?;
        if args.iter().any(|(n, _)| n.is_some()) {
            return Err(p.err(format!(
                "{name}: adlandırılmış parametre yalnız fonksiyon bloklarında kullanılır"
            )));
        }
        let a: Vec<&Expr> = args.iter().map(|(_, e)| e).collect();
        let count = |n: usize| -> Result<(), Diagnostic> {
            if a.len() == n {
                Ok(())
            } else {
                Err(p.err(format!("{name} {n} parametre alır")))
            }
        };
        if name == "LEN" {
            count(1)?;
            let (ExprKind::Name(n), Some(_)) = (&a[0].kind, a.first()) else {
                return Err(a[0].pos.err("LEN bir STRING değişkeni ister"));
            };
            let (_, base) = self
                .string_info(n)
                .ok_or_else(|| a[0].pos.err(format!("{n} bir STRING değil")))?;
            return Ok(Value::num(Ty::Int, format!("D{base}")));
        }
        if let Some((from, to)) = conversion(name) {
            count(1)?;
            let v = self.eval(a[0], Some(from))?;
            let v = self.coerce_for_conversion(v, from, a[0].pos)?;
            return self.convert(v, to, p);
        }
        match name {
            "SQRT" => {
                count(1)?;
                let v = self.eval(a[0], Some(Ty::Real))?;
                if v.ty != Ty::Real {
                    return Err(p.err("SQRT REAL parametre alır; INT için INT_TO_REAL kullanın"));
                }
                let src = self.materialize(v, p)?;
                let out = self.temp(Ty::Real, p)?;
                self.action(p, format!("DESQR {} {}", src.operand(), out.operand()));
                self.real_fault_exit(p, &src, None, false)?;
                Ok(out)
            }
            "ABS" => {
                count(1)?;
                let ty = self.arith_type(&a, hint, p)?;
                let v = self.eval(a[0], Some(ty))?;
                let v = self.coerce(v, ty, p)?;
                let out = self.temp(ty, p)?;
                self.action(
                    p,
                    format!("{} {} {}", move_cmd(ty, &v), v.operand(), out.operand()),
                );
                match ty {
                    Ty::Int => {
                        self.line(p, format!("LD< {} K0", out.operand()));
                        self.line(p, format!("NEG {}", out.operand()));
                    }
                    Ty::Dint => {
                        self.line(p, format!("DLD< {} K0", out.operand()));
                        self.line(p, format!("DNEG {}", out.operand()));
                    }
                    Ty::Real => {
                        self.action(p, format!("DAND {0} H7FFFFFFF {0}", out.operand()));
                    }
                    _ => return Err(p.err("ABS INT, DINT veya REAL gerektirir")),
                }
                Ok(out)
            }
            "MIN" | "MAX" | "LIMIT" | "SEL" => {
                match name {
                    "LIMIT" | "SEL" => count(3)?,
                    _ if a.len() < 2 => return Err(p.err(format!("{name} en az 2 parametre alır"))),
                    _ => {}
                }
                let list: &[&Expr] = if name == "SEL" { &a[1..] } else { &a };
                let ty = self.arith_type(list, hint, p)?;
                if !matches!(ty, Ty::Int | Ty::Dint | Ty::Real) {
                    return Err(p.err(format!("{name} INT, DINT veya REAL gerektirir")));
                }
                let mut values = Vec::new();
                for e in list {
                    let v = self.eval(e, Some(ty))?;
                    values.push(self.coerce(v, ty, e.pos)?);
                }
                let out = self.temp(ty, p)?;
                match name {
                    "SEL" => {
                        let g = self.bool_param(a[0])?;
                        self.store(p, &out, values[0].clone())?;
                        self.bool_contact(p, &g, "LD");
                        let cmd = move_cmd(ty, &values[1]);
                        self.line(
                            p,
                            format!("{cmd} {} {}", values[1].operand(), out.operand()),
                        );
                    }
                    "MIN" | "MAX" => {
                        self.store(p, &out, values[0].clone())?;
                        for v in &values[1..] {
                            self.select_into(p, &out, v, if name == "MIN" { ">" } else { "<" })?;
                        }
                    }
                    _ => {
                        // LIMIT(MN, IN, MX): clamp IN into [MN, MX].
                        self.store(p, &out, values[1].clone())?;
                        self.select_into(p, &out, &values[0], "<")?;
                        self.select_into(p, &out, &values[2], ">")?;
                    }
                }
                Ok(out)
            }
            _ => unreachable!("call_type rejected {name}"),
        }
    }
    /// `out := v` when `out <op> v` holds.
    fn select_into(&mut self, p: Pos, out: &Value, v: &Value, op: &str) -> Result<(), Diagnostic> {
        let cond = self.compare(p, op, out.clone(), v.clone())?;
        self.bool_contact(p, &cond, "LD");
        self.line(
            p,
            format!("{} {} {}", move_cmd(out.ty, v), v.operand(), out.operand()),
        );
        Ok(())
    }
    fn coerce_for_conversion(&mut self, v: Value, from: Ty, p: Pos) -> Result<Value, Diagnostic> {
        if v.ty == from {
            return Ok(v);
        }
        // A wider-typed argument is not silently narrowed: name the real source type.
        Err(p.err(format!(
            "{from}_TO_... bir {from} parametre bekler, {} verildi",
            v.ty
        )))
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
            StmtKind::Assign(name, e) if self.strings.contains_key(name) => {
                self.assign_string(name, e, p)?;
            }
            StmtKind::Assign(name, e) => {
                let dest = self.writable(name, p)?;
                let v = self.eval(e, Some(dest.ty))?;
                self.store(p, &dest, v)?;
            }
            StmtKind::Call(name, args) => self.stmt_call(name, args, p)?,
            StmtKind::Shared(instance, block) => self.shared_call(instance, block, p)?,
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
    fn string_info(&self, name: &str) -> Option<(usize, i64)> {
        let cap = *self.strings.get(name)?;
        let symbol = &self.symbols[*self.names.get(name)?];
        Some((cap, symbol.address.get(1..)?.parse().ok()?))
    }
    fn is_string(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Str(_) => true,
            ExprKind::Name(n) => self.strings.contains_key(n),
            _ => false,
        }
    }
    /// `s := 'text'` or `s := other`; unused characters are cleared so equality can compare words.
    fn assign_string(&mut self, name: &str, e: &Expr, p: Pos) -> Result<(), Diagnostic> {
        let (cap, base) = self
            .string_info(name)
            .ok_or_else(|| p.err(format!("{name} bir STRING değil")))?;
        let words = 1 + cap.div_ceil(2);
        match &e.kind {
            ExprKind::Str(text) => {
                for (i, v) in string_words(text, cap, e.pos)?.into_iter().enumerate() {
                    self.action(p, format!("MOV K{v} D{}", base + i as i64));
                }
            }
            ExprKind::Name(src) if self.strings.contains_key(src) => {
                let (src_cap, src_base) = self.string_info(src).expect("string");
                if src_cap > cap {
                    return Err(e.pos.err(format!(
                        "{src} (kapasite {src_cap}) {name} değişkenine (kapasite {cap}) sığmaz"
                    )));
                }
                if src != name {
                    let copied = 1 + src_cap.div_ceil(2);
                    self.action(p, format!("BMOV D{src_base} D{base} K{copied}"));
                    for i in copied..words {
                        self.action(p, format!("MOV K0 D{}", base + i as i64));
                    }
                }
            }
            _ => {
                return Err(e
                    .pos
                    .err("STRING'e yalnız 'metin' veya başka bir STRING atanabilir"));
            }
        }
        Ok(())
    }
    /// `s = t`, `s <> 'abc'`: every word (length + packed characters) must match.
    fn string_compare<'a>(
        &mut self,
        op: &str,
        a: &'a Expr,
        b: &'a Expr,
        p: Pos,
    ) -> Result<Value, Diagnostic> {
        enum Side<'a> {
            Var(usize, i64),
            Text(&'a str),
        }
        let side = |this: &Self, e: &'a Expr| -> Result<Side<'a>, Diagnostic> {
            match &e.kind {
                ExprKind::Str(text) => Ok(Side::Text(text)),
                ExprKind::Name(n) if this.strings.contains_key(n) => {
                    let (cap, base) = this.string_info(n).expect("string");
                    Ok(Side::Var(cap, base))
                }
                _ => Err(e
                    .pos
                    .err("STRING yalnız başka bir STRING veya metinle karşılaştırılır")),
            }
        };
        let (l, r) = (side(self, a)?, side(self, b)?);
        let words = match (&l, &r) {
            (Side::Var(x, _), Side::Var(y, _)) => 1 + x.max(y).div_ceil(2),
            (Side::Var(c, _), Side::Text(_)) | (Side::Text(_), Side::Var(c, _)) => {
                1 + c.div_ceil(2)
            }
            _ => return Err(p.err("İki metin sabiti karşılaştırılamaz")),
        };
        let operands = |side: &Side, pos: Pos| -> Result<Vec<String>, Diagnostic> {
            Ok(match side {
                Side::Var(cap, base) => (0..words)
                    .map(|w| {
                        if w < 1 + cap.div_ceil(2) {
                            format!("D{}", base + w as i64)
                        } else {
                            "K0".to_owned()
                        }
                    })
                    .collect(),
                Side::Text(text) => string_words(text, (words - 1) * 2, pos)?
                    .into_iter()
                    .map(|v| format!("K{v}"))
                    .collect(),
            })
        };
        let (lo, ro) = (operands(&l, a.pos)?, operands(&r, b.pos)?);
        let out = self.temp(Ty::Bool, p)?;
        let Repr::Bit(d, _) = &out.repr else {
            unreachable!()
        };
        for (w, (x, y)) in lo.iter().zip(&ro).enumerate() {
            self.line(p, format!("{}= {x} {y}", if w == 0 { "LD" } else { "AND" }));
        }
        self.line(p, format!("OUT {d}"));
        Ok(if op == "<>" { out.inverted() } else { out })
    }
    fn init_array(&mut self, name: &str, items: &[Expr], ty: Ty, p: Pos) -> Result<(), Diagnostic> {
        let (lo, hi) = self.array_symbol(name, p)?.array.expect("array");
        let count = (hi - lo + 1) as usize;
        if items.len() > count {
            return Err(p.err(format!(
                "Başlangıç listesinde {} değer var; dizi {count} eleman tutar",
                items.len()
            )));
        }
        for (k, item) in items.iter().enumerate() {
            let index = Expr {
                kind: ExprKind::Number((lo + k as i32).to_string()),
                pos: item.pos,
            };
            let dest = self.element(name, &index, p, false)?;
            let v = self.eval(item, Some(ty))?;
            self.store(p, &dest, v)?;
        }
        Ok(())
    }
    fn declarations(&mut self, decls: &[Decl]) -> Result<(), Diagnostic> {
        // Explicitly pinned timers/counters first, so automatic ones never take their numbers.
        for d in decls {
            if let (Some(kind), Some((raw, mp))) = (d.fb, &d.mapping) {
                let timer = kind.is_timer();
                let n = fb_device(raw, timer, *mp)?;
                let set = if timer {
                    &mut self.pool.timers
                } else {
                    &mut self.pool.counters
                };
                if !set.insert(n) {
                    return Err(mp.err(format!(
                        "{raw} başka bir örnek veya program tarafından kullanılıyor"
                    )));
                }
            }
        }
        for d in decls {
            if let Some(kind) = d.fb {
                if d.init.is_some() || d.retain {
                    return Err(d.names[0]
                        .1
                        .err("Fonksiyon bloğu başlangıç değeri veya VAR_RETAIN almaz"));
                }
                for (name, p) in &d.names {
                    if self.preplaced.contains(name) {
                        continue;
                    }
                    if reserved(name) || device(name, *p)?.is_some() {
                        return Err(
                            p.err("Değişken adı anahtar sözcük veya doğrudan PLC adresi olamaz")
                        );
                    }
                    if self.names.contains_key(name) || self.fbs.contains_key(name) {
                        return Err(p.err(format!("Yinelenen değişken: {name}")));
                    }
                    self.add_fb(name, kind, d.mapping.as_ref(), *p)?;
                }
                continue;
            }
            for (name, p) in &d.names {
                if self.preplaced.contains(name) {
                    continue;
                }
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
                if let Some(init) = &d.init {
                    let list = matches!(init.kind, ExprKind::List(_));
                    if d.string.is_none() && array.is_some() != list {
                        return Err(p.err(
                            "Dizi başlangıç değeri [a, b, 3(0)] listesi olmalı; liste yalnız dizilerde kullanılır",
                        ));
                    }
                    if d.string.is_some() && !matches!(init.kind, ExprKind::Str(_)) {
                        return Err(p.err("STRING başlangıç değeri 'metin' olmalı"));
                    }
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
                if let Some(cap) = d.string {
                    self.strings.insert(name.clone(), cap);
                }
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
                        if d.string.is_some() {
                            self.assign_string(name, init, *p)?;
                            self.pool.release(mark);
                            continue;
                        }
                        if let ExprKind::List(items) = &init.kind {
                            self.init_array(name, items, d.ty, *p)?;
                            self.pool.release(mark);
                            continue;
                        }
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

impl Emitter {
    /// Place every instance of a shared block: its words and bits in two contiguous runs with
    /// the layout's offsets, its timers / counters on their own devices.
    fn place_shared(&mut self, block: &SharedBlock, decls: &[Decl]) -> Result<(), Diagnostic> {
        let layout = SharedLayout::new(&block.members, &block.template);
        for instance in &block.instances {
            let prefix = format!("{instance}.");
            let p = decls
                .iter()
                .flat_map(|d| d.names.iter())
                .find(|(n, _)| n.starts_with(&prefix))
                .map_or(Pos { line: 1, col: 1 }, |(_, p)| *p);
            let mut base = |ty: Ty, width: usize, name: &str| -> Result<u16, Diagnostic> {
                if width == 0 {
                    return Ok(0);
                }
                let at = self.pool.allocate(ty, width, false, p)?;
                // One symbol per run, so other programs of the project keep clear of it.
                self.symbols.push(StSymbol {
                    name: format!("__{instance}_{name}"),
                    data_type: ty,
                    address: at.clone(),
                    line: p.line,
                    explicitly_mapped: false,
                    retained: false,
                    array: Some((0, width as i32 - 1)),
                });
                Ok(at[1..].parse().unwrap())
            };
            let words = base(Ty::Int, layout.words, "WORDS")?;
            let bits = base(Ty::Bool, layout.bits, "BITS")?;
            self.bases.insert(instance.clone(), (words, bits));
            for (suffix, ty, at) in &layout.plain {
                let name = format!("{instance}{suffix}");
                let address = if *ty == Ty::Bool {
                    format!("M{}", bits + *at as u16)
                } else {
                    format!("D{}", words + *at as u16)
                };
                self.names.insert(name.clone(), self.symbols.len());
                self.symbols.push(StSymbol {
                    name: name.clone(),
                    data_type: *ty,
                    address,
                    line: p.line,
                    explicitly_mapped: false,
                    retained: false,
                    array: None,
                });
                self.preplaced.insert(name);
            }
            for f in &layout.fbs {
                let name = format!("{instance}{}", f.suffix);
                let extra = f
                    .extra
                    .iter()
                    .map(|&(bit, at)| {
                        if bit {
                            format!("M{}", bits + at as u16)
                        } else {
                            format!("D{}", words + at as u16)
                        }
                    })
                    .collect();
                self.add_fb_with(&name, f.kind, None, p, Some(extra))?;
                self.preplaced.insert(name);
            }
        }
        // The template's variables, relative to the bases in E6 / E7.
        for (suffix, ty, at) in &layout.plain {
            let v = if *ty == Ty::Bool {
                Value::bit(format!("M{at}E7"))
            } else {
                Value::num(*ty, format!("D{at}E6"))
            };
            self.indexed.insert(format!("{}{suffix}", block.template), v);
        }
        for f in &layout.fbs {
            let name = format!("{}{}", block.template, f.suffix);
            let extra = f
                .extra
                .iter()
                .map(|&(bit, at)| if bit { format!("M{at}E7") } else { format!("D{at}E6") })
                .collect();
            self.fbs.insert(
                name.clone(),
                Fb {
                    kind: f.kind,
                    dev: String::new(),
                    extra,
                },
            );
            self.template_fbs
                .insert(name, (block.template.clone(), f.clone()));
        }
        self.templates.insert(
            block.block.clone(),
            Template {
                layout,
                segments: Vec::new(),
                holes: Vec::new(),
                abort: String::new(),
                faulting: Vec::new(),
                reads: BTreeSet::new(),
            },
        );
        Ok(())
    }

    /// Emit a shared body after `FEND`: one subroutine per segment between its timer /
    /// counter calls, whose arguments are evaluated at the end of the segment before.
    fn compile_template(&mut self, block: &SharedBlock) -> Result<(), Diagnostic> {
        let p = block.body.first().map_or(Pos { line: 1, col: 1 }, |s| s.pos);
        let abort = self.pool.allocate(Ty::Bool, 1, false, p)?;
        self.symbols.push(StSymbol {
            name: format!("__{}_ABORT", block.block),
            data_type: Ty::Bool,
            address: abort.clone(),
            line: p.line,
            explicitly_mapped: false,
            retained: false,
            array: None,
        });
        let main = (
            std::mem::take(&mut self.lines),
            std::mem::take(&mut self.line_map),
        );
        self.lines = std::mem::take(&mut self.sub_lines);
        self.line_map = std::mem::take(&mut self.sub_map);
        self.action_tail = usize::MAX;
        self.reads.clear();
        let mut segments = Vec::new();
        let mut holes = Vec::new();
        let mut faulting = Vec::new();
        let mut start = self.open_segment(p, &abort)?;
        for s in &block.body {
            let device_call = match &s.kind {
                StmtKind::Call(name, args) => self
                    .template_fbs
                    .get(name)
                    .filter(|(_, l)| l.kind.needs_device())
                    .map(|(_, l)| (name.clone(), args.clone(), l.suffix.clone())),
                _ => None,
            };
            let Some((name, args, suffix)) = device_call else {
                self.stmt(s)?;
                continue;
            };
            let mark = self.pool.temps.len();
            let (_, mut a) = self.fb_args(&name, &args, s.pos)?;
            // TMR / CNT presets cannot be index-addressed.
            if indexed(&a.preset) {
                let w = self.temp(Ty::Int, s.pos)?;
                self.action(s.pos, format!("MOV {} {}", a.preset, w.operand()));
                a.preset = w.operand().to_owned();
            }
            faulting.push(self.close_segment(s.pos, start, &abort)?);
            segments.push(start.0);
            holes.push((suffix, a));
            // The arguments are read by the instance's own TMR / CNT before the next CALL.
            self.pool.release(mark);
            start = self.open_segment(s.pos, &abort)?;
        }
        let end = block.body.last().map_or(p, |s| s.pos);
        faulting.push(self.close_segment(end, start, &abort)?);
        segments.push(start.0);
        self.sub_lines = std::mem::replace(&mut self.lines, main.0);
        self.sub_map = std::mem::replace(&mut self.line_map, main.1);
        self.action_tail = usize::MAX;
        let t = self.templates.get_mut(&block.block).unwrap();
        t.segments = segments;
        t.holes = holes;
        t.abort = abort;
        t.faulting = faulting;
        t.reads = std::mem::take(&mut self.reads);
        Ok(())
    }
    fn open_segment(&mut self, p: Pos, abort: &str) -> Result<(u8, usize), Diagnostic> {
        let label = self.label(p)?;
        self.mark(p, label);
        self.fault = Some(FaultExit {
            abort: abort.to_owned(),
            end: None,
        });
        Ok((label, self.lines.len()))
    }
    /// End a segment with `SRET`; returns whether it has fault exits.
    fn close_segment(&mut self, p: Pos, start: (u8, usize), abort: &str) -> Result<bool, Diagnostic> {
        let fault = self.fault.take().expect("open segment");
        if let Some(end) = fault.end {
            self.mark(p, end);
            // The abort bit is cleared on entry and set by a fault exit.
            let at = start.1;
            self.lines
                .splice(at..at, ["LD M1000".to_owned(), format!("RST {abort}")]);
            let line = self.line_map[at - 1];
            self.line_map.splice(at..at, [line; 2]);
            self.action_tail = usize::MAX;
        }
        self.line(p, "SRET");
        Ok(fault.end.is_some())
    }

    /// One call of a shared instance: point E6 / E7 at its variables, then run the segments
    /// with the instance's own timer / counter instructions between them.
    fn shared_call(&mut self, instance: &str, block: &str, p: Pos) -> Result<(), Diagnostic> {
        let t = &self.templates[block];
        let (segments, holes, abort, faulting, reads) = (
            t.segments.clone(),
            t.holes.clone(),
            t.abort.clone(),
            t.faulting.clone(),
            t.reads.clone(),
        );
        let layout = t.layout.clone();
        let (words, bits) = self.bases[instance];
        self.action(p, format!("MOV K{words} E6"));
        self.action(p, format!("MOV K{bits} E7"));
        for f in &layout.fbs {
            self.copy_device(instance, f, &reads, words, bits, p);
        }
        for (k, segment) in segments.iter().enumerate() {
            self.action(p, format!("CALL P{segment}"));
            if faulting[k] {
                self.line(p, format!("LD {abort}"));
                self.line(p, "CJ P0");
            }
            if let Some((suffix, a)) = holes.get(k) {
                let fb = self.fbs[&format!("{instance}{suffix}")].clone();
                self.fb_emit(p, &fb, a)?;
                if let Some(f) = layout.fbs.iter().find(|f| &f.suffix == suffix) {
                    self.copy_device(instance, f, &reads, words, bits, p);
                }
            }
        }
        Ok(())
    }
    /// Copy a timer / counter's contact and value into the instance's words the shared body
    /// reads (only those it reads).
    fn copy_device(
        &mut self,
        instance: &str,
        f: &FbLayout,
        reads: &BTreeSet<(String, bool)>,
        words: u16,
        bits: u16,
        p: Pos,
    ) {
        if !f.kind.needs_device() {
            return;
        }
        let dev = self.fbs[&format!("{instance}{}", f.suffix)].dev.clone();
        if let Some(q) = f.q
            && reads.contains(&(f.suffix.clone(), true))
        {
            self.line(p, format!("LD {dev}"));
            self.line(p, format!("OUT M{}", bits + q as u16));
        }
        if let Some((at, _)) = f.value
            && reads.contains(&(f.suffix.clone(), false))
        {
            let to = format!("D{}", words + at as u16);
            if matches!(f.kind, FbKind::Ton | FbKind::Tof | FbKind::Tp) {
                self.action(p, format!("MUL {dev} K100 {to}"));
            } else {
                self.action(p, format!("MOV {dev} {to}"));
            }
        }
    }
}

pub(super) fn lower(unit: Unit, options: &super::LowerOptions) -> Result<LoweredSt, Diagnostic> {
    let mut e = Emitter {
        fbs: BTreeMap::new(),
        strings: BTreeMap::new(),
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
        int_scratch: None,
        action_tail: usize::MAX,
        action_run: 0,
        templates: BTreeMap::new(),
        bases: BTreeMap::new(),
        indexed: BTreeMap::new(),
        template_fbs: BTreeMap::new(),
        preplaced: BTreeSet::new(),
        sub_lines: Vec::new(),
        sub_map: Vec::new(),
        fault: None,
        reads: BTreeSet::new(),
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
    for block in &unit.shared {
        e.place_shared(block, &unit.decls)?;
    }
    e.declarations(&unit.decls)?;
    for block in &unit.shared {
        e.compile_template(block)?;
    }
    e.block(&unit.body)?;
    let p = unit
        .body
        .last()
        .map(|s| s.pos)
        .unwrap_or(Pos { line: 1, col: 1 });
    e.mark(p, e.exit_label);
    if let Some((a, b)) = e.int_scratch.clone() {
        let first = e.line_map.first().copied().unwrap_or(1);
        let prologue = [
            "LD M1000".to_owned(),
            format!("MOV K0 {}", offset(&a, 1)),
            format!("MOV K0 {}", offset(&b, 1)),
        ];
        e.lines.splice(0..0, prologue);
        e.line_map.splice(0..0, [first; 3]);
    }
    let main_lines = e.lines.len();
    if !e.sub_lines.is_empty() {
        e.line(p, "FEND");
        let (lines, map) = (std::mem::take(&mut e.sub_lines), std::mem::take(&mut e.sub_map));
        e.lines.extend(lines);
        e.line_map.extend(map);
    }
    e.line(p, "END");
    let temporaries = e
        .pool
        .used_temps
        .iter()
        .map(|&(bit, index)| Device {
            area: if bit { Area::M } else { Area::D },
            index,
        })
        .collect();
    Ok(LoweredSt {
        shared_blocks: unit.shared.iter().map(|b| b.block.clone()).collect(),
        main_lines,
        temporaries,
        il: e.lines.join("\n") + "\n",
        line_map: e.line_map,
        symbols: e.symbols,
        next_label: e.labels,
    })
}
