//! `for s in [carbon, nitrate] { ... }` in a world rule (`language.md` §7.5): the body written
//! once for each name, worked out before checking. `format` never calls this, so `fmt` keeps the
//! loop; `emerge expand` prints it unrolled.
//!
//! The variable stands for a name. In each copy of the body every use of the variable as a name
//! (a substance, a direction, a field, a label's part) is replaced by the item, so the checker, the
//! schedule and the runners see only the statements written out. A `@label` inside a loop gets
//! `_` and the item added (`@sun` becomes `@sun_carbon`), so labels stay unique. Loops may nest.

use crate::ast::*;
use crate::check::Problem;

fn problem(code: &'static str, path: &str, pos: Pos, message: String) -> Problem {
    Problem { code, path: path.to_string(), pos, message, fix: None }
}

/// The file with every loop of a world rule unrolled, or the first problem found.
pub fn unroll(file: &File) -> Result<File, Problem> {
    let mut out = file.clone();
    for item in &mut out.items {
        let Item::World(w) = &mut item.node else { continue };
        let names: Vec<String> = w.decls.iter().filter_map(|d| declared(&d.node)).collect();
        for d in &mut w.decls {
            if let WDecl::Rule { name, body, .. } = &mut d.node {
                *body = block(&format!("{}.rules.{name}", w.name), &names, body)?;
            }
        }
    }
    Ok(out)
}

/// A name the world declares, which a loop variable cannot take (`E0111`).
fn declared(d: &WDecl) -> Option<String> {
    match d {
        WDecl::Element(n) | WDecl::Const { name: n, .. } | WDecl::Substance { name: n, .. } | WDecl::Field { name: n, .. } | WDecl::Source { name: n, .. } | WDecl::Sink { name: n, .. } | WDecl::Reaction { name: n, .. } => Some(n.clone()),
        _ => None,
    }
}

fn block(path: &str, names: &[String], body: &Block) -> Result<Block, Problem> {
    let mut out = Vec::new();
    for s in body {
        match &s.node {
            Stmt::Each { var, items, body } => {
                if crate::resolve::is_reserved(var) {
                    return Err(problem("E0110", path, s.pos, format!("`{var}` is a keyword or a built-in name, so a loop cannot use it as its variable")));
                }
                if names.iter().any(|n| n == var) {
                    return Err(problem("E0111", path, s.pos, format!("`{var}` is already the name of something in this world, so a loop cannot use it as its variable")));
                }
                if items.is_empty() {
                    return Err(problem("E0120", path, s.pos, "a loop needs at least one name to go through".into()));
                }
                for (k, it) in items.iter().enumerate() {
                    if items[..k].contains(it) {
                        return Err(problem("E0120", path, s.pos, format!("`{it}` appears twice in the loop's list")));
                    }
                }
                for it in items {
                    let copy: Block = body.iter().map(|b| Node { node: subst_stmt(&b.node, var, it), ..b.clone() }).collect();
                    out.extend(block(path, names, &copy)?);
                }
            }
            Stmt::When { cond, then, otherwise } => {
                let then = block(path, names, then)?;
                let otherwise = match otherwise { Some(o) => Some(block(path, names, o)?), None => None };
                out.push(Node { node: Stmt::When { cond: cond.clone(), then, otherwise }, ..s.clone() });
            }
            _ => out.push(s.clone()),
        }
    }
    Ok(out)
}

fn subst_stmt(s: &Stmt, var: &str, to: &str) -> Stmt {
    let b = |body: &Block| -> Block { body.iter().map(|n| Node { node: subst_stmt(&n.node, var, to), ..n.clone() }).collect() };
    match s {
        Stmt::Let { name, value } => Stmt::Let { name: name.clone(), value: expr(value, var, to) },
        Stmt::When { cond, then, otherwise } => Stmt::When { cond: expr(cond, var, to), then: b(then), otherwise: otherwise.as_ref().map(b) },
        // An inner loop that reuses the variable hides it; otherwise the outer name reaches into it.
        Stmt::Each { var: v, items, body } => Stmt::Each {
            var: v.clone(),
            items: items.iter().map(|i| if i == var { to.to_string() } else { i.clone() }).collect(),
            body: if v == var { body.clone() } else { b(body) },
        },
        Stmt::For { .. } | Stmt::Setting { .. } => s.clone(),
        Stmt::Assign { target, value } => Stmt::Assign { target: path(target, var, to), value: expr(value, var, to) },
        Stmt::Call { call: c, label } => Stmt::Call { call: call(c, var, to), label: label.as_ref().map(|l| format!("{l}_{to}")) },
    }
}

fn call(c: &Call, var: &str, to: &str) -> Call {
    Call { name: c.name.clone(), args: c.args.iter().map(|a| Arg { name: a.name.clone(), value: value(&a.value, var, to) }).collect() }
}

fn value(v: &Value, var: &str, to: &str) -> Value {
    match v {
        Value::Expr(e) => Value::Expr(expr(e, var, to)),
        other => other.clone(),
    }
}

fn path(p: &Path, var: &str, to: &str) -> Path {
    let s = |n: &String| if n == var { to.to_string() } else { n.clone() };
    Path { root: s(&p.root), segs: p.segs.iter().map(|g| match g { Seg::Field(n) => Seg::Field(s(n)), Seg::Label(n) => Seg::Label(n.clone()) }).collect() }
}

fn expr(e: &Expr, var: &str, to: &str) -> Expr {
    let r = |x: &Expr| Box::new(expr(x, var, to));
    match e {
        Expr::Path(p) => Expr::Path(path(p, var, to)),
        Expr::Call(c) => Expr::Call(call(c, var, to)),
        Expr::List(items) => Expr::List(items.iter().map(|v| value(v, var, to)).collect()),
        Expr::Paren(x) => Expr::Paren(r(x)),
        Expr::Neg(x) => Expr::Neg(r(x)),
        Expr::Not(x) => Expr::Not(r(x)),
        Expr::Binary(op, a, b) => Expr::Binary(*op, r(a), r(b)),
        Expr::Pipe(x, c) => Expr::Pipe(r(x), call(c, var, to)),
        Expr::Lit(_) | Expr::Capture(_) => e.clone(),
    }
}
