//! Constants (`language.md` §3.4): `const NAME = expr;` in a world, and constant arithmetic
//! worked out before checking. `format` never calls this, so `fmt` keeps the expressions.
//!
//! Every use of a constant, in the world or in a lineage of it, is replaced by its value,
//! and any arithmetic on literals is folded (`10_000 + 10_000` becomes `20_000`). The
//! arithmetic is the numeric contract of spec §10.1 and `language.md` §4.1: integers are
//! `i32` and saturate, amounts are `u32` µ and saturate at both ends, `amount * rate`
//! floors, `amount / int` floors, division by zero gives `0`, floats are `f32` operation by
//! operation. A combination `language.md` §4.1 does not list is left as written, and is an
//! error in a `const`. Positions that take a literal token (such as `potential` or `holds`)
//! cannot use a constant.

use std::collections::HashMap;

use crate::ast::*;
use crate::check::Problem;

fn problem(code: &'static str, path: String, pos: Pos, message: String) -> Problem {
    Problem { code, path, pos, message, fix: None }
}

/// The file with every constant replaced by its value and constant arithmetic folded.
pub fn resolve(file: &File) -> Result<File, Problem> {
    let mut out = file.clone();
    let mut worlds: HashMap<String, HashMap<String, Lit>> = HashMap::new();
    for item in &mut out.items {
        let Item::World(w) = &mut item.node else { continue };
        let consts = world_consts(w)?;
        w.decls.retain(|d| !matches!(d.node, WDecl::Const { .. }));
        for d in &mut w.decls {
            wdecl(&mut d.node, &consts);
        }
        worlds.insert(w.name.clone(), consts);
    }
    let none = HashMap::new();
    for item in &mut out.items {
        if let Item::Lineage(l) = &mut item.node {
            let consts = worlds.get(&l.world).unwrap_or(&none);
            check_shadow(&l.name, consts, &l.decls)?;
            for d in &mut l.decls {
                ldecl(&mut d.node, consts);
            }
        }
    }
    Ok(out)
}

/// The values of a world's constants, in any order of declaration.
fn world_consts(w: &World) -> Result<HashMap<String, Lit>, Problem> {
    let mut raw: Vec<(&str, &Expr, Pos)> = Vec::new();
    for d in &w.decls {
        if let WDecl::Const { name, value } = &d.node {
            let path = format!("{}.constants.{name}", w.name);
            if raw.iter().any(|r| r.0 == name) || w.decls.iter().any(|o| crate::check::declared(&o.node).is_some_and(|(n, _)| n == name)) {
                return Err(problem("E0111", path, d.pos, format!("`{name}` is already declared in {}", w.name)));
            }
            raw.push((name, value, d.pos));
        }
    }
    let mut done: HashMap<String, Lit> = HashMap::new();
    while done.len() < raw.len() {
        let before = done.len();
        for (name, value, _) in &raw {
            if done.contains_key(*name) {
                continue;
            }
            let mut e = (*value).clone();
            expr(&mut e, &done);
            if let Some(l) = fold(&e) {
                done.insert(name.to_string(), l);
            }
        }
        if done.len() == before {
            break;
        }
    }
    if let Some((name, value, pos)) = raw.iter().find(|r| !done.contains_key(r.0)) {
        let path = format!("{}.constants.{name}", w.name);
        let mut e = (*value).clone();
        expr(&mut e, &done);
        let waiting = refs(&e, &raw.iter().map(|r| r.0).collect::<Vec<_>>());
        return Err(match waiting.first() {
            Some(other) => problem("E0119", path, *pos, format!("constant `{name}` depends on `{other}`, which cannot be worked out (a cycle, or not a constant)")),
            None => problem("E0205", path, *pos, format!("constant `{name}` is not a constant expression of literals and other constants")),
        });
    }
    Ok(done)
}

/// Names from `names` that `e` still refers to.
fn refs(e: &Expr, names: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    let mut copy = e.clone();
    walk(&mut copy, &mut |x| {
        if let Expr::Path(p) = x {
            if p.segs.is_empty() && names.contains(&p.root.as_str()) {
                found.push(p.root.clone());
            }
        }
    });
    found
}

/// Calls `f` on `e` and on every expression inside it.
fn walk(e: &mut Expr, f: &mut dyn FnMut(&mut Expr)) {
    f(e);
    match e {
        Expr::Lit(_) | Expr::Path(_) | Expr::Capture(_) => {}
        Expr::Call(c) => call_walk(c, f),
        Expr::List(items) => items.iter_mut().for_each(|v| value_walk(v, f)),
        Expr::Paren(i) | Expr::Neg(i) | Expr::Not(i) => walk(i, f),
        Expr::Binary(_, a, b) => {
            walk(a, f);
            walk(b, f);
        }
        Expr::Pipe(a, c) => {
            walk(a, f);
            call_walk(c, f);
        }
    }
}

fn call_walk(c: &mut Call, f: &mut dyn FnMut(&mut Expr)) {
    c.args.iter_mut().for_each(|a| value_walk(&mut a.value, f));
}

fn value_walk(v: &mut Value, f: &mut dyn FnMut(&mut Expr)) {
    if let Value::Expr(e) = v {
        walk(e, f);
    }
}

/// E0111 for a `let` or `for` variable that has a constant's name: inside its block the
/// name would mean two things.
fn check_shadow(owner: &str, consts: &HashMap<String, Lit>, decls: &[Node<LDecl>]) -> Result<(), Problem> {
    for d in decls {
        if let LDecl::Genesis(b) = &d.node {
            shadow_block(owner, consts, b)?;
        }
    }
    Ok(())
}

fn shadow_block(owner: &str, consts: &HashMap<String, Lit>, b: &Block) -> Result<(), Problem> {
    for s in b {
        let name = match &s.node {
            Stmt::Let { name, .. } => Some(name),
            Stmt::For { var, .. } => Some(var),
            _ => None,
        };
        if let Some(n) = name.filter(|n| consts.contains_key(*n)) {
            return Err(problem("E0111", owner.to_string(), s.pos, format!("`{n}` is a constant of the world; it cannot also name a variable")));
        }
        match &s.node {
            Stmt::When { then, otherwise, .. } => {
                shadow_block(owner, consts, then)?;
                if let Some(o) = otherwise {
                    shadow_block(owner, consts, o)?;
                }
            }
            Stmt::For { body, .. } => shadow_block(owner, consts, body)?,
            _ => {}
        }
    }
    Ok(())
}

// ---------- replacing and folding ----------

fn wdecl(d: &mut WDecl, consts: &HashMap<String, Lit>) {
    match d {
        WDecl::Field { kind, .. } => call(kind, consts),
        WDecl::Reaction { catalyze, .. } => {
            if let Some(c) = catalyze {
                call(c, consts);
            }
        }
        WDecl::Menu { items, .. } => {
            for i in items {
                if let MenuItem::Action { requires: Some(r), .. } = &mut i.node {
                    expr(r, consts);
                }
            }
        }
        WDecl::Rule { body, .. } | WDecl::Genesis(body) => block(body, consts),
        WDecl::Config { entries, .. } => entries.iter_mut().for_each(|e| value(&mut e.node.value, consts)),
        WDecl::Setting(e) => value(&mut e.value, consts),
        _ => {}
    }
}

fn ldecl(d: &mut LDecl, consts: &HashMap<String, Lit>) {
    match d {
        LDecl::Profile { entries, .. } => entries.iter_mut().for_each(|e| value(&mut e.node.value, consts)),
        LDecl::Mutation(entries) => entries.iter_mut().for_each(|e| value(&mut e.node.value, consts)),
        LDecl::Genesis(b) => block(b, consts),
        LDecl::Seed { modules, .. } => {
            for m in modules {
                for g in &mut m.node.genes {
                    match &mut g.node.body {
                        GeneBody::Sense { call: c, .. } | GeneBody::Action { call: c, .. } => call(c, consts),
                        GeneBody::Regulate { call: c, .. } => call(c, consts),
                        GeneBody::Reaction { .. } => {}
                    }
                }
            }
        }
    }
}

fn block(b: &mut Block, consts: &HashMap<String, Lit>) {
    for s in b {
        match &mut s.node {
            Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Setting { value, .. } => expr(value, consts),
            Stmt::When { cond, then, otherwise } => {
                expr(cond, consts);
                block(then, consts);
                if let Some(o) = otherwise {
                    block(o, consts);
                }
            }
            Stmt::For { body, .. } => block(body, consts),
            Stmt::Call { call: c, .. } => call(c, consts),
        }
    }
}

fn call(c: &mut Call, consts: &HashMap<String, Lit>) {
    c.args.iter_mut().for_each(|a| value(&mut a.value, consts));
}

fn value(v: &mut Value, consts: &HashMap<String, Lit>) {
    if let Value::Expr(e) = v {
        expr(e, consts);
    }
}

/// Replaces constants in `e`, then folds what is left of literal arithmetic, inside out.
fn expr(e: &mut Expr, consts: &HashMap<String, Lit>) {
    match e {
        Expr::Lit(_) | Expr::Capture(_) => return,
        Expr::Path(p) => {
            if p.segs.is_empty() {
                if let Some(l) = consts.get(&p.root) {
                    *e = from_lit(l.clone());
                }
            }
            return;
        }
        Expr::Call(c) => call(c, consts),
        Expr::List(items) => items.iter_mut().for_each(|v| value(v, consts)),
        Expr::Paren(i) | Expr::Neg(i) | Expr::Not(i) => expr(i, consts),
        Expr::Binary(_, a, b) => {
            expr(a, consts);
            expr(b, consts);
        }
        Expr::Pipe(a, c) => {
            expr(a, consts);
            call(c, consts);
        }
    }
    let signed_literal = matches!(e, Expr::Neg(i) if matches!(**i, Expr::Lit(_)));
    if !signed_literal && matches!(e, Expr::Paren(_) | Expr::Neg(_) | Expr::Binary(..)) {
        if let Some(l) = fold(e) {
            let folded = from_lit(l);
            // A literal with its sign in front is already as folded as it gets.
            if &folded != e {
                *e = folded;
            }
        }
    }
}

/// A literal as an expression: negative numbers keep the parser's `-` in front.
fn from_lit(l: Lit) -> Expr {
    match l {
        Lit::Int(i) if i < 0 => Expr::Neg(Box::new(Expr::Lit(Lit::Int(-i)))),
        Lit::Float(s) if s.starts_with('-') => Expr::Neg(Box::new(Expr::Lit(Lit::Float(s[1..].to_string())))),
        l => Expr::Lit(l),
    }
}

/// The value of a literal-only expression, if `language.md` §4.1 defines it.
pub fn fold(e: &Expr) -> Option<Lit> {
    match e {
        Expr::Lit(l @ (Lit::Int(_) | Lit::Float(_) | Lit::Amount(_) | Lit::Rate(_) | Lit::Bool(_))) => Some(l.clone()),
        Expr::Paren(i) => fold(i),
        Expr::Neg(i) => match fold(i)? {
            Lit::Int(i) => Some(Lit::Int(i.saturating_neg().clamp(I32.0, I32.1))),
            Lit::Float(s) => Some(float(-parse(&s)?)),
            _ => None,
        },
        Expr::Binary(op, a, b) => binary(*op, fold(a)?, fold(b)?),
        _ => None,
    }
}

const I32: (i64, i64) = (i32::MIN as i64, i32::MAX as i64);

fn parse(s: &str) -> Option<f32> {
    s.parse::<f32>().ok()
}

/// A float literal: NaN and infinity become `0.0`; the text always has a `.` or an exponent.
fn float(v: f32) -> Lit {
    let v = if v.is_finite() { v } else { 0.0 };
    let v = if v == 0.0 { 0.0 } else { v };
    Lit::Float(format!("{v:?}"))
}

fn binary(op: BinOp, a: Lit, b: Lit) -> Option<Lit> {
    use BinOp::*;
    let clamp = |v: i64| Lit::Int(v.clamp(I32.0, I32.1));
    let amount = |v: i128| Lit::Amount(v.clamp(0, u32::MAX as i128) as u64);
    Some(match (a, b) {
        (Lit::Int(x), Lit::Int(y)) => match op {
            Add => clamp(x + y),
            Sub => clamp(x - y),
            Mul => clamp(x.saturating_mul(y)),
            Div => clamp(if y == 0 { 0 } else { x / y }),
            _ => return None,
        },
        (Lit::Amount(x), Lit::Amount(y)) => match op {
            Add => amount(x as i128 + y as i128),
            Sub => amount(x as i128 - y as i128),
            _ => return None,
        },
        (Lit::Amount(x), Lit::Rate(r)) | (Lit::Rate(r), Lit::Amount(x)) if op == Mul => amount(x as i128 * r as i128 / 10_000),
        (Lit::Amount(x), Lit::Int(y)) if op == Div => match y {
            0 => Lit::Amount(0),
            y if y > 0 => amount(x as i128 / y as i128),
            _ => return None,
        },
        (Lit::Float(x), Lit::Float(y)) => {
            let (x, y) = (parse(&x)?, parse(&y)?);
            match op {
                Add => float(x + y),
                Sub => float(x - y),
                Mul => float(x * y),
                Div => float(if y == 0.0 { 0.0 } else { x / y }),
                _ => return None,
            }
        }
        _ => return None,
    })
}
