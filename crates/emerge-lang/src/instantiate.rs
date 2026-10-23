//! `use NAME version N as INSTANCE(arg: value, …);` written out (`proposals/reusable-operators.md` §3): each rule `R` of the
//! definition `operator NAME version N { … }` becomes a rule `INSTANCE_R` in the place of the `use`, with every use of a
//! parameter as a name replaced by the bound name and every use of a value parameter by its value. Definitions and uses
//! are then gone: the checker, the schedule and the runners see only ordinary rules, as with a loop over names. `format`
//! never calls this, so `emerge fmt` keeps the definitions and the uses; `emerge expand` prints the rules.

use crate::ast::*;
use crate::check::Problem;
use std::collections::{HashMap, HashSet};

fn problem(code: &'static str, path: String, pos: Pos, message: String) -> Problem {
    Problem { code, path, pos, message, fix: None }
}

/// The kinds of parameter that stand for a declaration's name.
pub fn names_a_declaration(kind: &str) -> bool {
    matches!(kind, "substance" | "source" | "sink" | "field" | "reaction")
}

/// Whether `v` can be given for a parameter of `kind`: a plain name for a declaration, a literal of the kind (or a
/// constant's name, resolved later) for a value.
fn fits(kind: &str, v: &Value) -> bool {
    let name = matches!(v, Value::Expr(Expr::Path(p)) if p.segs.is_empty());
    match kind {
        k if names_a_declaration(k) => name,
        "amount" => name || matches!(v, Value::Expr(Expr::Lit(Lit::Amount(_)))),
        "rate" => name || matches!(v, Value::Expr(Expr::Lit(Lit::Rate(_)))),
        "int" => name || matches!(v, Value::Expr(Expr::Lit(Lit::Int(_)))),
        "float" => name || matches!(v, Value::Expr(Expr::Lit(Lit::Float(_)))),
        _ => false,
    }
}

/// What each parameter of a definition is bound to in one instance: a name, or a value.
struct Binding {
    names: HashMap<String, String>,
    values: HashMap<String, Expr>,
}

impl Binding {
    fn name(&self, n: &str) -> String {
        self.names.get(n).cloned().unwrap_or_else(|| n.to_string())
    }
}

/// The file with every `use` of every world written out and the definitions taken away, or the first problem found.
pub fn instantiate(file: &File) -> Result<File, Problem> {
    let mut out = file.clone();
    for item in &mut out.items {
        let Item::World(w) = &mut item.node else { continue };
        if !w.decls.iter().any(|d| matches!(d.node, WDecl::Operator { .. } | WDecl::Use { .. })) {
            continue;
        }
        let world = w.name.clone();
        // The definitions, checked once each.
        let mut defs: Vec<(String, i64, Vec<Node<OParam>>, Vec<Node<WDecl>>)> = Vec::new();
        for d in &w.decls {
            let WDecl::Operator { name, version, params, rules } = &d.node else { continue };
            let path = format!("{world}.operators.{name}@v{version}");
            if defs.iter().any(|(n, v, _, _)| n == name && v == version) {
                return Err(problem("E0111", path, d.pos, format!("operator `{name}` version {version} is declared twice")));
            }
            let mut seen = HashSet::new();
            for p in params {
                if !PARAM_KINDS.contains(&p.node.kind.as_str()) {
                    return Err(problem("E0205", path, p.pos, format!("`{}` is not a kind of parameter: {}", p.node.kind, PARAM_KINDS.join(", "))));
                }
                if !seen.insert(p.node.name.clone()) {
                    return Err(problem("E0111", path, p.pos, format!("parameter `{}` is declared twice", p.node.name)));
                }
                if let Some(v) = &p.node.default {
                    if !fits(&p.node.kind, v) {
                        return Err(problem("E0205", path, p.pos, format!("the default of `{}` is not a {}", p.node.name, p.node.kind)));
                    }
                }
            }
            let mut rnames = HashSet::new();
            for r in rules {
                let WDecl::Rule { name: rn, body, .. } = &r.node else { return Err(problem("E0301", path, r.pos, "an operator holds parameters and rules only".into())) };
                if !rnames.insert(rn.clone()) {
                    return Err(problem("E0111", path, r.pos, format!("rule `{rn}` is declared twice in the operator")));
                }
                let _ = body;
            }
            defs.push((name.clone(), *version, params.clone(), rules.clone()));
        }
        let taken: HashSet<String> = w.decls.iter().filter_map(|d| crate::check::declared(&d.node).map(|(n, _)| n.to_string())).collect();
        let mut instances = HashSet::new();
        let mut made: HashSet<String> = HashSet::new();
        let mut decls = Vec::with_capacity(w.decls.len());
        for d in std::mem::take(&mut w.decls) {
            match d.node {
                WDecl::Operator { .. } => {}
                WDecl::Use { def, version, name, args } => {
                    let path = format!("{world}.uses.{name}");
                    if !instances.insert(name.clone()) {
                        return Err(problem("E0111", path, d.pos, format!("instance `{name}` is declared twice")));
                    }
                    let Some((_, _, params, rules)) = defs.iter().find(|(n, v, _, _)| *n == def && *v == version) else {
                        let have: Vec<String> = defs.iter().filter(|(n, _, _, _)| *n == def).map(|(_, v, _, _)| v.to_string()).collect();
                        let msg = if have.is_empty() { format!("no operator `{def}` in this world: put its definition in the world first") } else { format!("operator `{def}` has no version {version} here (it has {})", have.join(", ")) };
                        return Err(problem("E0114", path, d.pos, msg));
                    };
                    let mut b = Binding { names: HashMap::new(), values: HashMap::new() };
                    let mut given = HashMap::new();
                    for a in &args {
                        let Some(k) = &a.name else { return Err(problem("E0205", path, d.pos, "every argument of an instance is named: `use NAME version N as I(param: value)`".into())) };
                        if !params.iter().any(|p| p.node.name == *k) {
                            return Err(problem("E0115", path, d.pos, format!("operator `{def}` has no parameter `{k}` (it has {})", params.iter().map(|p| format!("`{}`", p.node.name)).collect::<Vec<_>>().join(", "))));
                        }
                        given.insert(k.clone(), a.value.clone());
                    }
                    for p in params {
                        let v = match (given.get(&p.node.name), &p.node.default) {
                            (Some(v), _) | (None, Some(v)) => v.clone(),
                            (None, None) => return Err(problem("E0116", path, d.pos, format!("`{}` needs a {} for its parameter `{}`", name, p.node.kind, p.node.name))),
                        };
                        if !fits(&p.node.kind, &v) {
                            return Err(problem("E0205", path, d.pos, format!("`{}` of `{name}` is a {}: {}", p.node.name, p.node.kind, if names_a_declaration(&p.node.kind) { "give a name of this world" } else { "give a literal of that kind" })));
                        }
                        match (&v, names_a_declaration(&p.node.kind)) {
                            (Value::Expr(Expr::Path(q)), true) => {
                                b.names.insert(p.node.name.clone(), q.root.clone());
                            }
                            (Value::Expr(e), _) => {
                                b.values.insert(p.node.name.clone(), e.clone());
                            }
                            _ => unreachable!("checked by `fits`"),
                        }
                    }
                    let mut first = true;
                    for r in rules {
                        let WDecl::Rule { name: rn, on, body } = &r.node else { continue };
                        let rule = format!("{name}_{rn}");
                        if taken.contains(&rule) || !made.insert(rule.clone()) {
                            return Err(problem("E0111", path, d.pos, format!("instance `{name}` writes a rule `{rule}`, a name this world already has")));
                        }
                        let body = block(body, &b);
                        let trivia = if first { Trivia { blank_before: true, ..d.trivia.clone() } } else { Trivia { blank_before: true, ..Trivia::default() } };
                        first = false;
                        decls.push(Node { trivia, pos: d.pos, node: WDecl::Rule { name: rule, on: b.name(on), body } });
                    }
                }
                other => decls.push(Node { node: other, ..d }),
            }
        }
        w.decls = decls;
    }
    Ok(out)
}

fn block(body: &Block, b: &Binding) -> Block {
    body.iter().map(|n| Node { node: stmt(&n.node, b), ..n.clone() }).collect()
}

fn stmt(s: &Stmt, b: &Binding) -> Stmt {
    match s {
        Stmt::Let { name, value } => Stmt::Let { name: name.clone(), value: expr(value, b) },
        Stmt::When { cond, then, otherwise } => Stmt::When { cond: expr(cond, b), then: block(then, b), otherwise: otherwise.as_ref().map(|o| block(o, b)) },
        Stmt::For { var, field, body } => Stmt::For { var: var.clone(), field: b.name(field), body: block(body, b) },
        Stmt::Each { var, items, body } => Stmt::Each { var: var.clone(), items: items.iter().map(|i| b.name(i)).collect(), body: block(body, b) },
        Stmt::Assign { target, value } => Stmt::Assign { target: path(target, b), value: expr(value, b) },
        Stmt::Call { call: c, label } => Stmt::Call { call: call(c, b), label: label.clone() },
        Stmt::Setting { key, value } => Stmt::Setting { key: key.clone(), value: expr(value, b) },
    }
}

fn call(c: &Call, b: &Binding) -> Call {
    Call { name: c.name.clone(), args: c.args.iter().map(|a| Arg { name: a.name.clone(), value: value(&a.value, b) }).collect() }
}

fn value(v: &Value, b: &Binding) -> Value {
    match v {
        Value::Expr(e) => Value::Expr(expr(e, b)),
        other => other.clone(),
    }
}

fn path(p: &Path, b: &Binding) -> Path {
    Path { root: b.name(&p.root), segs: p.segs.iter().map(|g| match g { Seg::Field(n) => Seg::Field(b.name(n)), Seg::Label(n) => Seg::Label(n.clone()) }).collect() }
}

fn expr(e: &Expr, b: &Binding) -> Expr {
    let r = |x: &Expr| Box::new(expr(x, b));
    match e {
        // A value parameter used on its own is its value; a name parameter is the bound name wherever a name stands.
        Expr::Path(p) if p.segs.is_empty() && b.values.contains_key(&p.root) => b.values[&p.root].clone(),
        Expr::Path(p) => Expr::Path(path(p, b)),
        Expr::Call(c) => Expr::Call(call(c, b)),
        Expr::List(items) => Expr::List(items.iter().map(|v| value(v, b)).collect()),
        Expr::Paren(x) => Expr::Paren(r(x)),
        Expr::Neg(x) => Expr::Neg(r(x)),
        Expr::Not(x) => Expr::Not(r(x)),
        Expr::Binary(op, x, y) => Expr::Binary(*op, r(x), r(y)),
        Expr::Pipe(x, c) => Expr::Pipe(r(x), call(c, b)),
        Expr::Lit(_) | Expr::Capture(_) => e.clone(),
    }
}

/// A block with names replaced (`names`: old to new) wherever a name stands, as an instance's bindings are put in: for a
/// template inserted with other bindings (`reuse::template_rules`).
pub fn rename(body: &Block, names: &HashMap<String, String>) -> Block {
    block(body, &Binding { names: names.clone(), values: HashMap::new() })
}

/// A block with each use of a name in `values` on its own replaced by that value (an operator's value parameters).
pub fn put_values(body: &Block, values: &HashMap<String, Expr>) -> Block {
    block(body, &Binding { names: HashMap::new(), values: values.clone() })
}
