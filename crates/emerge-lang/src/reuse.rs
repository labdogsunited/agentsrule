//! Reuse in the node editor (`proposals/reusable-operators.md`): templates made from selected statements and written back
//! with other bindings, and an operator definition made from selected rules. The server keeps both in its library; what
//! reaches a world is ordinary source, made into one patch by `graph::op_patch`.

use crate::ast::*;
use crate::fmt::{decl_text, stmt_text, value_text};
use std::collections::{BTreeMap, HashMap};

/// A template: rules of statements (each with the field it works on), the declarations they name as bindings (the name it
/// had, and its kind), and the amounts and rates of labelled statements as controls (`label.arg`, the value it had).
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub rules: Vec<(String, String, String)>,
    pub bindings: Vec<(String, &'static str)>,
    pub controls: Vec<(String, String)>,
}

fn world<'a>(file: &'a File, name: &str) -> Result<&'a World, String> {
    file.items.iter().find_map(|i| match &i.node { Item::World(w) if w.name == name => Some(w), _ => None }).ok_or_else(|| format!("no world `{name}`"))
}

/// The kind of declaration a name is in `w`, among those a template or an operator can bind.
fn decl_kind(w: &World, n: &str) -> Option<&'static str> {
    w.decls.iter().find_map(|d| match &d.node {
        WDecl::Substance { name, .. } if name == n => Some("substance"),
        WDecl::Source { name, .. } if name == n => Some("source"),
        WDecl::Sink { name, .. } if name == n => Some("sink"),
        WDecl::Field { name, .. } if name == n => Some("field"),
        WDecl::Reaction { name, .. } if name == n => Some("reaction"),
        _ => None,
    })
}

/// Every name a block uses as a name: paths' roots and fields, call subjects and named arguments, loop items.
fn names_in(body: &[Node<Stmt>], out: &mut Vec<String>) {
    fn e(x: &Expr, out: &mut Vec<String>) {
        match x {
            Expr::Path(p) => {
                out.push(p.root.clone());
                out.extend(p.segs.iter().filter_map(|s| match s { Seg::Field(f) => Some(f.clone()), _ => None }));
            }
            Expr::Call(c) => c.args.iter().for_each(|a| v(&a.value, out)),
            Expr::List(items) => items.iter().for_each(|i| v(i, out)),
            Expr::Paren(y) | Expr::Neg(y) | Expr::Not(y) => e(y, out),
            Expr::Binary(_, a, b) => { e(a, out); e(b, out); }
            Expr::Pipe(y, c) => { e(y, out); c.args.iter().for_each(|a| v(&a.value, out)); }
            Expr::Lit(_) | Expr::Capture(_) => {}
        }
    }
    fn v(x: &Value, out: &mut Vec<String>) {
        if let Value::Expr(y) = x { e(y, out) }
    }
    for s in body {
        match &s.node {
            Stmt::Let { value, .. } | Stmt::Setting { value, .. } => e(value, out),
            Stmt::When { cond, then, otherwise } => { e(cond, out); names_in(then, out); if let Some(o) = otherwise { names_in(o, out) } }
            Stmt::For { field, body, .. } => { out.push(field.clone()); names_in(body, out) }
            Stmt::Each { items, body, .. } => { out.extend(items.iter().cloned()); names_in(body, out) }
            Stmt::Assign { target, value } => { e(&Expr::Path(target.clone()), out); e(value, out) }
            Stmt::Call { call, .. } => call.args.iter().for_each(|a| v(&a.value, out)),
        }
    }
}

/// The top-level statement a box id stands for, and the rule (name, field) it is in.
fn statement<'a>(w: &'a World, world_name: &str, id: &str) -> Result<(&'a str, &'a str, usize, &'a Node<Stmt>), String> {
    let top = id.split('/').next().unwrap_or(id);
    let rest = top.strip_prefix(&format!("{world_name}.rules.")).ok_or_else(|| format!("`{id}` is not a statement of a rule"))?;
    for d in &w.decls {
        let WDecl::Rule { name, on, body } = &d.node else { continue };
        for (i, st) in body.iter().enumerate() {
            let me = match &st.node { Stmt::Call { label: Some(l), .. } => format!("{name}@{l}"), _ => format!("{name}#{i}") };
            if me == rest {
                return Ok((name, on, i, st));
            }
        }
    }
    Err(format!("no statement `{id}`"))
}

/// A template of the statements `ids` (top-level ones; one inside a `when` brings its whole `when`).
pub fn template_from(file: &File, world_name: &str, ids: &[String]) -> Result<Template, String> {
    let w = world(file, world_name)?;
    let mut by_rule: Vec<(String, String, Vec<(usize, String)>)> = Vec::new();
    let mut names = Vec::new();
    let mut controls = Vec::new();
    for id in ids {
        let (rule, on, i, st) = statement(w, world_name, id)?;
        let slot = match by_rule.iter().position(|(r, _, _)| r == rule) {
            Some(k) => k,
            None => { by_rule.push((rule.to_string(), on.to_string(), Vec::new())); by_rule.len() - 1 }
        };
        if by_rule[slot].2.iter().any(|(k, _)| *k == i) { continue; }
        by_rule[slot].2.push((i, stmt_text(&st.node)));
        names.push(on.to_string());
        names_in(std::slice::from_ref(st), &mut names);
        if let Stmt::Call { call, label: Some(l) } = &st.node {
            for a in &call.args {
                if let (Some(k), Value::Expr(Expr::Lit(Lit::Amount(_) | Lit::Rate(_)))) = (&a.name, &a.value) {
                    controls.push((format!("{l}.{k}"), value_text(&a.value)));
                }
            }
        }
    }
    if by_rule.is_empty() {
        return Err("select the statements to keep as a template".into());
    }
    let mut bindings: Vec<(String, &'static str)> = Vec::new();
    for n in names {
        if let Some(k) = decl_kind(w, &n) {
            if !bindings.iter().any(|(b, _)| *b == n) {
                bindings.push((n, k));
            }
        }
    }
    let rules = by_rule.into_iter().map(|(r, on, mut st)| { st.sort_by_key(|(i, _)| *i); (r, on, st.into_iter().map(|(_, t)| t).collect::<Vec<_>>().join("\n")) }).collect();
    Ok(Template { rules, bindings, controls })
}

/// The statements of a block from text (as a rule holds them).
fn parse_block(text: &str) -> Result<Block, String> {
    let src = format!("world T {{ rule r on f {{ {text} }} }}");
    let file = crate::parse(&src).map_err(|d| format!("{}: {}", d.code, d.message))?;
    for item in &file.items {
        if let Item::World(w) = &item.node {
            for d in &w.decls {
                if let WDecl::Rule { body, .. } = &d.node {
                    return Ok(body.clone());
                }
            }
        }
    }
    Err("not a statement".to_string())
}

fn parse_value(text: &str) -> Result<Value, String> {
    let b = parse_block(&format!("f(v: {text});"))?;
    match b.first().map(|s| &s.node) {
        Some(Stmt::Call { call, .. }) => call.args.first().map(|a| a.value.clone()).ok_or_else(|| "no value".into()),
        _ => Err("not a value".into()),
    }
}

/// A template's rules with `bindings` (a binding's name to the name to use; unbound ones keep theirs) and `controls`
/// (`label.arg` to a value) put in: (a name for the rule, its field, its statements as text).
pub fn template_rules(t: &Template, bindings: &BTreeMap<String, String>, controls: &BTreeMap<String, String>) -> Result<Vec<(String, String, String)>, String> {
    let names: HashMap<String, String> = bindings.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    for k in bindings.keys() {
        if !t.bindings.iter().any(|(b, _)| b == k) {
            return Err(format!("the template has no binding `{k}`"));
        }
    }
    for k in controls.keys() {
        if !t.controls.iter().any(|(c, _)| c == k) {
            return Err(format!("the template has no control `{k}`"));
        }
    }
    let mut out = Vec::new();
    for (rule, on, text) in &t.rules {
        let mut body = crate::instantiate::rename(&parse_block(text)?, &names);
        for (key, value) in controls {
            let (label, arg) = key.split_once('.').ok_or_else(|| format!("`{key}` is not `label.arg`"))?;
            let v = parse_value(value)?;
            for s in body.iter_mut() {
                if let Stmt::Call { call, label: Some(l) } = &mut s.node {
                    if l == label {
                        if let Some(a) = call.args.iter_mut().find(|a| a.name.as_deref() == Some(arg)) { a.value = v.clone() }
                    }
                }
            }
        }
        out.push((rule.clone(), names.get(on).cloned().unwrap_or_else(|| on.clone()), body.iter().map(|s| stmt_text(&s.node)).collect::<Vec<_>>().join(" ")));
    }
    Ok(out)
}

/// An operator definition made of the rules that hold `ids` (whole rules, which must stand next to each other among the
/// rules, so their order is kept): every declaration they name becomes a parameter of its kind (named as it is, so the
/// rules read as before), and every amount or rate of a labelled statement a value parameter `LABEL_ARG` with its value as
/// the default. Returns the definition, the rules it takes (in order) and the instance's arguments.
pub fn operator_from(file: &File, world_name: &str, ids: &[String], name: &str, version: i64) -> Result<(WDecl, Vec<String>, Vec<Arg>), String> {
    let w = world(file, world_name)?;
    let mut rules: Vec<String> = Vec::new();
    for id in ids {
        let (rule, ..) = statement(w, world_name, id)?;
        if !rules.iter().any(|r| r == rule) {
            rules.push(rule.to_string());
        }
    }
    if rules.is_empty() {
        return Err("select the statements whose rules make the operator".into());
    }
    // In declaration order, and next to each other among the rules and instances.
    let order: Vec<&str> = w.decls.iter().filter_map(|d| match &d.node { WDecl::Rule { name, .. } => Some(name.as_str()), WDecl::Use { name, .. } => Some(name.as_str()), _ => None }).collect();
    let mut at: Vec<usize> = rules.iter().map(|r| order.iter().position(|o| o == r).unwrap()).collect();
    at.sort();
    if at.windows(2).any(|p| p[1] != p[0] + 1) {
        return Err("the rules are not next to each other: making them one operator would change the order they run in".into());
    }
    let rules: Vec<String> = at.iter().map(|&i| order[i].to_string()).collect();
    let (mut params, mut args): (Vec<Node<OParam>>, Vec<Arg>) = (Vec::new(), Vec::new());
    let mut body_rules = Vec::new();
    fn node<T>(x: T) -> Node<T> {
        Node { trivia: Trivia::default(), pos: Pos::default(), node: x }
    }
    for r in &rules {
        let Some(WDecl::Rule { on, body, .. }) = w.decls.iter().map(|d| &d.node).find(|d| matches!(d, WDecl::Rule { name, .. } if name == r)) else { continue };
        let mut names = vec![on.clone()];
        names_in(body, &mut names);
        for n in names {
            if let Some(k) = decl_kind(w, &n) {
                if !params.iter().any(|p: &Node<OParam>| p.node.name == n) {
                    // Named as it is and defaulting to itself: a world with the same names uses it as it is.
                    params.push(node(OParam { name: n.clone(), kind: k.to_string(), default: Some(Value::Expr(Expr::Path(Path { root: n.clone(), segs: vec![] }))) }));
                    args.push(Arg { name: Some(n.clone()), value: Value::Expr(Expr::Path(Path { root: n.clone(), segs: vec![] })) });
                }
            }
        }
        // Amounts and rates of labelled statements: value parameters, defaulting to what they are.
        let mut body = body.clone();
        for s in body.iter_mut() {
            if let Stmt::Call { call, label: Some(l) } = &mut s.node {
                for a in call.args.iter_mut() {
                    let kind = match (&a.name, &a.value) {
                        (Some(_), Value::Expr(Expr::Lit(Lit::Amount(_)))) => "amount",
                        (Some(_), Value::Expr(Expr::Lit(Lit::Rate(_)))) => "rate",
                        _ => continue,
                    };
                    let p = format!("{l}_{}", a.name.as_deref().unwrap_or(""));
                    if params.iter().any(|x| x.node.name == p) { continue; }
                    params.push(node(OParam { name: p.clone(), kind: kind.to_string(), default: Some(a.value.clone()) }));
                    a.value = Value::Expr(Expr::Path(Path { root: p, segs: vec![] }));
                }
            }
        }
        body_rules.push(node(WDecl::Rule { name: r.clone(), on: on.clone(), body }));
    }
    let _ = decl_text;
    Ok((WDecl::Operator { name: name.to_string(), version, params, rules: body_rules }, rules, args))
}
