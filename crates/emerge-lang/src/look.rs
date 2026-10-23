//! The Look network (`proposals/houdini-node-system.md` §11, H5): a `view` as a typed dataflow of its own syntax.
//!
//! ```text
//! field(cells.nitrate) → read value → scale(log, …) → palette(greens) → color → layer ground ┐
//! agents(where: …)     → read lineage_id → palette(categorical) → color → layer life ───────┴→ composite
//! ```
//!
//! Every node is a piece of the view's text: a layer's source, the head of an entry (a name it reads, or an expression),
//! each `|>` stage of the entry, the entry, the layer's body, the layer, and the composite of all layers in their order.
//! Each node's output has the type the view checker gives that piece (`view::expr_type`), so a socket says what flows
//! through it. Edits (`LookOp`) change the view's syntax tree and print it with the formatter; whoever applies one checks
//! the result before keeping it, so a refused edit leaves the last valid view. A node can be shown on its own
//! (`derived`): the view cut at that node, with a neutral palette for a value that is not yet a colour. Nothing here
//! reads or changes a world: a view is presentation.

use crate::ast::*;
use crate::fmt::value_text;
use crate::view::{expr_type, source_kind};

#[derive(Clone, Debug, PartialEq)]
pub struct LNode {
    /// `composite`, `layer:NAME`, `layer:NAME/source`, `layer:NAME/body`, `layer:NAME/ENTRY` (the entry) and
    /// `layer:NAME/ENTRY/K` (K = 0 is the head of the entry's chain, K ≥ 1 its K-th `|>` stage).
    pub id: String,
    /// `source`, `read`, `expression`, `stage`, `entry`, `body`, `layer`, `composite`.
    pub kind: &'static str,
    pub title: String,
    /// The node's own source text: the call of a stage, the head expression, the entry's whole value.
    pub text: String,
    /// The type of what it gives (`number`, `scaled`, `colour`, `identity`, …; `cells` or `agents` for a source).
    pub out: &'static str,
    pub layer: Option<String>,
    /// For a stage: its arguments other than the subject, as source text (`kind` for the word after the subject).
    pub params: Vec<(String, String)>,
    /// Problems the checker finds in this piece, in words.
    pub problems: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LWire {
    pub from: String,
    pub to: String,
    pub ty: &'static str,
}

#[derive(Clone, Debug)]
pub struct Pipeline {
    pub view: String,
    pub world: String,
    pub nodes: Vec<LNode>,
    pub wires: Vec<LWire>,
}

/// The `|>` chain of an expression: its head and the calls applied to it in order.
fn chain(e: &Expr) -> (Expr, Vec<Call>) {
    match e {
        Expr::Pipe(x, c) => {
            let (h, mut cs) = chain(x);
            cs.push(c.clone());
            (h, cs)
        }
        other => (other.clone(), Vec::new()),
    }
}

/// The chain back together, up to and including stage `k` (0: the head alone).
fn prefix(head: &Expr, calls: &[Call], k: usize) -> Expr {
    calls[..k].iter().fold(head.clone(), |acc, c| Expr::Pipe(Box::new(acc), c.clone()))
}

fn expr_text(e: &Expr) -> String {
    value_text(&Value::Expr(e.clone()))
}

fn call_text(c: &Call) -> String {
    expr_text(&Expr::Call(c.clone()))
}

/// The arguments of a piped call, other than its subject: the first unnamed one is `kind` (`scale(log, …)`,
/// `palette(greens)`), named ones by name.
fn stage_params(c: &Call) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut first = true;
    for a in &c.args {
        match &a.name {
            Some(n) => out.push((n.clone(), value_text(&a.value))),
            None if first => {
                first = false;
                out.push(("kind".to_string(), value_text(&a.value)));
            }
            None => out.push(("value".to_string(), value_text(&a.value))),
        }
    }
    out
}

fn find_view<'a>(file: &'a File, name: &str) -> Option<&'a View> {
    file.items.iter().rev().find_map(|i| match &i.node {
        Item::View(v) if v.name == name => Some(v),
        _ => None,
    })
}

fn find_world<'a>(file: &'a File, name: &str) -> Option<&'a World> {
    file.items.iter().find_map(|i| match &i.node {
        Item::World(w) if w.name == name => Some(w),
        _ => None,
    })
}

/// The network of view `view` in `file` (the world's source with the view documents after it).
pub fn pipeline(file: &File, view: &str) -> Result<Pipeline, String> {
    let v = find_view(file, view).ok_or_else(|| format!("no view `{view}`"))?;
    let world = find_world(file, &v.world).ok_or_else(|| format!("view `{view}` is for `{}`, which is not here", v.world))?;
    let (mut nodes, mut wires) = (Vec::new(), Vec::new());
    let problems = |p: Vec<crate::check::Problem>| p.into_iter().map(|x| format!("{} {}", x.code, x.message)).collect::<Vec<_>>();
    for layer in &v.layers {
        let l = &layer.node;
        let lid = format!("layer:{}", l.name);
        let src_kind = source_kind(l);
        let sid = format!("{lid}/source");
        nodes.push(LNode { id: sid.clone(), kind: "source", title: l.source.name.clone(), text: call_text(&l.source), out: src_kind, layer: Some(l.name.clone()), params: Vec::new(), problems: Vec::new() });
        for e in &l.entries {
            let key = e.node.key.as_str();
            let eid = format!("{lid}/{key}");
            let Value::Expr(x) = &e.node.value else { continue };
            if key == "blend" {
                // A blend mode is a setting of the layer, not data: shown on the layer node.
                continue;
            }
            let (head, calls) = chain(x);
            let (ht, hp) = expr_type(world, l, &head);
            let is_read = matches!(&head, Expr::Path(p) if p.segs.is_empty());
            nodes.push(LNode {
                id: format!("{eid}/0"),
                kind: if is_read { "read" } else { "expression" },
                title: if is_read { format!("read {}", expr_text(&head)) } else { "expression".to_string() },
                text: expr_text(&head),
                out: ht,
                layer: Some(l.name.clone()),
                params: Vec::new(),
                problems: problems(hp),
            });
            wires.push(LWire { from: sid.clone(), to: format!("{eid}/0"), ty: src_kind });
            let mut prev = (format!("{eid}/0"), ht);
            for (k, c) in calls.iter().enumerate() {
                let (t, p) = expr_type(world, l, &prefix(&head, &calls, k + 1));
                let id = format!("{eid}/{}", k + 1);
                nodes.push(LNode { id: id.clone(), kind: "stage", title: c.name.clone(), text: call_text(c), out: t, layer: Some(l.name.clone()), params: stage_params(c), problems: problems(p) });
                wires.push(LWire { from: prev.0.clone(), to: id.clone(), ty: prev.1 });
                prev = (id, t);
            }
            nodes.push(LNode { id: eid.clone(), kind: "entry", title: key.to_string(), text: expr_text(x), out: prev.1, layer: Some(l.name.clone()), params: Vec::new(), problems: Vec::new() });
            wires.push(LWire { from: prev.0, to: eid.clone(), ty: prev.1 });
            wires.push(LWire { from: eid, to: lid.clone(), ty: prev.1 });
        }
        if !l.body.is_empty() {
            let text = l.body.iter().map(|s| vstmt_text(&s.node)).collect::<Vec<_>>().join("\n");
            nodes.push(LNode { id: format!("{lid}/body"), kind: "body", title: "body".to_string(), text, out: "attributes", layer: Some(l.name.clone()), params: Vec::new(), problems: Vec::new() });
            wires.push(LWire { from: sid.clone(), to: format!("{lid}/body"), ty: src_kind });
            wires.push(LWire { from: format!("{lid}/body"), to: lid.clone(), ty: "attributes" });
        }
        let blend = l.entries.iter().filter(|e| e.node.key == "blend" || e.node.key == "opacity").map(|e| (e.node.key.clone(), value_text(&e.node.value))).collect();
        nodes.push(LNode { id: lid.clone(), kind: "layer", title: format!("layer {}", l.name), text: format!("{} = {}", l.name, call_text(&l.source)), out: "picture", layer: Some(l.name.clone()), params: blend, problems: Vec::new() });
        wires.push(LWire { from: lid, to: "composite".to_string(), ty: "picture" });
    }
    nodes.push(LNode { id: "composite".to_string(), kind: "composite", title: format!("view {}", v.name), text: format!("{} layers, the last on top", v.layers.len()), out: "picture", layer: None, params: Vec::new(), problems: Vec::new() });
    Ok(Pipeline { view: v.name.clone(), world: v.world.clone(), nodes, wires })
}

/// A layer body's statement as text (the formatter's view of it).
fn vstmt_text(s: &VStmt) -> String {
    let file = File { items: vec![Node { trivia: Trivia::default(), pos: Pos::default(), node: Item::View(View { name: "T".into(), world: "W".into(), layers: vec![Node { trivia: Trivia::default(), pos: Pos::default(), node: Layer { name: "l".into(), source: Call { name: "field".into(), args: vec![] }, entries: vec![], body: vec![Node { trivia: Trivia::default(), pos: Pos::default(), node: s.clone() }] } }] }) }], tail: vec![] };
    let text = crate::format(&file);
    // The statement's lines between the layer's braces, without their indentation.
    let lines: Vec<&str> = text.lines().collect();
    lines.iter().skip(2).take(lines.len().saturating_sub(4)).map(|l| l.trim_start()).collect::<Vec<_>>().join(" ")
}

/// An edit of a view (`proposals/houdini-node-system.md` §11). Texts are source text; the result is checked by whoever
/// applies it.
#[derive(Clone, Debug)]
pub enum LookOp {
    /// Replace stage `index` of entry `entry` (0: the head expression; K ≥ 1: the K-th `|>` call, `scale(log, range: …)`).
    SetStage { layer: String, entry: String, index: usize, text: String },
    /// A call put into the chain after position `after` (0: right after the head).
    InsertStage { layer: String, entry: String, after: usize, text: String },
    /// Take stage `index` (≥ 1) out of the chain.
    RemoveStage { layer: String, entry: String, index: usize },
    /// Add, replace or (with no text) remove an entry: `color`, `size`, `glyph`, `opacity`, `blend`.
    SetEntry { layer: String, key: String, text: Option<String> },
    /// The layer's body statements, as text (empty removes the body).
    SetBody { layer: String, text: String },
    /// The layer's source: `field(cells.nitrate)`, `agents(where: …)`, `context(cells)`.
    SetSource { layer: String, text: String },
    /// A new layer at the top, with a colour.
    AddLayer { name: String, source: String, color: String },
    RemoveLayer { layer: String },
    /// One place up (drawn later, so over the next) or down.
    MoveLayer { layer: String, up: bool },
}

/// A one-layer view from text, to take pieces of: `view T for W { layer l = SRC { BODY } }`.
fn parse_layer(src: &str, inner: &str) -> Result<Layer, String> {
    let text = format!("view T for W {{ layer l = {src} {{ {inner} }} }}");
    let file = crate::parse(&text).map_err(|d| format!("{}: {}", d.code, d.message))?;
    match file.items.first().map(|i| &i.node) {
        Some(Item::View(v)) => v.layers.first().map(|l| l.node.clone()).ok_or_else(|| "no layer".to_string()),
        _ => Err("not a view".to_string()),
    }
}

/// An expression as an entry holds it.
fn parse_expr(text: &str) -> Result<Expr, String> {
    let l = parse_layer("field(f)", &format!("color: {text};"))?;
    match l.entries.into_iter().next().map(|e| e.node.value) {
        Some(Value::Expr(e)) => Ok(e),
        _ => Err("not an expression".to_string()),
    }
}

fn parse_call(text: &str) -> Result<Call, String> {
    match parse_expr(text)? {
        Expr::Call(c) => Ok(c),
        _ => Err(format!("`{text}` is not a call: a stage is a call such as `scale(log, range: 1.0..100.0)`")),
    }
}

fn node<T>(x: T) -> Node<T> {
    Node { trivia: Trivia::default(), pos: Pos::default(), node: x }
}

/// The view after `op`.
pub fn edit_view(v: &View, op: &LookOp) -> Result<View, String> {
    let mut v = v.clone();
    let layer_at = |v: &View, name: &str| v.layers.iter().position(|l| l.node.name == name).ok_or_else(|| format!("no layer `{name}` in view `{}`", v.name));
    match op {
        LookOp::SetStage { layer, entry, index, text } => {
            let at = layer_at(&v, layer)?;
            let e = entry_mut(&mut v.layers[at].node, entry)?;
            let Value::Expr(x) = &e.value else { return Err(format!("`{entry}` is not an expression")) };
            let (mut head, mut calls) = chain(x);
            if *index == 0 {
                head = parse_expr(text)?;
            } else {
                let slot = calls.get_mut(index - 1).ok_or_else(|| format!("`{entry}` has no stage {index}"))?;
                *slot = parse_call(text)?;
            }
            e.value = Value::Expr(prefix(&head, &calls, calls.len()));
        }
        LookOp::InsertStage { layer, entry, after, text } => {
            let at = layer_at(&v, layer)?;
            let e = entry_mut(&mut v.layers[at].node, entry)?;
            let Value::Expr(x) = &e.value else { return Err(format!("`{entry}` is not an expression")) };
            let (head, mut calls) = chain(x);
            if *after > calls.len() {
                return Err(format!("`{entry}` has {} stages; nothing to insert after stage {after}", calls.len()));
            }
            calls.insert(*after, parse_call(text)?);
            e.value = Value::Expr(prefix(&head, &calls, calls.len()));
        }
        LookOp::RemoveStage { layer, entry, index } => {
            let at = layer_at(&v, layer)?;
            let e = entry_mut(&mut v.layers[at].node, entry)?;
            let Value::Expr(x) = &e.value else { return Err(format!("`{entry}` is not an expression")) };
            let (head, mut calls) = chain(x);
            if *index == 0 || *index > calls.len() {
                return Err(format!("`{entry}` has no stage {index} to take out (the head stays: replace it instead)"));
            }
            calls.remove(index - 1);
            e.value = Value::Expr(prefix(&head, &calls, calls.len()));
        }
        LookOp::SetEntry { layer, key, text } => {
            let at = layer_at(&v, layer)?;
            let l = &mut v.layers[at].node;
            match text {
                None => {
                    let before = l.entries.len();
                    l.entries.retain(|e| e.node.key != *key);
                    if l.entries.len() == before {
                        return Err(format!("layer `{layer}` has no `{key}`"));
                    }
                }
                Some(t) => {
                    let parsed = parse_layer("field(f)", &format!("{key}: {t};"))?.entries.into_iter().next().ok_or("no entry")?;
                    match l.entries.iter_mut().find(|e| e.node.key == *key) {
                        Some(e) => e.node.value = parsed.node.value,
                        None => l.entries.push(node(parsed.node)),
                    }
                }
            }
        }
        LookOp::SetBody { layer, text } => {
            let at = layer_at(&v, layer)?;
            v.layers[at].node.body = if text.trim().is_empty() { Vec::new() } else { parse_layer("field(f)", text)?.body };
        }
        LookOp::SetSource { layer, text } => {
            let at = layer_at(&v, layer)?;
            v.layers[at].node.source = parse_layer(text, "")?.source;
        }
        LookOp::AddLayer { name, source, color } => {
            if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
                return Err(format!("`{name}` is not a layer name: letters, digits and `_`"));
            }
            if v.layers.iter().any(|l| l.node.name == *name) {
                return Err(format!("view `{}` already has a layer `{name}`", v.name));
            }
            let mut l = parse_layer(source, &format!("color: {color};"))?;
            l.name = name.clone();
            v.layers.push(node(l));
        }
        LookOp::RemoveLayer { layer } => {
            let at = layer_at(&v, layer)?;
            if v.layers.len() == 1 {
                return Err("a view keeps at least one layer".to_string());
            }
            v.layers.remove(at);
        }
        LookOp::MoveLayer { layer, up } => {
            let at = layer_at(&v, layer)?;
            let to = if *up { at + 1 } else { at.checked_sub(1).ok_or("it is already the bottom layer")? };
            if to >= v.layers.len() {
                return Err("it is already the top layer".to_string());
            }
            v.layers.swap(at, to);
        }
    }
    Ok(v)
}

fn entry_mut<'a>(l: &'a mut Layer, key: &str) -> Result<&'a mut Entry, String> {
    let name = l.name.clone();
    l.entries.iter_mut().find(|e| e.node.key == key).map(|e| &mut e.node).ok_or_else(|| format!("layer `{name}` has no `{key}`"))
}

/// `doc` (a file of views) with `v` in place of the view of the same name, or added at its end; in canonical form.
pub fn put_view(doc: &str, v: &View) -> Result<String, String> {
    let mut file = if doc.trim().is_empty() { File { items: Vec::new(), tail: Vec::new() } } else { crate::parse(doc).map_err(|d| format!("the views: {}:{}: {}", d.line, d.col, d.message))? };
    match file.items.iter_mut().rev().find(|i| matches!(&i.node, Item::View(x) if x.name == v.name)) {
        Some(slot) => slot.node = Item::View(v.clone()),
        None => file.items.push(node(Item::View(v.clone()))),
    }
    Ok(crate::format(&file))
}

/// The view `name` in `doc`, if it is there.
pub fn view_in(doc: &str, name: &str) -> Option<View> {
    let file = crate::parse(doc).ok()?;
    find_view(&file, name).cloned()
}

/// What `derived` shows, and, for a value that is not a colour yet, the expression whose range of values must be known to
/// map it into a neutral palette (the caller measures it on the frame and calls again with `range`).
pub struct Derived {
    pub view: View,
    pub shows: String,
    pub needs_range: Option<(String, Expr)>,
}

/// The view cut at node `id` of `v`, named `name`: the composite is the view itself, a layer is that layer alone, and a
/// node of an entry's chain is its layer with that entry ending at the node. A colour shows as it is; a scaled value
/// through `grays`; an identity through `categorical`; a direction through `phase`; a number or an amount through a
/// linear scale over `range` (the values of the frame it is drawn from), then `grays`. A condition cannot be shown.
pub fn derived(v: &View, id: &str, name: &str, range: Option<(f64, f64)>, world: &World) -> Result<Derived, String> {
    let mut out = v.clone();
    out.name = name.to_string();
    if id == "composite" {
        return Ok(Derived { view: out, shows: format!("the whole view {} (every layer, in order)", v.name), needs_range: None });
    }
    let rest = id.strip_prefix("layer:").ok_or_else(|| format!("no node `{id}`"))?;
    let (layer, tail) = rest.split_once('/').unwrap_or((rest, ""));
    let l = v.layers.iter().find(|l| l.node.name == layer).ok_or_else(|| format!("no layer `{layer}`"))?.node.clone();
    let mut alone = l.clone();
    let shows;
    let mut needs = None;
    let parts: Vec<&str> = tail.split('/').filter(|s| !s.is_empty()).collect();
    match parts.as_slice() {
        [] | ["body"] => shows = format!("layer {layer} alone"),
        ["source"] => {
            alone.body.clear();
            if source_kind(&l) == "agents" {
                alone.entries = vec![node(Entry { key: "color".into(), value: Value::Expr(parse_expr("1.0 |> palette(grays)")?) }), node(Entry { key: "glyph".into(), value: Value::Expr(parse_expr("dot()")?) })];
                shows = "every agent the layer's source passes, as a white dot".to_string();
            } else {
                let value = parse_expr("value")?;
                let (lo, hi) = match range {
                    Some(r) => r,
                    None => return Ok(Derived { view: out, shows: String::new(), needs_range: Some((layer.to_string(), value)) }),
                };
                alone.entries = vec![node(Entry { key: "color".into(), value: Value::Expr(parse_expr(&format!("value |> scale(linear, range: {}..{}) |> palette(grays)", num(lo), num(hi.max(lo + 1e-9))))?) })];
                shows = format!("the source's value, from {} (black) to {} (white): the range this frame holds", num(lo), num(hi));
            }
        }
        [entry] | [entry, _] => {
            let e = l.entries.iter().find(|e| e.node.key == *entry).ok_or_else(|| format!("layer `{layer}` has no `{entry}`"))?;
            let Value::Expr(x) = &e.node.value else { return Err(format!("`{entry}` is not an expression")) };
            let (head, calls) = chain(x);
            let k = match parts.get(1) {
                Some(k) => k.parse::<usize>().map_err(|_| format!("no node `{id}`"))?,
                None => calls.len(),
            };
            if k > calls.len() {
                return Err(format!("no node `{id}`"));
            }
            let piece = prefix(&head, &calls, k);
            let (ty, _) = expr_type(world, &l, &piece);
            alone.body.clear();
            let keep: Vec<&str> = if *entry == "color" { vec!["glyph", "size"] } else { vec!["glyph"] };
            alone.entries.retain(|x| keep.contains(&x.node.key.as_str()));
            let color = match (*entry, ty) {
                ("glyph", _) => {
                    alone.entries.retain(|x| x.node.key != "glyph");
                    alone.entries.push(node(Entry { key: "glyph".into(), value: Value::Expr(piece.clone()) }));
                    shows = format!("the glyph {} gives, in white", expr_text(&piece));
                    "1.0 |> palette(grays)".to_string()
                }
                ("size", _) => {
                    alone.entries.retain(|x| x.node.key != "size");
                    alone.entries.push(node(Entry { key: "size".into(), value: Value::Expr(piece.clone()) }));
                    shows = format!("the size {} gives, in white", expr_text(&piece));
                    "1.0 |> palette(grays)".to_string()
                }
                (_, "colour") => {
                    shows = format!("the colour {} gives", expr_text(&piece));
                    expr_text(&piece)
                }
                (_, "scaled") => {
                    shows = format!("{} (0 black, 1 white)", expr_text(&piece));
                    format!("{} |> palette(grays)", paren(&piece))
                }
                (_, "identity") => {
                    shows = format!("{} by identity", expr_text(&piece));
                    format!("{} |> palette(categorical)", paren(&piece))
                }
                (_, "direction") => {
                    shows = format!("{} as a direction (the hue goes round)", expr_text(&piece));
                    format!("{} |> palette(phase)", paren(&piece))
                }
                (_, "number" | "whole number" | "amount") => {
                    let num_expr = if ty == "amount" { parse_expr(&format!("units({})", expr_text(&piece)))? } else { piece.clone() };
                    let (lo, hi) = match range {
                        Some(r) => r,
                        None => return Ok(Derived { view: out, shows: String::new(), needs_range: Some((layer.to_string(), num_expr)) }),
                    };
                    shows = format!("{} from {} (black) to {} (white): the range this frame holds", expr_text(&piece), num(lo), num(hi));
                    format!("{} |> scale(linear, range: {}..{}) |> palette(grays)", paren(&num_expr), num(lo), num(hi.max(lo + 1e-9)))
                }
                (_, other) => return Err(format!("a {other} cannot be shown as a picture on its own")),
            };
            alone.entries.push(node(Entry { key: "color".into(), value: Value::Expr(parse_expr(&color)?) }));
        }
        _ => return Err(format!("no node `{id}`")),
    }
    out.layers = vec![node(alone)];
    Ok(Derived { view: out, shows, needs_range: needs.take() })
}

/// An expression in parentheses where a `|>` after it would bind to a part of it.
fn paren(e: &Expr) -> String {
    match e {
        Expr::Binary(..) | Expr::Not(_) | Expr::Neg(_) => format!("({})", expr_text(e)),
        _ => expr_text(e),
    }
}

/// A number as a view's float literal.
fn num(x: f64) -> String {
    let s = format!("{}", (x * 1e6).round() / 1e6);
    if s.contains('.') || s.contains('e') { s } else { format!("{s}.0") }
}
