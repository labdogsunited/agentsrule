//! The graph contract (`proposals/houdini-node-system.md` §4, §5, §7): what the node editor may know about the
//! boxes and wires of `graph::graph`, without guessing. Each box has named ports with a role and a type, each
//! wire a kind and the source edit it stands for (or the reason it has none), each parameter a type, unit and
//! help, each box the operations it supports and why it does not support the others, and every statement
//! inside a `when` is a box of its own in the rule's network. Everything is derived from the checked source;
//! nothing here is stored, so it cannot drift from it.

use crate::ast::*;
use crate::fmt::{stmt_text, value_text};
use crate::graph::{graph, GNode, Graph, Kind, Wire};
use std::collections::BTreeMap;

/// A named place on a box where a wire attaches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub key: &'static str,
    pub name: &'static str,
    /// `in` or `out`.
    pub dir: &'static str,
    /// `resource` (amounts move), `reference` (names a declaration), `value` (a computed value).
    pub role: &'static str,
    /// What may attach: `store`, `reaction`, `sink`.
    pub ty: &'static str,
    pub required: bool,
    pub many: bool,
}

/// What one parameter is: its type and unit read from its source text, its help, and whether a control may set it.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamMeta {
    pub key: String,
    pub text: String,
    /// `amount`, `rate`, `integer`, `number`, `name`, `expression`.
    pub ty: &'static str,
    pub unit: Option<&'static str>,
    /// A slider's range, and whether it is only a convenience (`convenience`) or a rule of the language (`language`).
    pub range: Option<(f64, f64)>,
    pub range_kind: &'static str,
    pub help: &'static str,
    pub editable: bool,
    pub why_not: Option<String>,
}

/// The operations a box supports, each with a reason when it does not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Caps {
    pub edit_text: bool,
    pub set_param: bool,
    pub delete: bool,
    pub rewire: bool,
    pub duplicate: bool,
    pub label: bool,
    pub why_not: BTreeMap<&'static str, String>,
}

/// What a statement reads and writes, found in the checked syntax; `complete` is false when part of it is behind a
/// construct this analysis does not follow (a `let`, an assignment, a call it does not know), so a missing name is
/// "not found", never "absent".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Deps {
    pub conditions: Vec<String>,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub references: Vec<String>,
    pub complete: bool,
}

/// A rule seen from the world overview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub on: String,
    pub statements: usize,
    pub nested: usize,
    pub kinds: Vec<String>,
    pub steps: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct NodeContract {
    pub context: String,
    /// `label` for a box a name keeps in place, `position` for one that is found by its place in its rule.
    pub identity: &'static str,
    pub ports: Vec<Port>,
    pub params: Vec<ParamMeta>,
    pub caps: Caps,
    pub deps: Deps,
    pub summary: Option<Summary>,
    /// Ids of the statements directly inside it (a `when`).
    pub children: Vec<String>,
    /// A `when`'s condition, as written: set with the `condition` edit, read on the start-of-tick snapshot.
    pub condition: Option<String>,
}

#[derive(Clone, Debug)]
pub struct WireContract {
    /// `resource flow` or `declaration reference`.
    pub kind: &'static str,
    pub from_port: &'static str,
    pub to_port: &'static str,
    /// Drawn from the formula or the field, not from an argument of its own: no edit of the wire exists.
    pub derived: bool,
    /// What connecting this wire writes in the source, if anything.
    pub edit: Option<String>,
    pub why_not: Option<String>,
}

/// A statement inside a `when` (or `else`): a box in the rule's network, owned by exactly one top-level statement.
#[derive(Clone, Debug)]
pub struct Nested {
    /// `<container id>/t0` (then, first), `/e1` (else, second), `/t0.e2` deeper.
    pub id: String,
    pub container: String,
    /// The rule it belongs to (its box id).
    pub group: String,
    pub kind: Kind,
    pub title: String,
    pub text: String,
    /// The condition under which it runs, as written, joined with `and` from the outside in.
    pub when: String,
    pub params: Vec<(String, String)>,
    pub meta: NodeContract,
}

pub struct Contract {
    pub nodes: BTreeMap<String, NodeContract>,
    /// Parallel to `Graph::wires`.
    pub wires: Vec<WireContract>,
    /// Wires the projection of top-level statements does not draw: the resource wires of statements inside a `when`, and
    /// value dependencies (what a condition or a signal statement reads and writes). Each with its contract.
    pub more_wires: Vec<(crate::graph::Wire, WireContract)>,
    pub nested: Vec<Nested>,
    /// `world`, then `rule:<name>` for each rule, with a breadcrumb.
    pub contexts: Vec<(String, String)>,
}

fn classify(text: &str) -> (&'static str, Option<&'static str>, Option<(f64, f64)>) {
    let t = text.trim();
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '_' || c == '.');
    if let Some(n) = t.strip_suffix("bp") {
        if digits(n) {
            return ("rate", Some("bp"), Some((0.0, 10_000.0)));
        }
    }
    if let Some(n) = t.strip_suffix("mu") {
        if digits(n) {
            return ("amount", Some("mu"), None);
        }
    }
    if let Some(n) = t.strip_suffix('u') {
        if digits(n) {
            return ("amount", Some("u"), None);
        }
    }
    if digits(t) {
        return (if t.contains('.') { "number" } else { "integer" }, None, None);
    }
    if !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return ("name", None, None);
    }
    ("expression", None, None)
}

fn help(call: &str, key: &str) -> &'static str {
    match (call, key) {
        ("transfer", "amount") => "how much moves in each cell each tick the rule runs, before the origin's balance and the destination's room cut it",
        ("transfer", "from") => "where the substance comes from: a source, or `self` for the cell the rule works on",
        ("transfer", "to") => "where it goes: a field, `self`, or a neighbouring cell (above, right, below, left)",
        ("transfer", "overflow") => "a sink that takes what does not fit in the destination",
        ("react", "rate") => "the share of each cell's input that reacts each tick, in basis points (100bp is one percent)",
        ("diffuse_conserved", "rate") => "the share of each cell's amount offered to its neighbours each tick, in basis points",
        (_, "amount") => "an amount",
        (_, "rate") => "a share, in basis points",
        _ => "",
    }
}

fn param_meta(call: &str, k: &str, v: &str) -> ParamMeta {
    let (ty, unit, range) = classify(v);
    ParamMeta {
        key: k.to_string(),
        text: v.to_string(),
        ty: if matches!(k, "from" | "to" | "overflow") { "name" } else { ty },
        unit,
        range,
        range_kind: if range.is_some() { "convenience" } else { "" },
        help: help(call, k),
        editable: true,
        why_not: None,
    }
}

/// Names a statement's expression mentions: the roots of paths and the fields after `self`.
fn idents(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Path(p) => {
            if p.root == "self" || p.root == "cell" {
                out.extend(p.segs.iter().filter_map(|s| match s {
                    Seg::Field(f) => Some(f.clone()),
                    _ => None,
                }));
            } else if NEIGHBOURS.contains(&p.root.as_str()) {
                // `below.grain`, `moore.carbon`: a neighbour's amount, so the substance is what is read.
                out.extend(p.segs.iter().filter_map(|s| match s {
                    Seg::Field(f) => Some(f.clone()),
                    _ => None,
                }));
            } else {
                out.push(p.root.clone());
                // `scent.here`: another field read where the cell is.
            }
        }
        Expr::Call(c) => c.args.iter().for_each(|a| value_idents(&a.value, out)),
        Expr::List(v) => v.iter().for_each(|x| value_idents(x, out)),
        Expr::Paren(x) | Expr::Neg(x) | Expr::Not(x) => idents(x, out),
        Expr::Binary(_, a, b) => {
            idents(a, out);
            idents(b, out);
        }
        Expr::Pipe(x, c) => {
            idents(x, out);
            c.args.iter().for_each(|a| value_idents(&a.value, out));
        }
        Expr::Lit(_) | Expr::Capture(_) => {}
    }
}

/// The cells a rule may read beside its own: one neighbour, or a collection of them.
const NEIGHBOURS: [&str; 6] = ["above", "below", "left", "right", "moore", "vonneumann"];

/// Names a world rule may use that declare nothing: built-in functions, directions and the like. A name outside these and
/// the world's declarations is a local (`let`) or unknown, so what it stands for is not followed.
const BUILTINS: [&str; 24] = ["sum", "min", "max", "mean", "count", "argmin", "argmax", "cap", "select", "blur", "fade", "abs", "sqrt", "sin", "tanh", "units", "amount", "float", "tick", "self", "cell", "here", "value", "life_like"];

fn value_idents(v: &Value, out: &mut Vec<String>) {
    if let Value::Expr(e) = v {
        idents(e, out);
    }
}

fn arg<'a>(c: &'a Call, key: &str) -> Option<&'a Value> {
    c.args.iter().find(|a| a.name.as_deref() == Some(key)).map(|a| &a.value)
}

fn names(v: Option<&Value>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(v) = v {
        value_idents(v, &mut out);
    }
    out
}

type Formulas = BTreeMap<String, (Vec<String>, Vec<String>)>;

/// What one call statement reads and writes, and what declarations it names.
fn call_deps(c: &Call, formulas: &Formulas, on: &str, deps: &mut Deps) {
    let subject: Vec<String> = c.args.iter().find(|a| a.name.is_none()).map(|a| names(Some(&a.value))).unwrap_or_default();
    let cells = |n: String| if n == "self" { on.to_string() } else { n };
    match c.name.as_str() {
        "transfer" => {
            deps.reads.extend(names(arg(c, "from")).into_iter().map(cells));
            deps.writes.extend(names(arg(c, "to")).into_iter().map(cells));
            deps.references.extend(subject);
            deps.references.extend(names(arg(c, "overflow")));
        }
        "react" => {
            for r in &subject {
                deps.references.push(r.clone());
                if let Some((ins, outs)) = formulas.get(r) {
                    deps.reads.extend(ins.iter().cloned());
                    deps.writes.extend(outs.iter().cloned());
                }
            }
        }
        "diffuse_conserved" => {
            deps.reads.extend(subject.iter().cloned());
            deps.writes.extend(subject.iter().cloned());
        }
        _ => deps.complete = false,
    }
}

fn kind_of(st: &Stmt) -> (Kind, &'static str, &'static str) {
    match st {
        Stmt::Call { call, .. } if call.name == "transfer" => (Kind::Transfer, "transfer", "transfer"),
        Stmt::Call { call, .. } if call.name == "react" => (Kind::React, "react", "react"),
        Stmt::Call { call, .. } if call.name == "diffuse_conserved" => (Kind::Diffuse, "diffuse", "diffuse_conserved"),
        Stmt::When { .. } => (Kind::Code, "when", ""),
        Stmt::Assign { .. } => (Kind::Signal, "set", ""),
        _ => (Kind::Code, "code", ""),
    }
}

/// What a walk through a rule's statements needs: the rule, its field, the world's names and formulas.
struct Cx<'a> {
    world: &'a str,
    w: &'a World,
    rule: &'a str,
    on: &'a str,
    formulas: &'a Formulas,
}

impl Cx<'_> {
    /// The box id of the store (field, source, sink) called `n`.
    fn store(&self, n: &str) -> Option<String> {
        self.w.decls.iter().find_map(|d| match &d.node {
            WDecl::Field { name, .. } if name == n => Some(format!("{}.fields.{n}", self.world)),
            WDecl::Source { name, .. } if name == n => Some(format!("{}.sources.{n}", self.world)),
            WDecl::Sink { name, .. } if name == n => Some(format!("{}.sinks.{n}", self.world)),
            _ => None,
        })
    }
    fn cells(&self) -> Option<String> {
        self.store(self.on)
    }
    /// A name that stands for the cells the rule works on, or a store.
    fn target(&self, n: &str) -> Option<String> {
        if n == "self" || n == "cell" || NEIGHBOURS[..4].contains(&n) { self.cells() } else { self.store(n) }
    }
    fn is_substance(&self, n: &str) -> bool {
        self.w.decls.iter().any(|d| matches!(&d.node, WDecl::Substance { name, .. } if name == n))
    }
    fn is_field(&self, n: &str) -> bool {
        self.w.decls.iter().any(|d| matches!(&d.node, WDecl::Field { name, .. } if name == n))
    }
    /// Whether every name is one this analysis knows: a declaration of the world or a built-in.
    fn known(&self, names: &[String]) -> bool {
        names.iter().all(|n| BUILTINS.contains(&n.as_str()) || NEIGHBOURS.contains(&n.as_str()) || self.w.decls.iter().any(|d| crate::check::declared(&d.node).is_some_and(|(x, _)| x == n)))
    }
}

/// A value-dependency wire: what a condition or a signal statement reads (`into` the box) or writes (out of it).
fn dep_wire(from: String, to: String, substance: Option<String>, write: bool) -> (Wire, WireContract) {
    let why = if write { "a signal statement writes its field's value; change it in the statement's text" } else { "read on the start-of-tick snapshot by a condition or an expression; change it in the text" };
    (Wire { from, to, substance }, WireContract { kind: "value dependency", from_port: if write { "writes" } else { "out" }, to_port: if write { "in" } else { "reads" }, derived: true, edit: None, why_not: Some(why.into()) })
}

/// The wires of one statement: resource wires for a call (as `graph::graph` draws them for a top-level one), value
/// dependencies for what a signal statement reads and writes, and for what its own condition reads, if it is a `when`.
fn stmt_wires(st: &Stmt, id: &str, cx: &Cx, nested: bool, out: &mut Vec<(Wire, WireContract)>) {
    let flow = |from: String, to: String, substance: Option<String>, edit: Option<&str>, why: Option<&str>, ports: (&'static str, &'static str)| {
        (Wire { from, to, substance }, WireContract { kind: if ports.1 == "reaction" { "declaration reference" } else { "resource flow" }, from_port: ports.0, to_port: ports.1, derived: edit.is_none(), edit: edit.map(String::from), why_not: why.map(String::from) })
    };
    match st {
        Stmt::Call { call, .. } if nested && call.name == "transfer" => {
            let s = call.args.iter().find(|a| a.name.is_none()).and_then(|a| match &a.value { Value::Expr(Expr::Path(p)) if p.segs.is_empty() => Some(p.root.clone()), _ => None });
            let name = |k: &str| arg(call, k).and_then(|v| match v { Value::Expr(Expr::Path(p)) if p.segs.is_empty() => Some(p.root.clone()), _ => None });
            if let Some(f) = name("from").and_then(|n| cx.target(&n)) {
                out.push(flow(f, id.to_string(), s.clone(), Some("sets `from` of the transfer"), None, ("out", "from")));
            }
            if let Some(t) = name("to").and_then(|n| cx.target(&n)) {
                out.push(flow(id.to_string(), t, s, Some("sets `to` of the transfer"), None, ("to", "in")));
            }
        }
        Stmt::Call { call, .. } if nested && call.name == "react" => {
            let r = call.args.iter().find(|a| a.name.is_none()).and_then(|a| match &a.value { Value::Expr(Expr::Path(p)) if p.segs.is_empty() => Some(p.root.clone()), _ => None });
            if let Some(r) = r {
                out.push(flow(format!("{}.reactions.{r}", cx.world), id.to_string(), None, Some("sets the reaction this `react` runs"), None, ("use", "reaction")));
                if let (Some(cells), Some((ins, outs))) = (cx.cells(), cx.formulas.get(&r)) {
                    for t in ins {
                        out.push(flow(cells.clone(), id.to_string(), Some(t.clone()), None, Some("an input of the reaction's formula"), ("out", "cells")));
                    }
                    for t in outs {
                        out.push(flow(id.to_string(), cells.clone(), Some(t.clone()), None, Some("an output of the reaction's formula"), ("products", "in")));
                    }
                }
            }
        }
        Stmt::Call { call, .. } if nested && call.name == "diffuse_conserved" => {
            if let Some(cells) = cx.cells() {
                for s in call.args.iter().find(|a| a.name.is_none()).map(|a| names(Some(&a.value))).unwrap_or_default() {
                    out.push(flow(cells.clone(), id.to_string(), Some(s.clone()), None, Some("a substance the diffusion spreads"), ("out", "cells")));
                    out.push(flow(id.to_string(), cells.clone(), Some(s), None, Some("a substance the diffusion spreads"), ("products", "in")));
                }
            }
        }
        Stmt::Assign { .. } | Stmt::Call { .. } if matches!(st, Stmt::Assign { .. }) || matches!(st, Stmt::Call { call, .. } if call.name == "life_like") => {
            // A signal statement writes its field's value and reads the fields its expression names (`life_like` reads
            // its own field's neighbours).
            let mut read = Vec::new();
            match value_of(st) {
                Some(v) => idents(v, &mut read),
                None => read.push(cx.on.to_string()),
            }
            read.sort();
            read.dedup();
            for f in read.iter().filter(|n| cx.is_field(n)) {
                if let Some(sid) = cx.store(f) {
                    out.push(dep_wire(sid, id.to_string(), None, false));
                }
            }
            if let Some(cells) = cx.cells() {
                out.push(dep_wire(id.to_string(), cells, None, true));
            }
        }
        Stmt::When { cond, .. } => {
            let mut c = Vec::new();
            idents(cond, &mut c);
            c.sort();
            c.dedup();
            if let Some(cells) = cx.cells() {
                for n in c.iter().filter(|n| cx.is_substance(n)) {
                    out.push(dep_wire(cells.clone(), id.to_string(), Some(n.clone()), false));
                }
            }
        }
        _ => {}
    }
}

fn value_of(st: &Stmt) -> Option<&Expr> {
    match st {
        Stmt::Assign { value, .. } => Some(value),
        _ => None,
    }
}

/// Dependencies of `st`, a box for each statement nested in it, and the wires of those boxes.
#[allow(clippy::too_many_arguments)]
fn walk(st: &Stmt, cx: &Cx, deps: &mut Deps, when: &str, container: &str, nested: &mut Vec<Nested>, path: &[String], wires: &mut Vec<(Wire, WireContract)>) {
    let (on, formulas, rule) = (cx.on, cx.formulas, cx.rule);
    match st {
        Stmt::Call { call, .. } if call.name == "life_like" => {
            deps.reads.push(on.to_string());
            deps.writes.push(on.to_string());
            deps.references.push("life_like".into());
        }
        Stmt::Call { call, .. } => call_deps(call, formulas, on, deps),
        Stmt::When { cond, then, otherwise } => {
            let mut c = Vec::new();
            idents(cond, &mut c);
            // A name the condition uses that is neither declared nor built in (a `let`) is not followed.
            deps.complete &= cx.known(&c);
            deps.conditions.extend(c);
            let text = value_text(&Value::Expr(cond.clone()));
            let join = |extra: String| if when.is_empty() { extra } else { format!("{when} and {extra}") };
            for (tag, block, w) in [("t", Some(then), join(text.clone())), ("e", otherwise.as_ref(), join(format!("not ({text})")))] {
                let Some(block) = block else { continue };
                for (i, s) in block.iter().enumerate() {
                    let mut p = path.to_vec();
                    p.push(format!("{tag}{i}"));
                    let id = format!("{container}/{}", p.join("."));
                    let (kind, title, call_name) = kind_of(&s.node);
                    let mut d = Deps { complete: true, ..Default::default() };
                    let mut inner = Vec::new();
                    walk(&s.node, cx, &mut d, &w, container, &mut inner, &p, wires);
                    stmt_wires(&s.node, &id, cx, true, wires);
                    // What the condition reads is read by everything under it; what the statements inside read and write,
                    // the `when` reads and writes.
                    deps.conditions.extend(d.conditions.iter().cloned());
                    deps.reads.extend(d.reads.iter().cloned());
                    deps.writes.extend(d.writes.iter().cloned());
                    deps.references.extend(d.references.iter().cloned());
                    deps.complete &= d.complete;
                    let params: Vec<(String, String)> = match &s.node {
                        Stmt::Call { call, .. } => call.args.iter().filter_map(|a| a.name.as_ref().map(|n| (n.clone(), value_text(&a.value)))).collect(),
                        _ => Vec::new(),
                    };
                    let call_like = matches!(kind, Kind::Transfer | Kind::React | Kind::Diffuse);
                    let mut ports = Vec::new();
                    let mut caps = Caps { edit_text: true, set_param: call_like, delete: true, rewire: matches!(kind, Kind::Transfer | Kind::React), ..Default::default() };
                    match kind {
                        Kind::Transfer => {
                            ports.push(Port { key: "from", name: "from", dir: "in", role: "resource", ty: "store", required: true, many: false });
                            ports.push(Port { key: "to", name: "to", dir: "out", role: "resource", ty: "store", required: true, many: false });
                        }
                        Kind::React => {
                            ports.push(Port { key: "reaction", name: "reaction", dir: "in", role: "reference", ty: "reaction", required: true, many: false });
                            ports.push(Port { key: "cells", name: "cells (inputs)", dir: "in", role: "resource", ty: "store", required: false, many: true });
                            ports.push(Port { key: "products", name: "cells (outputs)", dir: "out", role: "resource", ty: "store", required: false, many: true });
                            caps.why_not.insert("rewire_resources", "the inputs and outputs come from the reaction's formula; change the reaction or edit its formula".into());
                        }
                        Kind::Diffuse => {
                            ports.push(Port { key: "cells", name: "cells (in)", dir: "in", role: "resource", ty: "store", required: false, many: true });
                            ports.push(Port { key: "products", name: "cells (out)", dir: "out", role: "resource", ty: "store", required: false, many: true });
                            caps.why_not.insert("rewire", "a diffusion spreads between a field's own cells; its substances are in its text".into());
                        }
                        _ => {
                            if matches!(s.node, Stmt::When { .. }) {
                                ports.push(Port { key: "reads", name: "reads", dir: "in", role: "value", ty: "store", required: false, many: true });
                            }
                            caps.why_not.insert("rewire", "its dependencies are drawn as value wires; edit its text".into());
                        }
                    }
                    caps.why_not.insert("duplicate", "copy the whole `when`".into());
                    caps.why_not.insert("label", "labels name top-level statements".into());
                    if !call_like {
                        caps.why_not.insert("set_param", "this statement has no named arguments to set; edit its text".into());
                    }
                    let children = inner.iter().filter(|x| x.id.strip_prefix(&format!("{id}.")).is_some_and(|r| !r.contains('/') && !r.contains('.'))).map(|x| x.id.clone()).collect();
                    let condition = match &s.node { Stmt::When { cond, .. } => Some(value_text(&Value::Expr(cond.clone()))), _ => None };
                    for list in [&mut d.conditions, &mut d.reads, &mut d.writes, &mut d.references] {
                        list.sort();
                        list.dedup();
                    }
                    nested.push(Nested {
                        id,
                        container: container.to_string(),
                        group: container.rsplit_once(['@', '#']).map(|(r, _)| r.to_string()).unwrap_or_else(|| container.to_string()),
                        kind,
                        title: title.to_string(),
                        text: stmt_text(&s.node),
                        when: w.clone(),
                        meta: NodeContract {
                            context: format!("rule:{rule}"),
                            identity: "position",
                            ports,
                            params: params.iter().map(|(k, v)| param_meta(call_name, k, v)).collect(),
                            caps,
                            deps: d,
                            summary: None,
                            children,
                            condition,
                        },
                        params,
                    });
                    nested.extend(inner);
                }
            }
        }
        Stmt::For { body, .. } | Stmt::Each { body, .. } => {
            deps.complete = false;
            for s in body {
                walk(&s.node, cx, deps, when, container, nested, path, wires);
            }
        }
        Stmt::Assign { value, .. } => {
            // A signal statement: it writes its field's value; its expression reads the fields and amounts it names.
            let mut read = Vec::new();
            idents(value, &mut read);
            deps.complete &= cx.known(&read);
            deps.writes.push(on.to_string());
            deps.reads.extend(read.into_iter().filter(|n| cx.is_field(n) || cx.is_substance(n)));
        }
        Stmt::Let { .. } | Stmt::Setting { .. } => deps.complete = false,
    }
}

fn caps_of(n: &GNode, ports: &mut Vec<Port>) -> Caps {
    let label = !n.id.contains('@');
    let declaration_only = "declarations cannot be replaced, removed or copied from the graph yet";
    let mut caps = Caps::default();
    match n.kind {
        Kind::Transfer => {
            ports.push(Port { key: "from", name: "from", dir: "in", role: "resource", ty: "store", required: true, many: false });
            ports.push(Port { key: "to", name: "to", dir: "out", role: "resource", ty: "store", required: true, many: false });
            caps = Caps { edit_text: true, set_param: true, delete: true, rewire: true, duplicate: true, label, ..Default::default() };
        }
        Kind::React => {
            ports.push(Port { key: "reaction", name: "reaction", dir: "in", role: "reference", ty: "reaction", required: true, many: false });
            ports.push(Port { key: "cells", name: "cells (inputs)", dir: "in", role: "resource", ty: "store", required: false, many: true });
            ports.push(Port { key: "products", name: "cells (outputs)", dir: "out", role: "resource", ty: "store", required: false, many: true });
            caps = Caps { edit_text: true, set_param: true, delete: true, rewire: true, duplicate: true, label, ..Default::default() };
            caps.why_not.insert("rewire_resources", "the inputs and outputs come from the reaction's formula; change the reaction or edit its formula".into());
        }
        Kind::Diffuse => {
            ports.push(Port { key: "cells", name: "cells (in)", dir: "in", role: "resource", ty: "store", required: false, many: true });
            ports.push(Port { key: "products", name: "cells (out)", dir: "out", role: "resource", ty: "store", required: false, many: true });
            caps = Caps { edit_text: true, set_param: true, delete: true, rewire: false, duplicate: true, label, ..Default::default() };
            caps.why_not.insert("rewire", "a diffusion spreads between a field's own cells; its substances are in its text".into());
        }
        Kind::Signal | Kind::Code => {
            caps = Caps { edit_text: true, set_param: false, delete: true, rewire: false, duplicate: true, label, ..Default::default() };
            caps.why_not.insert("set_param", "this statement has no named arguments to set; edit its text".into());
            caps.why_not.insert("rewire", "what it reads and writes is drawn as value wires; change them in its text".into());
        }
        Kind::Store => {
            ports.push(Port { key: "in", name: "in", dir: "in", role: "resource", ty: "store", required: false, many: true });
            ports.push(Port { key: "out", name: "out", dir: "out", role: "resource", ty: "store", required: false, many: true });
            caps.set_param = !n.params.is_empty();
            for k in ["edit_text", "delete", "duplicate", "rewire"] {
                caps.why_not.insert(k, declaration_only.into());
            }
        }
        Kind::Reaction => {
            ports.push(Port { key: "use", name: "used by", dir: "out", role: "reference", ty: "reaction", required: false, many: true });
            caps.set_param = !n.params.is_empty();
            for k in ["edit_text", "delete", "duplicate", "rewire"] {
                caps.why_not.insert(k, declaration_only.into());
            }
        }
        Kind::Rule => {
            caps.why_not.insert("delete", "delete its statements; a rule with none is gone".into());
            caps.why_not.insert("edit_text", "edit its statements, one at a time".into());
            caps.why_not.insert("rewire", "wire its statements".into());
            caps.why_not.insert("duplicate", "copy its statements".into());
        }
        Kind::Decl => {
            caps.set_param = !n.params.is_empty();
            for k in ["edit_text", "delete", "duplicate", "rewire"] {
                caps.why_not.insert(k, declaration_only.into());
            }
        }
    }
    if !caps.set_param && !caps.why_not.contains_key("set_param") {
        caps.why_not.insert("set_param", "nothing here has a value to set".into());
    }
    if !caps.label {
        caps.why_not.insert("label", if n.group.is_some() { "it already has a label".into() } else { "only statements are labelled".into() });
    }
    caps
}

/// The contract of the `world` of `file`.
pub fn contract(file: &File, world: &str) -> Result<Contract, String> {
    let g: Graph = graph(file, world)?;
    let w = file
        .items
        .iter()
        .find_map(|i| match &i.node {
            Item::World(w) if w.name == world => Some(w),
            _ => None,
        })
        .ok_or_else(|| format!("no world `{world}`"))?;
    let mut formulas = Formulas::new();
    for d in &w.decls {
        if let WDecl::Reaction { name, formula, .. } = &d.node {
            formulas.insert(name.clone(), (formula.inputs.iter().map(|t| t.name.clone()).collect(), formula.outputs.iter().map(|t| t.name.clone()).collect()));
        }
    }
    let mut contexts = vec![("world".to_string(), world.to_string())];
    for d in &w.decls {
        if let WDecl::Rule { name, .. } = &d.node {
            contexts.push((format!("rule:{name}"), format!("{world} / rules / {name}")));
        }
    }
    let mut nodes = BTreeMap::new();
    let mut nested = Vec::new();
    let mut more_wires = Vec::new();
    for n in &g.nodes {
        let rule_name = n.group.as_ref().map(|r| r.rsplit('.').next().unwrap_or("").to_string());
        let context = rule_name.as_ref().map_or("world".to_string(), |r| format!("rule:{r}"));
        let mut ports = Vec::new();
        let mut caps = caps_of(n, &mut ports);
        if n.kind == Kind::Decl && n.id.contains(".waste.") {
            ports.push(Port { key: "out", name: "routes to", dir: "out", role: "resource", ty: "store", required: true, many: false });
        }
        let call_name = match n.kind {
            Kind::Transfer => "transfer",
            Kind::React => "react",
            Kind::Diffuse => "diffuse_conserved",
            _ => "",
        };
        let params = n.params.iter().map(|(k, v)| param_meta(call_name, k, v)).collect();
        let mut deps = Deps { complete: !matches!(n.kind, Kind::Signal | Kind::Code), ..Default::default() };
        let mut children = Vec::new();
        let mut condition = None;
        // The statement's syntax: its dependencies, the statements nested in it, and the wires they have.
        if let Some(rule) = &rule_name {
            if let Some(WDecl::Rule { on, body, .. }) = w.decls.iter().map(|d| &d.node).find(|d| matches!(d, WDecl::Rule { name, .. } if name == rule)) {
                let at = n.id.rsplit_once('#').and_then(|(_, i)| i.parse::<usize>().ok()).or_else(|| body.iter().position(|s| matches!(&s.node, Stmt::Call { label: Some(l), .. } if n.id.ends_with(&format!("@{l}")))));
                if let Some(st) = at.and_then(|i| body.get(i)) {
                    let cx = Cx { world, w, rule, on, formulas: &formulas };
                    // A `when` or a signal statement is followed through; the walk says where it is not.
                    if matches!(&st.node, Stmt::When { .. } | Stmt::Assign { .. }) || matches!(&st.node, Stmt::Call { call, .. } if call.name == "life_like") {
                        deps.complete = true;
                    }
                    let before = nested.len();
                    walk(&st.node, &cx, &mut deps, "", &n.id, &mut nested, &[], &mut more_wires);
                    stmt_wires(&st.node, &n.id, &cx, false, &mut more_wires);
                    children = nested[before..].iter().filter(|x| !x.id[n.id.len() + 1..].contains('.')).map(|x| x.id.clone()).collect();
                    match &st.node {
                        Stmt::When { cond, .. } => {
                            caps.why_not.insert("set_param", "a `when` has no arguments; set its condition, or the statements inside it".into());
                            ports.push(Port { key: "reads", name: "reads", dir: "in", role: "value", ty: "store", required: false, many: true });
                            condition = Some(value_text(&Value::Expr(cond.clone())));
                        }
                        Stmt::Assign { .. } | Stmt::Call { .. } if n.kind == Kind::Signal => {
                            ports.push(Port { key: "reads", name: "reads", dir: "in", role: "value", ty: "store", required: false, many: true });
                            ports.push(Port { key: "writes", name: "writes", dir: "out", role: "value", ty: "store", required: true, many: false });
                        }
                        _ => {}
                    }
                }
            }
        }
        let summary = (n.kind == Kind::Rule).then(|| {
            let mine: Vec<&GNode> = g.nodes.iter().filter(|x| x.group.as_deref() == Some(&n.id)).collect();
            let on = w.decls.iter().find_map(|d| match &d.node {
                WDecl::Rule { name, on, .. } if Some(name) == n.id.rsplit('.').next().map(str::to_string).as_ref() => Some(on.clone()),
                _ => None,
            });
            let mut kinds: Vec<String> = mine.iter().map(|x| x.kind.as_str().to_string()).collect();
            kinds.sort();
            kinds.dedup();
            let mut steps: Vec<String> = mine.iter().filter_map(|x| x.step.map(String::from)).collect();
            steps.dedup();
            let count = nested.iter().filter(|x| mine.iter().any(|m| m.id == x.container)).count();
            Summary { on: on.unwrap_or_default(), statements: mine.len(), nested: count, kinds, steps }
        });
        let identity = if n.id.contains('@') || n.group.is_none() { "label" } else { "position" };
        for list in [&mut deps.conditions, &mut deps.reads, &mut deps.writes, &mut deps.references] {
            list.sort();
            list.dedup();
        }
        nodes.insert(n.id.clone(), NodeContract { context, identity, ports, params, caps, deps, summary, children, condition });
    }
    let kind = |id: &str| g.nodes.iter().find(|n| n.id == id).map(|n| n.kind);
    let wires = g
        .wires
        .iter()
        .map(|wire| match (kind(&wire.from), kind(&wire.to)) {
            (Some(Kind::Reaction), Some(Kind::React)) => WireContract { kind: "declaration reference", from_port: "use", to_port: "reaction", derived: false, edit: Some("sets the reaction this `react` runs".into()), why_not: None },
            (Some(Kind::Store), Some(Kind::Transfer)) => WireContract { kind: "resource flow", from_port: "out", to_port: "from", derived: false, edit: Some("sets `from` of the transfer".into()), why_not: None },
            (Some(Kind::Transfer), Some(Kind::Store)) => WireContract { kind: "resource flow", from_port: "to", to_port: "in", derived: false, edit: Some("sets `to` of the transfer".into()), why_not: None },
            (Some(Kind::Store), Some(Kind::React | Kind::Diffuse)) => WireContract { kind: "resource flow", from_port: "out", to_port: "cells", derived: true, edit: None, why_not: Some("an input of the reaction's formula (or a substance the diffusion spreads), not an argument of its own".into()) },
            (Some(Kind::React | Kind::Diffuse), Some(Kind::Store)) => WireContract { kind: "resource flow", from_port: "products", to_port: "in", derived: true, edit: None, why_not: Some("an output of the reaction's formula (or a substance the diffusion spreads), not an argument of its own".into()) },
            _ => WireContract { kind: "resource flow", from_port: "out", to_port: "in", derived: true, edit: None, why_not: Some("a declared route (waste), changed in its declaration".into()) },
        })
        .collect();
    Ok(Contract { nodes, wires, more_wires, nested, contexts })
}

/// A name the code box offers as a completion (`proposals/houdini-node-system.md` §8): what it is, where it is declared
/// (the box to jump to), and a line of help. From the parsed source, so it is what the checker will resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub name: String,
    /// `substance`, `field`, `source`, `sink`, `reaction`, `const`, `element`, `rule`, `function`, `direction`, `keyword`.
    pub kind: &'static str,
    /// The box that declares it, for jump-to-definition.
    pub id: Option<String>,
    pub line: usize,
    pub detail: String,
}

/// The world-rule built-ins with their call shape (`language.md` §12), for completion and help.
pub const RULE_FUNCTIONS: [(&str, &str); 20] = [
    ("transfer", "transfer(SUBSTANCE, from: SOURCE|self, to: FIELD|self|above|right|below|left, amount: A, overflow: SINK): an effect of a transfer rule"),
    ("react", "react(REACTION, rate: Nbp): an effect of a reaction rule"),
    ("diffuse_conserved", "diffuse_conserved([SUBSTANCES], rate: Nbp): spread between neighbouring cells"),
    ("sum", "sum(COLLECTION): over moore.S or vonneumann.S"),
    ("min", "min(COLLECTION)"),
    ("max", "max(COLLECTION)"),
    ("mean", "mean(COLLECTION)"),
    ("count", "count(CONDITION over a collection): an int"),
    ("argmin", "argmin(COLLECTION): the neighbour with the smallest value"),
    ("argmax", "argmax(COLLECTION): the neighbour with the largest value"),
    ("cap", "cap(SUBSTANCE): the rule's field cap for it"),
    ("select", "select(when: C, then: A, else: B)"),
    ("blur", "blur(FIELD, amount: f32): signal fields"),
    ("fade", "fade(VALUE, keep: f32)"),
    ("abs", "abs(f32)"),
    ("sqrt", "sqrt(f32)"),
    ("units", "units(AMOUNT): a number"),
    ("amount", "amount(SUBSTANCE, units: f32)"),
    ("float", "float(INT)"),
    ("life_like", "life_like(born: [N…], survives: [N…]): a state field's rule"),
];

/// Every name a statement of `world` may use, declared or built in.
pub fn scope_names(file: &File, world: &str) -> Result<Vec<Name>, String> {
    let w = file.items.iter().find_map(|i| match &i.node { Item::World(w) if w.name == world => Some(w), _ => None }).ok_or_else(|| format!("no world `{world}`"))?;
    let mut out = Vec::new();
    for d in &w.decls {
        let line = d.pos.line;
        let mut push = |name: &str, kind: &'static str, plural: &str, detail: String| out.push(Name { name: name.to_string(), kind, id: Some(format!("{world}.{plural}.{name}")), line, detail });
        match &d.node {
            WDecl::Substance { name, .. } => push(name, "substance", "substances", crate::fmt::decl_text(&d.node)),
            WDecl::Field { name, holds, .. } => push(name, "field", "fields", if holds.is_some() { "a field that holds substances".into() } else { "a signal or state field".into() }),
            WDecl::Source { name, emits } => push(name, "source", "sources", format!("emits {}", emits.join(", "))),
            WDecl::Sink { name, .. } => push(name, "sink", "sinks", crate::fmt::decl_text(&d.node)),
            WDecl::Reaction { name, formula, .. } => push(name, "reaction", "reactions", crate::fmt::formula_text(formula)),
            WDecl::Const { name, .. } => push(name, "const", "consts", crate::fmt::decl_text(&d.node)),
            WDecl::Element(name) => push(name, "element", "elements", "an element".into()),
            WDecl::Rule { name, on, .. } => push(name, "rule", "rules", format!("a rule on {on}")),
            _ => {}
        }
    }
    for (n, d) in RULE_FUNCTIONS {
        out.push(Name { name: n.to_string(), kind: "function", id: None, line: 0, detail: d.to_string() });
    }
    for (n, d) in [("self", "the cell the rule works on"), ("above", "the neighbour above"), ("below", "the neighbour below"), ("left", "the neighbour to the left"), ("right", "the neighbour to the right"), ("moore", "the 8 neighbours, as a collection"), ("vonneumann", "the 4 neighbours, as a collection"), ("tick", "the current tick, an int")] {
        out.push(Name { name: n.to_string(), kind: "direction", id: None, line: 0, detail: d.to_string() });
    }
    for (n, d) in [("when", "when CONDITION { … } else { … }"), ("else", "the other branch of a when"), ("let", "let NAME = EXPR;"), ("for", "for NAME in [a, b] { … }: written out once for each name"), ("and", "both"), ("or", "either"), ("not", "the opposite")] {
        out.push(Name { name: n.to_string(), kind: "keyword", id: None, line: 0, detail: d.to_string() });
    }
    Ok(out)
}
