//! User `FUNCTION_BLOCK` / `FUNCTION` support by inlining.
//!
//! An instance `m : Motor;` is a struct whose fields are the block's variables
//! (`m.start`, `m.run`, ...). A call `m(start := x);` becomes assignments to the
//! inputs followed by the block body with every block variable renamed to
//! `m.<name>`. A function call inside an expression gets its own hidden
//! instance (`__fn_NAME_N`); its statements are hoisted in front of the
//! statement that contains the call.
//!
//! When several top-level instances of one block type exist (see [`super::Share`]), their
//! calls are not expanded: the inputs are assigned and a `Shared` statement runs one common
//! body, written once for a template instance `__S_<BLOCK>` (see `emit.rs`).
use super::Share;
use super::syntax::{Arg, Decl, Expr, ExprKind, FbDef, Pos, SharedBlock, Stmt, StmtKind};
use crate::plc::Diagnostic;
use std::collections::{BTreeMap, BTreeSet};

type Types = BTreeMap<String, Vec<Decl>>;
type Defs = BTreeMap<String, FbDef>;
/// Declarations, body and shared block bodies of a program after expansion.
type Expanded = (Vec<Decl>, Vec<Stmt>, Vec<SharedBlock>);

const MAX_DEPTH: usize = 8;
const MAX_STATEMENTS: usize = 20_000;

struct Cx<'a> {
    types: &'a Types,
    defs: &'a Defs,
    /// Full instance name -> block type, for every user function-block instance.
    instances: BTreeMap<String, String>,
    hidden: Vec<Decl>,
    counter: usize,
    emitted: usize,
    /// Top-level instance -> block type, for instances whose calls run a shared body.
    shared: BTreeMap<String, String>,
}

pub(super) fn inline(
    mut decls: Vec<Decl>,
    body: Vec<Stmt>,
    types: &Types,
    defs: &Defs,
    share: Share,
    exclude: &BTreeSet<String>,
) -> Result<Expanded, Diagnostic> {
    if defs.is_empty() {
        return Ok((decls, body, Vec::new()));
    }
    let mut cx = Cx {
        types,
        defs,
        instances: BTreeMap::new(),
        hidden: Vec::new(),
        counter: 0,
        emitted: 0,
        shared: BTreeMap::new(),
    };
    cx.collect(&decls, 0)?;
    let shared = cx.templates(&decls, share, exclude)?;
    let body = cx.stmts(body, 0)?;
    decls.append(&mut cx.hidden);
    Ok((decls, body, shared))
}

/// Names of the timer / `CTU` instances among `decls` (nested blocks included).
fn device_blocks(decls: &[Decl], types: &Types, out: &mut BTreeSet<String>) {
    for d in decls {
        if d.fb.is_some_and(|k| k.needs_device()) {
            out.extend(d.names.iter().map(|(n, _)| n.clone()));
        }
        if let Some(t) = &d.strukt
            && let Some(fields) = types.get(t)
        {
            for (n, _) in &d.names {
                device_blocks(&prefixed(fields, n), types, out);
            }
        }
    }
}

/// Is a call of one of `names` nested inside a compound statement (or, with `top`, anywhere)?
fn calls_nested(list: &[Stmt], names: &BTreeSet<String>, top: bool) -> bool {
    list.iter().any(|s| match &s.kind {
        StmtKind::Call(n, _) => !top && names.contains(n),
        StmtKind::Shared(..) => true,
        StmtKind::If(arms, other) => {
            arms.iter().any(|(_, b)| calls_nested(b, names, false))
                || calls_nested(other, names, false)
        }
        StmtKind::Case(_, arms, other) => {
            arms.iter().any(|(_, b)| calls_nested(b, names, false))
                || calls_nested(other, names, false)
        }
        StmtKind::For(_, _, _, _, b) | StmtKind::While(_, b) | StmtKind::Repeat(b, _) => {
            calls_nested(b, names, false)
        }
        _ => false,
    })
}

/// Does the body call one of `names` anywhere?
fn calls_any(list: &[Stmt], names: &BTreeSet<String>) -> bool {
    list.iter().any(|s| match &s.kind {
        StmtKind::Call(n, _) => names.contains(n),
        StmtKind::Shared(..) => true,
        StmtKind::If(arms, other) => {
            arms.iter().any(|(_, b)| calls_any(b, names)) || calls_any(other, names)
        }
        StmtKind::Case(_, arms, other) => {
            arms.iter().any(|(_, b)| calls_any(b, names)) || calls_any(other, names)
        }
        StmtKind::For(_, _, _, _, b) | StmtKind::While(_, b) | StmtKind::Repeat(b, _) => {
            calls_any(b, names)
        }
        _ => false,
    })
}

fn prefixed(decls: &[Decl], prefix: &str) -> Vec<Decl> {
    decls
        .iter()
        .map(|d| {
            let mut d = d.clone();
            d.names = d
                .names
                .iter()
                .map(|(n, p)| (format!("{prefix}.{n}"), *p))
                .collect();
            d
        })
        .collect()
}

fn first_segment(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

impl Cx<'_> {
    /// Can every instance of these variables sit at the same offsets from a word base and a
    /// bit base? Arrays (they need E0 as well), strings, retentive and mapped variables cannot.
    fn plain_layout(&self, decls: &[Decl], depth: usize) -> bool {
        depth <= MAX_DEPTH
            && decls.iter().all(|d| {
                d.array.is_none()
                    && d.string.is_none()
                    && !d.retain
                    && d.mapping.is_none()
                    && d.strukt.as_ref().is_none_or(|t| {
                        self.types
                            .get(t)
                            .is_some_and(|fields| self.plain_layout(fields, depth + 1))
                    })
            })
    }

    /// Chooses the block types whose top-level instances share one body and expands that
    /// body once for the template instance.
    fn templates(
        &mut self,
        decls: &[Decl],
        share: Share,
        exclude: &BTreeSet<String>,
    ) -> Result<Vec<SharedBlock>, Diagnostic> {
        if share == Share::Never {
            return Ok(Vec::new());
        }
        let mut by_type: BTreeMap<String, Vec<(String, Pos)>> = BTreeMap::new();
        for d in decls {
            let Some(t) = &d.strukt else { continue };
            if self.defs.get(t).is_some_and(|f| f.returns.is_none()) {
                by_type
                    .entry(t.clone())
                    .or_default()
                    .extend(d.names.iter().cloned());
            }
        }
        let top: BTreeSet<String> = by_type.values().flatten().map(|(n, _)| n.clone()).collect();
        let wanted = if share == Share::Always { 1 } else { 2 };
        let mut out = Vec::new();
        for (block, instances) in by_type {
            let def = &self.defs[&block];
            if instances.len() < wanted
                || exclude.contains(&block)
                || !self.plain_layout(&def.decls, 0)
            {
                continue;
            }
            let template = format!("__S_{block}");
            let members = prefixed(&def.decls, &template);
            let (hidden, emitted, counter) = (self.hidden.len(), self.emitted, self.counter);
            self.collect(&members, 1)?;
            let names: BTreeSet<String> = def
                .decls
                .iter()
                .flat_map(|d| d.names.iter().map(|(n, _)| n.clone()))
                .collect();
            let library = def.library;
            let pos = instances[0].1;
            let mut body = Vec::new();
            for s in def.body.clone() {
                let mut s = rename_stmt(s, &template, &names)?;
                if library {
                    relocate_stmt(&mut s, pos);
                }
                self.stmt(s, 1, &mut body)?;
            }
            let mut devices = BTreeSet::new();
            device_blocks(&members, self.types, &mut devices);
            // Each timer / CTU call splits the body, so it must run unconditionally; a call of
            // a top-level instance from inside the body would reuse the index registers.
            if calls_nested(&body, &devices, true) || calls_any(&body, &top) {
                self.hidden.truncate(hidden);
                self.emitted = emitted;
                self.counter = counter;
                continue;
            }
            for (n, _) in &instances {
                self.shared.insert(n.clone(), block.clone());
            }
            out.push(SharedBlock {
                block,
                template,
                members,
                body,
                instances: instances.into_iter().map(|(n, _)| n).collect(),
            });
        }
        Ok(out)
    }

    /// Records every user block instance found in `decls` (names already fully qualified).
    fn collect(&mut self, decls: &[Decl], depth: usize) -> Result<(), Diagnostic> {
        for d in decls {
            let Some(t) = &d.strukt else { continue };
            if depth > MAX_DEPTH {
                return Err(d.names[0].1.err("Bloklar en çok 8 düzey iç içe olabilir"));
            }
            for (name, _) in &d.names {
                if self.defs.get(t).is_some_and(|f| f.returns.is_none()) {
                    self.instances.insert(name.clone(), t.clone());
                }
                let inner = prefixed(&self.types[t], name);
                self.collect(&inner, depth + 1)?;
            }
        }
        Ok(())
    }

    fn stmts(&mut self, list: Vec<Stmt>, depth: usize) -> Result<Vec<Stmt>, Diagnostic> {
        let mut out = Vec::new();
        for s in list {
            self.stmt(s, depth, &mut out)?;
        }
        Ok(out)
    }

    fn push(&mut self, out: &mut Vec<Stmt>, s: Stmt) -> Result<(), Diagnostic> {
        self.emitted += 1;
        if self.emitted > MAX_STATEMENTS {
            return Err(s.pos.err("Blok açılımı çok büyük (en çok 20000 deyim)"));
        }
        out.push(s);
        Ok(())
    }

    fn stmt(&mut self, s: Stmt, depth: usize, out: &mut Vec<Stmt>) -> Result<(), Diagnostic> {
        let pos = s.pos;
        let kind = match s.kind {
            StmtKind::Assign(n, e) => {
                let e = self.hoist(e, depth, out)?;
                StmtKind::Assign(n, e)
            }
            StmtKind::AssignIndex(n, i, e) => {
                let i = self.hoist(i, depth, out)?;
                let e = self.hoist(e, depth, out)?;
                StmtKind::AssignIndex(n, i, e)
            }
            StmtKind::If(arms, other) => {
                let mut new = Vec::new();
                for (k, (c, body)) in arms.into_iter().enumerate() {
                    let c = if k == 0 {
                        self.hoist(c, depth, out)?
                    } else {
                        self.no_calls(c)?
                    };
                    new.push((c, self.stmts(body, depth)?));
                }
                StmtKind::If(new, self.stmts(other, depth)?)
            }
            StmtKind::Case(v, arms, other) => {
                let v = self.hoist(v, depth, out)?;
                let mut new = Vec::new();
                for (labels, body) in arms {
                    new.push((labels, self.stmts(body, depth)?));
                }
                StmtKind::Case(v, new, self.stmts(other, depth)?)
            }
            StmtKind::For(n, a, b, c, body) => {
                let a = self.hoist(a, depth, out)?;
                let b = self.hoist(b, depth, out)?;
                let c = match c {
                    Some(c) => Some(self.hoist(c, depth, out)?),
                    None => None,
                };
                StmtKind::For(n, a, b, c, self.stmts(body, depth)?)
            }
            StmtKind::While(c, body) => {
                StmtKind::While(self.no_calls(c)?, self.stmts(body, depth)?)
            }
            StmtKind::Repeat(body, c) => {
                StmtKind::Repeat(self.stmts(body, depth)?, self.no_calls(c)?)
            }
            StmtKind::Call(name, args) => {
                let mut hoisted = Vec::new();
                for (n, e) in args {
                    hoisted.push((n, self.hoist(e, depth, out)?));
                }
                if let Some(t) = self.shared.get(&name).cloned() {
                    self.assign_inputs(&name, &t, hoisted, pos, out)?;
                    return self.push(
                        out,
                        Stmt {
                            kind: StmtKind::Shared(name, t),
                            pos,
                        },
                    );
                }
                if let Some(t) = self.instances.get(&name).cloned() {
                    return self.call_block(&name, &t, hoisted, pos, depth, out);
                }
                StmtKind::Call(name, hoisted)
            }
            k @ (StmtKind::Exit | StmtKind::Return | StmtKind::Shared(..)) => k,
        };
        self.push(out, Stmt { kind, pos })
    }

    /// Assigns the arguments to the inputs of `inst`, then appends the renamed block body.
    fn call_block(
        &mut self,
        inst: &str,
        block: &str,
        args: Vec<Arg>,
        pos: Pos,
        depth: usize,
        out: &mut Vec<Stmt>,
    ) -> Result<(), Diagnostic> {
        if depth >= MAX_DEPTH {
            return Err(pos.err(format!(
                "{block}: blok çağrıları en çok 8 düzey iç içe olabilir (özyineleme yok)"
            )));
        }
        self.assign_inputs(inst, block, args, pos, out)?;
        let def = &self.defs[block];
        let members: BTreeSet<String> = def
            .decls
            .iter()
            .flat_map(|d| d.names.iter().map(|(n, _)| n.clone()))
            .collect();
        let body = def.body.clone();
        let library = def.library;
        let mut renamed = Vec::new();
        for s in body {
            let mut s = rename_stmt(s, inst, &members)?;
            if library {
                relocate_stmt(&mut s, pos);
            }
            renamed.push(s);
        }
        for s in renamed {
            self.stmt(s, depth + 1, out)?;
        }
        Ok(())
    }

    /// `inst.<input> := <argument>` for every argument of a call.
    fn assign_inputs(
        &mut self,
        inst: &str,
        block: &str,
        args: Vec<Arg>,
        pos: Pos,
        out: &mut Vec<Stmt>,
    ) -> Result<(), Diagnostic> {
        let def = &self.defs[block];
        let mut positional = def.inputs.iter();
        for (name, value) in args {
            let param = match name {
                Some(n) => {
                    if !def.inputs.contains(&n) {
                        return Err(value
                            .pos
                            .err(format!("{n}, {block} bloğunun VAR_INPUT parametresi değil")));
                    }
                    n
                }
                None => positional
                    .next()
                    .ok_or_else(|| value.pos.err(format!("{block} için fazla argüman")))?
                    .clone(),
            };
            self.push(
                out,
                Stmt {
                    kind: StmtKind::Assign(format!("{inst}.{param}"), value),
                    pos,
                },
            )?;
        }
        Ok(())
    }

    /// Replaces function calls in `e` by the name of their result variable,
    /// pushing the statements that compute them onto `out`.
    fn hoist(&mut self, e: Expr, depth: usize, out: &mut Vec<Stmt>) -> Result<Expr, Diagnostic> {
        let pos = e.pos;
        let kind = match e.kind {
            ExprKind::Call(name, args)
                if self.defs.get(&name).is_some_and(|f| f.returns.is_some()) =>
            {
                let mut hoisted = Vec::new();
                for (n, a) in args {
                    hoisted.push((n, self.hoist(a, depth, out)?));
                }
                self.counter += 1;
                let inst = format!("__fn_{name}_{}", self.counter);
                let hidden = prefixed(&self.defs[&name].decls, &inst);
                self.collect(&hidden, 0)?;
                self.hidden.extend(hidden);
                self.call_block(&inst, &name, hoisted, pos, depth, out)?;
                ExprKind::Name(format!("{inst}.{name}"))
            }
            ExprKind::Call(name, args) => {
                let mut hoisted = Vec::new();
                for (n, a) in args {
                    hoisted.push((n, self.hoist(a, depth, out)?));
                }
                ExprKind::Call(name, hoisted)
            }
            ExprKind::Unary(op, a) => ExprKind::Unary(op, Box::new(self.hoist(*a, depth, out)?)),
            ExprKind::Binary(op, a, b) => {
                let a = self.hoist(*a, depth, out)?;
                let b = self.hoist(*b, depth, out)?;
                ExprKind::Binary(op, Box::new(a), Box::new(b))
            }
            ExprKind::Index(n, i) => ExprKind::Index(n, Box::new(self.hoist(*i, depth, out)?)),
            k => k,
        };
        Ok(Expr { kind, pos })
    }

    /// Conditions that are re-evaluated (WHILE, REPEAT, ELSIF) cannot host a function call.
    fn no_calls(&self, e: Expr) -> Result<Expr, Diagnostic> {
        fn find(defs: &Defs, e: &Expr) -> Option<Pos> {
            match &e.kind {
                ExprKind::Call(n, args) => {
                    if defs.get(n).is_some_and(|f| f.returns.is_some()) {
                        Some(e.pos)
                    } else {
                        args.iter().find_map(|(_, a)| find(defs, a))
                    }
                }
                ExprKind::Unary(_, a) | ExprKind::Index(_, a) => find(defs, a),
                ExprKind::Binary(_, a, b) => find(defs, a).or_else(|| find(defs, b)),
                _ => None,
            }
        }
        match find(self.defs, &e) {
            Some(p) => Err(p.err(
                "FUNCTION çağrısı WHILE / REPEAT / ELSIF koşulunda kullanılamaz; sonucu önce bir değişkene atayın",
            )),
            None => Ok(e),
        }
    }
}

/// Library code has no lines in the user's source: every position becomes the call site,
/// so diagnostics and the IL line map point at the line that called the block.
fn relocate_expr(e: &mut Expr, p: Pos) {
    e.pos = p;
    match &mut e.kind {
        ExprKind::Index(_, a) | ExprKind::Unary(_, a) => relocate_expr(a, p),
        ExprKind::Binary(_, a, b) => {
            relocate_expr(a, p);
            relocate_expr(b, p);
        }
        ExprKind::List(v) => v.iter_mut().for_each(|x| relocate_expr(x, p)),
        ExprKind::Call(_, args) => args.iter_mut().for_each(|(_, x)| relocate_expr(x, p)),
        ExprKind::Number(_)
        | ExprKind::Bool(_)
        | ExprKind::Name(_)
        | ExprKind::Member(..)
        | ExprKind::Str(_) => {}
    }
}

fn relocate_stmt(s: &mut Stmt, p: Pos) {
    s.pos = p;
    let block = |b: &mut Vec<Stmt>| b.iter_mut().for_each(|s| relocate_stmt(s, p));
    match &mut s.kind {
        StmtKind::Assign(_, e) => relocate_expr(e, p),
        StmtKind::AssignIndex(_, i, e) => {
            relocate_expr(i, p);
            relocate_expr(e, p);
        }
        StmtKind::If(arms, other) => {
            for (c, b) in arms {
                relocate_expr(c, p);
                block(b);
            }
            block(other);
        }
        StmtKind::Case(v, arms, other) => {
            relocate_expr(v, p);
            for (labels, b) in arms {
                for (a, z) in labels {
                    relocate_expr(a, p);
                    if let Some(z) = z {
                        relocate_expr(z, p);
                    }
                }
                block(b);
            }
            block(other);
        }
        StmtKind::For(_, a, b, c, body) => {
            relocate_expr(a, p);
            relocate_expr(b, p);
            if let Some(c) = c {
                relocate_expr(c, p);
            }
            block(body);
        }
        StmtKind::While(c, body) | StmtKind::Repeat(body, c) => {
            relocate_expr(c, p);
            block(body);
        }
        StmtKind::Call(_, args) => args.iter_mut().for_each(|(_, x)| relocate_expr(x, p)),
        StmtKind::Exit | StmtKind::Return | StmtKind::Shared(..) => {}
    }
}

fn rename(name: &str, inst: &str, members: &BTreeSet<String>) -> String {
    if members.contains(first_segment(name)) {
        format!("{inst}.{name}")
    } else {
        name.to_string()
    }
}

fn rename_expr(e: Expr, inst: &str, m: &BTreeSet<String>) -> Expr {
    let pos = e.pos;
    let r = |x: Expr| rename_expr(x, inst, m);
    let kind = match e.kind {
        ExprKind::Name(n) => ExprKind::Name(rename(&n, inst, m)),
        ExprKind::Index(n, i) => ExprKind::Index(rename(&n, inst, m), Box::new(r(*i))),
        ExprKind::Unary(op, a) => ExprKind::Unary(op, Box::new(r(*a))),
        ExprKind::Binary(op, a, b) => ExprKind::Binary(op, Box::new(r(*a)), Box::new(r(*b))),
        ExprKind::Member(a, b) => ExprKind::Member(rename(&a, inst, m), b),
        ExprKind::List(v) => ExprKind::List(v.into_iter().map(r).collect()),
        ExprKind::Call(n, args) => ExprKind::Call(
            rename(&n, inst, m),
            args.into_iter().map(|(k, v)| (k, r(v))).collect(),
        ),
        k @ (ExprKind::Number(_) | ExprKind::Bool(_) | ExprKind::Str(_)) => k,
    };
    Expr { kind, pos }
}

fn rename_stmt(s: Stmt, inst: &str, m: &BTreeSet<String>) -> Result<Stmt, Diagnostic> {
    let pos = s.pos;
    let e = |x: Expr| rename_expr(x, inst, m);
    let block = |list: Vec<Stmt>| -> Result<Vec<Stmt>, Diagnostic> {
        list.into_iter().map(|s| rename_stmt(s, inst, m)).collect()
    };
    let kind = match s.kind {
        StmtKind::Assign(n, v) => StmtKind::Assign(rename(&n, inst, m), e(v)),
        StmtKind::AssignIndex(n, i, v) => StmtKind::AssignIndex(rename(&n, inst, m), e(i), e(v)),
        StmtKind::If(arms, other) => {
            let mut new = Vec::new();
            for (c, b) in arms {
                new.push((e(c), block(b)?));
            }
            StmtKind::If(new, block(other)?)
        }
        StmtKind::Case(v, arms, other) => {
            let mut new = Vec::new();
            for (labels, b) in arms {
                let labels = labels.into_iter().map(|(a, z)| (e(a), z.map(e))).collect();
                new.push((labels, block(b)?));
            }
            StmtKind::Case(e(v), new, block(other)?)
        }
        StmtKind::For(n, a, b, c, body) => {
            StmtKind::For(rename(&n, inst, m), e(a), e(b), c.map(e), block(body)?)
        }
        StmtKind::While(c, body) => StmtKind::While(e(c), block(body)?),
        StmtKind::Repeat(body, c) => StmtKind::Repeat(block(body)?, e(c)),
        StmtKind::Call(n, args) => StmtKind::Call(
            rename(&n, inst, m),
            args.into_iter().map(|(k, v)| (k, e(v))).collect(),
        ),
        StmtKind::Exit => StmtKind::Exit,
        k @ StmtKind::Shared(..) => k,
        StmtKind::Return => {
            return Err(pos.err("RETURN bir FUNCTION / FUNCTION_BLOCK içinde desteklenmiyor"));
        }
    };
    Ok(Stmt { kind, pos })
}
