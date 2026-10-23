//! The node-graph projection of a world (`proposals/node-editor.md` §3, §5): boxes and wires drawn
//! from the source text, which stays the truth.
//!
//! Every declaration of the world is one box, and every statement of a rule is one box inside its
//! rule. A statement the editor draws specially (a transfer, a reaction, a diffusion, a signal
//! assignment) gets wires to the stores and reactions it names; any other statement is a **code
//! box** holding its text verbatim, so the projection is defined for every world that parses and
//! no statement is lost or drawn twice. A box's `text` is the canonical source of its item, as
//! `emerge fmt` writes it, so a box can be replaced by a patch that carries its edited text.

use crate::ast::*;
use crate::fmt::{decl_text, lit, stmt_text, value_text};
use crate::schedule::{lower, Slot};

/// What a box is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A `field`, `source` or `sink`: where amounts are held.
    Store,
    /// A `reaction` declaration (its formula).
    Reaction,
    /// A world rule: the group of its statements.
    Rule,
    /// `transfer(s, from: A, to: B, amount: …)`.
    Transfer,
    /// `react(name, rate: …)`.
    React,
    /// `diffuse_conserved([s…], rate: …)`.
    Diffuse,
    /// An assignment to a signal field, or `life_like`.
    Signal,
    /// Any statement the editor does not draw specially, verbatim.
    Code,
    /// Anything else the world declares: elements, constants, substances, waste, menus, settings.
    Decl,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Store => "store",
            Kind::Reaction => "reaction",
            Kind::Rule => "rule",
            Kind::Transfer => "transfer",
            Kind::React => "react",
            Kind::Diffuse => "diffuse",
            Kind::Signal => "signal",
            Kind::Code => "code",
            Kind::Decl => "decl",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GNode {
    /// The item's path (`Dish.fields.cells`, `Dish.rules.weather@transfer_carbon`). A statement
    /// without a label ends in `#` and its place in the rule.
    pub id: String,
    pub kind: Kind,
    /// The name shown on the box.
    pub title: String,
    /// The canonical source of the item.
    pub text: String,
    /// The step of the tick it runs in (`emerge order`), for statements.
    pub step: Option<&'static str>,
    /// The rule a statement belongs to.
    pub group: Option<String>,
    /// The line where it starts in the source.
    pub line: usize,
    /// The named arguments of a call statement, as source text (`amount`, `40mu`).
    pub params: Vec<(String, String)>,
}

/// A wire from one box to another, carrying a substance (or nothing, for a reaction's formula).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wire {
    pub from: String,
    pub to: String,
    pub substance: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Graph {
    pub world: String,
    pub nodes: Vec<GNode>,
    pub wires: Vec<Wire>,
}

/// What a declaration box lets you set: the entries of a config block, a setting, a constant, an
/// action's cost (`take.cost`), a reaction's catalyst. Each is one `set` patch (`op_patch`).
fn decl_params(d: &WDecl) -> Vec<(String, String)> {
    match d {
        WDecl::Config { entries, .. } => entries.iter().map(|e| (e.node.key.clone(), value_text(&e.node.value))).collect(),
        WDecl::Setting(e) => vec![(e.key.clone(), value_text(&e.value))],
        WDecl::Const { value, .. } => vec![("value".to_string(), value_text(&Value::Expr(value.clone())))],
        WDecl::Menu { items, .. } => items
            .iter()
            .filter_map(|m| match &m.node {
                MenuItem::Action { name, cost, .. } => Some((format!("{name}.cost"), lit(cost))),
                _ => None,
            })
            .collect(),
        WDecl::Reaction { catalyze, .. } => vec![("catalyze".to_string(), catalyze.as_ref().map(|c| value_text(&Value::Expr(Expr::Call(c.clone())))).unwrap_or_default())],
        _ => Vec::new(),
    }
}

fn plural(d: &WDecl) -> &'static str {
    match d {
        WDecl::Element(_) => "elements",
        WDecl::Const { .. } => "consts",
        WDecl::Substance { .. } => "substances",
        WDecl::Field { .. } => "fields",
        WDecl::Source { .. } => "sources",
        WDecl::Sink { .. } => "sinks",
        WDecl::Waste { .. } => "waste",
        WDecl::Reaction { .. } => "reactions",
        WDecl::Menu { .. } => "menus",
        WDecl::Rule { .. } => "rules",
        WDecl::Genesis(_) => "genesis",
        WDecl::Config { .. } => "config",
        WDecl::Setting(_) => "settings",
    }
}

fn name_of(d: &WDecl) -> String {
    match d {
        WDecl::Element(n) | WDecl::Const { name: n, .. } | WDecl::Substance { name: n, .. } => n.clone(),
        WDecl::Field { name, .. } | WDecl::Source { name, .. } | WDecl::Sink { name, .. } | WDecl::Reaction { name, .. } | WDecl::Rule { name, .. } => name.clone(),
        WDecl::Waste { substance, .. } => substance.clone(),
        WDecl::Menu { kind, .. } => kind.clone(),
        WDecl::Genesis(_) => "genesis".into(),
        WDecl::Config { name, .. } => name.clone(),
        WDecl::Setting(e) => e.key.clone(),
    }
}

/// The name an argument holds, if it is a plain name.
fn arg_name(c: &Call, key: &str) -> Option<String> {
    c.args.iter().find(|a| a.name.as_deref() == Some(key)).and_then(|a| match &a.value {
        Value::Expr(Expr::Path(Path { root, segs })) if segs.is_empty() => Some(root.clone()),
        _ => None,
    })
}

/// The subject (first unnamed argument) as a name, or as the names of a list.
fn subject(c: &Call) -> Vec<String> {
    let Some(a) = c.args.iter().find(|a| a.name.is_none()) else { return Vec::new() };
    match &a.value {
        Value::Expr(Expr::Path(Path { root, segs })) if segs.is_empty() => vec![root.clone()],
        Value::Expr(Expr::List(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::Expr(Expr::Path(Path { root, segs })) if segs.is_empty() => Some(root.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The graph of `world`. `file` is the parsed file, as written, so each box's text is the text of
/// the item; the schedule is worked out from the prepared file.
pub fn graph(file: &File, world: &str) -> Result<Graph, String> {
    let prepared = crate::prepare(file).map_err(|p| format!("{}: {}: {}", p.code, p.path, p.message))?;
    let schedule = lower(&prepared, world)?;
    let w = file
        .items
        .iter()
        .find_map(|i| match &i.node {
            Item::World(w) if w.name == world => Some(w),
            _ => None,
        })
        .ok_or_else(|| format!("no world `{world}`"))?;

    let id = |d: &WDecl| format!("{world}.{}.{}", plural(d), name_of(d));
    // Stores and reactions by name, for wires.
    let store = |n: &str| {
        w.decls.iter().find_map(|d| match &d.node {
            WDecl::Field { name, .. } | WDecl::Source { name, .. } | WDecl::Sink { name, .. } if name == n => Some(id(&d.node)),
            _ => None,
        })
    };
    let reaction = |n: &str| {
        w.decls.iter().find_map(|d| match &d.node {
            WDecl::Reaction { name, .. } if name == n => Some((id(&d.node), d)),
            _ => None,
        })
    };
    let step_of = |slot: Slot| schedule.steps.iter().find(|s| s.slot == slot).map(|s| s.name);

    let mut nodes = Vec::new();
    let mut wires = Vec::new();
    for d in &w.decls {
        let kind = match &d.node {
            WDecl::Field { .. } | WDecl::Source { .. } | WDecl::Sink { .. } => Kind::Store,
            WDecl::Reaction { .. } => Kind::Reaction,
            WDecl::Rule { .. } => Kind::Rule,
            _ => Kind::Decl,
        };
        let rule_id = id(&d.node);
        nodes.push(GNode { id: rule_id.clone(), kind, title: name_of(&d.node), text: decl_text(&d.node), step: None, group: None, line: d.pos.line, params: decl_params(&d.node) });

        let WDecl::Rule { name, on, body } = &d.node else { continue };
        let lowered = schedule.rules.iter().find(|r| &r.name == name);
        // Names a statement may use for the cells it works on.
        let target = |n: &str| if n == "self" || n == "cell" { store(on) } else { store(n) };
        for (i, st) in body.iter().enumerate() {
            let low = lowered.and_then(|r| r.stmts.get(i));
            let sid = match low.and_then(|l| l.label.clone()) {
                Some(l) => format!("{world}.rules.{name}@{l}"),
                None => format!("{world}.rules.{name}#{i}"),
            };
            let slot = low.and_then(|l| l.slot.clone().ok());
            let (kind, title) = match &st.node {
                Stmt::Call { call, .. } if call.name == "transfer" => (Kind::Transfer, format!("transfer {}", subject(call).join(", "))),
                Stmt::Call { call, .. } if call.name == "react" => (Kind::React, format!("react {}", subject(call).join(", "))),
                Stmt::Call { call, .. } if call.name == "diffuse_conserved" => (Kind::Diffuse, format!("diffuse {}", subject(call).join(", "))),
                Stmt::Assign { .. } => (Kind::Signal, "set".to_string()),
                Stmt::Call { call, .. } if call.name == "life_like" => (Kind::Signal, "life_like".to_string()),
                _ => (Kind::Code, "code".to_string()),
            };
            nodes.push(GNode {
                id: sid.clone(),
                kind,
                title,
                text: stmt_text(&st.node),
                step: slot.and_then(step_of),
                group: Some(rule_id.clone()),
                line: st.pos.line,
                params: match &st.node {
                    Stmt::Call { call, .. } => call.args.iter().filter_map(|a| a.name.as_ref().map(|n| (n.clone(), value_text(&a.value)))).collect(),
                    _ => Vec::new(),
                },
            });
            let Stmt::Call { call, .. } = &st.node else { continue };
            match kind {
                Kind::Transfer => {
                    let s = subject(call).into_iter().next();
                    let from = arg_name(call, "from").and_then(|n| target(&n));
                    // A move to a neighbouring cell (`to: below`) stays in the field it leaves.
                    let to = arg_name(call, "to").and_then(|n| if ["above", "right", "below", "left"].contains(&n.as_str()) { store(on) } else { target(&n) });
                    if let Some(f) = from {
                        wires.push(Wire { from: f, to: sid.clone(), substance: s.clone() });
                    }
                    if let Some(t) = to {
                        wires.push(Wire { from: sid.clone(), to: t, substance: s });
                    }
                }
                Kind::React => {
                    let Some(r) = subject(call).into_iter().next().and_then(|n| reaction(&n)) else { continue };
                    wires.push(Wire { from: r.0, to: sid.clone(), substance: None });
                    if let (Some(cells), WDecl::Reaction { formula, .. }) = (store(on), &r.1.node) {
                        for t in &formula.inputs {
                            wires.push(Wire { from: cells.clone(), to: sid.clone(), substance: Some(t.name.clone()) });
                        }
                        for t in &formula.outputs {
                            wires.push(Wire { from: sid.clone(), to: cells.clone(), substance: Some(t.name.clone()) });
                        }
                    }
                }
                Kind::Diffuse => {
                    if let Some(cells) = store(on) {
                        for s in subject(call) {
                            wires.push(Wire { from: cells.clone(), to: sid.clone(), substance: Some(s.clone()) });
                            wires.push(Wire { from: sid.clone(), to: cells.clone(), substance: Some(s) });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    // Waste: reaction heat and the like leaves for a sink.
    for d in &w.decls {
        if let WDecl::Waste { substance, sink } = &d.node {
            if let Some(s) = store(sink) {
                wires.push(Wire { from: id(&d.node), to: s, substance: Some(substance.clone()) });
            }
        }
    }
    Ok(Graph { world: world.to_string(), nodes, wires })
}

/// One edit of the graph as patches: the patch itself (written against `base`), the box the edit leaves
/// selected, and the bodies of the two patches that move between the states before and after it. A
/// body is what goes between the braces of `patch W base N { … }`, so undo and redo can be written
/// against whatever the revision is when they are applied.
#[derive(Clone, Debug)]
pub struct Edit {
    pub patch: String,
    pub selected: Option<String>,
    /// Goes back to the state before the edit.
    pub undo: String,
    /// Does the edit again after an undo.
    pub redo: String,
}

impl Edit {
    fn new(world: &str, base: u64, redo: String, undo: String, selected: Option<String>) -> Edit {
        Edit { patch: patch_text(world, base, &redo), selected, undo, redo }
    }
}

/// `patch World base N { body }`.
pub fn patch_text(world: &str, base: u64, body: &str) -> String {
    format!("patch {world} base {base} {{ {body} }}")
}

/// The statements of the rule that holds box `id`, as they are now (the whole rule body: what undoes a
/// change to any one of them, whatever happened to its label).
fn rule_body_replace(g: &Graph, rule_id: &str) -> String {
    let prefix = format!("{}.", g.world);
    let rel = rule_id.strip_prefix(&prefix).unwrap_or(rule_id);
    let stmts: Vec<&str> = g.nodes.iter().filter(|n| n.group.as_deref() == Some(rule_id)).map(|n| n.text.as_str()).collect();
    format!("replace {rel} {{ {} }}", stmts.join(" "))
}

/// The edit that replaces the text of one statement box (`proposals/node-editor.md` §6, N2):
/// `replace rules.R@label { … }` for a labelled statement, and for one without a label the rule's
/// whole body with that statement swapped. `base` is the revision the graph was drawn at. Only
/// statements can be edited; the other boxes are not yet (a reaction or a store has no `replace`).
pub fn edit_text(g: &Graph, id: &str, text: &str, base: u64) -> Result<Edit, String> {
    let node = g.nodes.iter().find(|n| n.id == id).ok_or_else(|| format!("no box `{id}`"))?;
    let Some(rule_id) = &node.group else { return Err(format!("`{id}` is not a statement; only statements can be edited so far")) };
    let prefix = format!("{}.", g.world);
    let rel = |s: &str| s.strip_prefix(&prefix).unwrap_or(s).to_string();
    let body = if id.contains('@') {
        format!("replace {} {{ {text} }}", rel(id))
    } else {
        let stmts: Vec<String> = g.nodes.iter().filter(|n| n.group.as_ref() == Some(rule_id)).map(|n| if n.id == id { text.to_string() } else { n.text.clone() }).collect();
        format!("replace {} {{ {} }}", rel(rule_id), stmts.join(" "))
    };
    Ok(Edit::new(&g.world, base, body, rule_body_replace(g, rule_id), Some(id.to_string())))
}

/// A statement inside a `when` or `else` (`contract::Nested`): `<top-level statement id>/<path>`, where the path is
/// `t0` (the first statement of the `then` block), `e1` (the second of the `else`), `t0.e2` (inside a deeper `when`).
/// Returns the top-level statement's id and the way down: for each step, whether it is the `else` block and the index.
fn nested_route(id: &str) -> Result<(&str, Vec<(bool, usize)>), String> {
    let (container, path) = id.split_once('/').ok_or_else(|| format!("`{id}` is not a statement inside a `when`"))?;
    let mut route = Vec::new();
    for seg in path.split('.') {
        let (kind, n) = seg.split_at(1);
        let index: usize = n.parse().map_err(|_| format!("`{id}` is not a statement inside a `when`"))?;
        match kind {
            "t" => route.push((false, index)),
            "e" => route.push((true, index)),
            _ => return Err(format!("`{id}` is not a statement inside a `when`")),
        }
    }
    Ok((container, route))
}

/// The statement at the end of `route` in `st`, or none if the source has moved on.
fn nested_mut<'a>(st: &'a mut Stmt, route: &[(bool, usize)]) -> Option<&'a mut Stmt> {
    let Some(((other, i), rest)) = route.split_first() else { return Some(st) };
    let Stmt::When { then, otherwise, .. } = st else { return None };
    let block = if *other { otherwise.as_mut()? } else { then };
    nested_mut(&mut block.get_mut(*i)?.node, rest)
}

/// What changes in a nested statement: its text, one argument, or the statement itself.
enum NestedChange {
    Text(String),
    Arg(String, String),
    Delete,
}

/// An edit of a statement inside a `when`: the top-level statement that holds it is rebuilt with the change, and its rule's
/// whole body is replaced in one patch (so one revision, one undo). The way back replaces the body with what it was.
fn nested_edit(file: &File, world: &str, id: &str, change: NestedChange, base: u64) -> Result<Edit, String> {
    let (container, route) = nested_route(id)?;
    let prefix = format!("{world}.rules.");
    let tail = container.strip_prefix(&prefix).ok_or_else(|| format!("no statement `{container}`"))?;
    let (rule, at) = tail.split_once('#').ok_or_else(|| format!("`{container}` does not hold a `when`"))?;
    let at: usize = at.parse().map_err(|_| format!("`{container}` does not hold a `when`"))?;
    let w = file.items.iter().find_map(|i| match &i.node { Item::World(w) if w.name == world => Some(w), _ => None }).ok_or_else(|| format!("no world `{world}`"))?;
    let body = w.decls.iter().find_map(|d| match &d.node { WDecl::Rule { name, body, .. } if name == rule => Some(body), _ => None }).ok_or_else(|| format!("no rule `{rule}`"))?;
    let texts = |b: &Block| b.iter().map(|s| stmt_text(&s.node)).collect::<Vec<_>>().join(" ");
    let mut new = body.clone();
    let top = new.get_mut(at).ok_or_else(|| format!("no statement `{container}`"))?;
    let gone = || format!("`{id}` is no longer in the source");
    match change {
        NestedChange::Delete => {
            let (last, parent) = route.split_last().ok_or_else(gone)?;
            let holder = nested_mut(&mut top.node, parent).ok_or_else(gone)?;
            let Stmt::When { then, otherwise, .. } = holder else { return Err(gone()) };
            let block = if last.0 { otherwise.as_mut().ok_or_else(gone)? } else { then };
            if last.1 >= block.len() { return Err(gone()); }
            block.remove(last.1);
        }
        NestedChange::Text(text) => {
            let stmt = parse_stmt(&text)?;
            *nested_mut(&mut top.node, &route).ok_or_else(gone)? = stmt;
        }
        NestedChange::Arg(param, value) => {
            let v = parse_value(&value)?;
            let Stmt::Call { call, .. } = nested_mut(&mut top.node, &route).ok_or_else(gone)? else { return Err(format!("`{id}` has no arguments to set")) };
            put_arg(call, Some(&param), v);
        }
    }
    let rel = format!("rules.{rule}");
    Ok(Edit::new(world, base, format!("replace {rel} {{ {} }}", texts(&new)), format!("replace {rel} {{ {} }}", texts(body)), Some(id.to_string())))
}

/// Statements from source text, as a rule body holds them.
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

/// One statement from source text.
fn parse_stmt(text: &str) -> Result<Stmt, String> {
    match parse_block(text)?.as_slice() {
        [one] => Ok(one.node.clone()),
        [] => Err("no statement".to_string()),
        _ => Err("this is one statement: write a `when` of its own to hold several".to_string()),
    }
}

/// The edit that replaces the text of the statement box `id`, wherever it is: a top-level statement (`edit_text`) or one
/// inside a `when` (rebuilt through the statement that holds it).
pub fn edit_text_in(file: &File, world: &str, id: &str, text: &str, base: u64) -> Result<Edit, String> {
    if id.contains('/') {
        return nested_edit(file, world, id, NestedChange::Text(text.to_string()), base);
    }
    let g = graph(file, world)?;
    edit_text(&g, id, text, base)
}

/// An edit of the graph (`proposals/node-editor.md` §6, N3). Each one becomes one patch.
#[derive(Clone, Debug)]
pub enum Op {
    /// Change (or add) a named argument of a statement: `amount`, `rate`, `overflow`, …; `value` is
    /// source text for the argument.
    SetParam { id: String, param: String, value: String },
    /// Connect two boxes: a store to a transfer sets its `from`, a transfer to a store its `to`, a
    /// reaction to a `react` the reaction it runs.
    Wire { from: String, to: String },
    /// Remove a statement (its rule too, if it was the rule's only one).
    Delete { id: String },
    /// A new rule with one statement of `kind` (`transfer`, `react`, `diffuse` or `code`) with defaults
    /// the world allows; every rule holds one kind of effect, so a new box is a new rule.
    AddRule { kind: String },
    /// A new rule with one statement of `kind` already connected: `from` is a source (or a store) for a `transfer`, or
    /// the reaction for a `react`; `to` is the field the rule works on. One patch, one revision: nothing is applied
    /// first and connected after.
    AddConnected { kind: String, from: String, to: String, substance: Option<String> },
    /// Copy statements, each into a rule of its own after the one it was in, with fresh names (a copy runs as well,
    /// so it is a second execution of the same effect).
    Duplicate { ids: Vec<String> },
    /// Give an unlabelled call statement a label, so a name keeps it in place whatever is inserted before it.
    Label { id: String, label: String },
    /// Take a connection away: only where the argument is optional (`overflow`). A required end is replaced by wiring
    /// a different one, never removed.
    Unwire { id: String, port: String },
    /// Several edits as one: each is worked out against the file the ones before it leave, and the result is one
    /// patch, one revision and one undo.
    Batch { ops: Vec<Op> },
    /// Statements copied as text (from this world or another), each into a new rule of its own on the field it was on
    /// (`on`, which this world must have). The checker of this world decides whether they are valid here.
    Paste { items: Vec<(String, String)> },
    /// A box dropped on a wire (`proposals/houdini-node-system.md` §6): only where the wire has a defined insertion. The
    /// one there is: a sink dropped on the wire from a transfer to its destination becomes that transfer's `overflow`
    /// (what does not fit goes to the sink). Anywhere else the box only moves.
    Insert { id: String, from: String, to: String },
}

/// A value as the AST holds it, parsed from source text.
fn parse_value(text: &str) -> Result<Value, String> {
    let src = format!("world T {{ rule r on f {{ f(v: {text}); }} }}");
    let file = crate::parse(&src).map_err(|d| format!("{}: {}", d.code, d.message))?;
    for item in &file.items {
        if let Item::World(w) = &item.node {
            for d in &w.decls {
                if let WDecl::Rule { body, .. } = &d.node {
                    if let Some(Stmt::Call { call, .. }) = body.first().map(|s| &s.node) {
                        return call.args.first().map(|a| a.value.clone()).ok_or_else(|| "no value".to_string());
                    }
                }
            }
        }
    }
    Err("not a value".to_string())
}

fn name_value(name: &str) -> Value {
    Value::Expr(Expr::Path(Path { root: name.to_string(), segs: Vec::new() }))
}

/// Sets the argument `name` (the subject if `None`) of `call`, adding it if it is not there.
fn put_arg(call: &mut Call, name: Option<&str>, v: Value) {
    match call.args.iter_mut().find(|a| a.name.as_deref() == name) {
        Some(a) => a.value = v,
        None => call.args.push(Arg { name: name.map(str::to_string), value: v }),
    }
}

/// The name a store box stands for: the last segment of its id.
fn short(id: &str) -> &str {
    id.rsplit('.').next().unwrap_or(id)
}

/// The patch for `op` on `world` of the parsed `file`, written against revision `base`, and the id
/// of the box the edit leaves selected, if there is one.
pub fn op_patch(file: &File, world: &str, op: &Op, base: u64) -> Result<Edit, String> {
    let g = graph(file, world)?;
    let w = file
        .items
        .iter()
        .find_map(|i| match &i.node {
            Item::World(w) if w.name == world => Some(w),
            _ => None,
        })
        .ok_or_else(|| format!("no world `{world}`"))?;
    let node = |id: &str| g.nodes.iter().find(|n| n.id == id).ok_or_else(|| format!("no box `{id}`"));
    let prefix = format!("{world}.");
    let rel = |s: &str| s.strip_prefix(&prefix).unwrap_or(s).to_string();
    // The statement a box stands for: its rule's name and field, and the statement.
    let statement = |id: &str| -> Result<(String, String, Stmt), String> {
        let n = node(id)?;
        let rule_id = n.group.as_ref().ok_or_else(|| format!("`{id}` is not a statement"))?;
        for d in &w.decls {
            let WDecl::Rule { name, on, body } = &d.node else { continue };
            if format!("{world}.rules.{name}") != *rule_id {
                continue;
            }
            for (i, st) in body.iter().enumerate() {
                let sid = match &st.node {
                    Stmt::Call { label: Some(l), .. } => format!("{world}.rules.{name}@{l}"),
                    _ => format!("{world}.rules.{name}#{i}"),
                };
                if sid == id {
                    return Ok((name.clone(), on.clone(), st.node.clone()));
                }
            }
        }
        Err(format!("no statement `{id}`"))
    };
    let replace_with = |id: &str, stmt: &Stmt| edit_text(&g, id, &stmt_text(stmt), base);
    // Change one named argument. A labelled statement that has the argument is changed in place with
    // `set`, which keeps its comments; otherwise the statement is replaced by its new text.
    let set_arg = |id: &str, param: &str, text: &str| -> Result<Edit, String> {
        let (_, _, mut stmt) = statement(id)?;
        let value = parse_value(text)?;
        let Stmt::Call { call, .. } = &mut stmt else { return Err(format!("`{id}` has no arguments to set")) };
        if id.contains('@') && call.args.iter().any(|a| a.name.as_deref() == Some(param)) {
            // Undone by setting the old text back, if the argument had one that can be written again.
            let old = node(id)?.params.iter().find(|(k, _)| k == param).map(|(_, v)| v.clone());
            let undo = match old {
                Some(v) if parse_value(&v).is_ok() => format!("set {}.{param} = {v};", rel(id)),
                _ => rule_body_replace(&g, node(id)?.group.as_deref().unwrap_or_default()),
            };
            return Ok(Edit::new(world, base, format!("set {}.{param} = {text};", rel(id)), undo, Some(id.to_string())));
        }
        put_arg(call, Some(param), value);
        replace_with(id, &stmt)
    };

    match op {
        Op::Paste { items } => {
            if items.is_empty() {
                return Err("nothing to paste".to_string());
            }
            let mut taken: Vec<String> = w.decls.iter().filter_map(|d| crate::check::declared(&d.node).map(|(n, _)| n.to_string())).collect();
            let fields: Vec<&str> = w.decls.iter().filter_map(|d| match &d.node { WDecl::Field { name, .. } => Some(name.as_str()), _ => None }).collect();
            let mut place = w.decls.iter().rev().find_map(|d| match &d.node { WDecl::Rule { name, .. } => Some(name.clone()), _ => None });
            let (mut redo, mut undo) = (Vec::new(), Vec::new());
            let mut selected = None;
            for (on, text) in items {
                if !fields.contains(&on.as_str()) {
                    return Err(format!("this world has no field `{on}` for the pasted statement to work on (it has {})", fields.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ")));
                }
                let stmt = parse_stmt(text).map_err(|m| format!("the pasted text: {m}"))?;
                let name = (1..).map(|k| format!("pasted_{k}")).find(|n| !taken.contains(n)).unwrap();
                taken.push(name.clone());
                let at = match &place { Some(p) => format!("add after rules.{p}: "), None => "add ".to_string() };
                redo.push(format!("{at}rule {name} on {on} {{ {} }}", stmt_text(&stmt)));
                undo.push(format!("remove rules.{name};"));
                selected = Some(match &stmt { Stmt::Call { label: Some(l), .. } => format!("{world}.rules.{name}@{l}"), _ => format!("{world}.rules.{name}#0") });
                place = Some(name);
            }
            undo.reverse();
            Ok(Edit::new(world, base, redo.join(" "), undo.join(" "), selected))
        }
        Op::Insert { id, from, to } => {
            let (n, a, b) = (node(id)?, node(from)?, node(to)?);
            let wired = g.wires.iter().any(|x| x.from == *from && x.to == *to);
            if !wired {
                return Err(format!("there is no wire from `{}` to `{}`", a.title, b.title));
            }
            let sink = n.kind == Kind::Store && n.id.starts_with(&format!("{world}.sinks."));
            if !(sink && a.kind == Kind::Transfer && b.kind == Kind::Store) {
                return Err(format!("no insertion is defined for `{}` on the wire from `{}` to `{}`: a sink dropped on a transfer's destination wire becomes its overflow; anything else only moves", n.title, a.title, b.title));
            }
            set_arg(from, "overflow", short(id))
        }
        Op::SetParam { id, param, value } if id.contains('/') => nested_edit(file, world, id, NestedChange::Arg(param.clone(), value.clone()), base),
        Op::Delete { id } if id.contains('/') => nested_edit(file, world, id, NestedChange::Delete, base),
        Op::Batch { ops } => op_batch(file, world, ops, base),
        Op::AddConnected { kind, from, to, substance } => {
            let field = to.strip_prefix(&prefix).and_then(|r| r.strip_prefix("fields.")).ok_or_else(|| format!("`{to}` is not a field: a new rule works on a field"))?;
            let held = w.decls.iter().any(|d| matches!(&d.node, WDecl::Field { name, holds: Some(_), .. } if name == field));
            if !held {
                return Err(format!("`{field}` holds no substances, so a rule cannot move any into it"));
            }
            let last_rule = w.decls.iter().rev().find_map(|d| match &d.node {
                WDecl::Rule { name, .. } => Some(name.clone()),
                _ => None,
            });
            let last_rule = last_rule.ok_or("the world has no rule to place the new one after")?;
            let mut taken: Vec<String> = Vec::new();
            for d in &w.decls {
                if let Some((n, _)) = crate::check::declared(&d.node) {
                    taken.push(n.to_string());
                }
            }
            let n = (1..).find(|n| !taken.contains(&format!("new_{kind}_{n}"))).unwrap();
            let name = format!("new_{kind}_{n}");
            let src = short(from);
            let stmt = match kind.as_str() {
                "transfer" => {
                    let sources: Vec<&WDecl> = w.decls.iter().map(|d| &d.node).filter(|d| matches!(d, WDecl::Source { name, .. } if name == src)).collect();
                    let WDecl::Source { emits, .. } = sources.first().ok_or_else(|| format!("`{from}` is not a source: a new transfer takes from a source"))? else { unreachable!() };
                    let s = match substance {
                        Some(x) if emits.contains(x) => x.clone(),
                        Some(x) => return Err(format!("`{src}` does not emit `{x}`")),
                        None => emits.first().cloned().ok_or_else(|| format!("`{src}` emits nothing"))?,
                    };
                    format!("transfer({s}, from: {src}, to: self, amount: 1u) @transfer_{s}_{n};")
                }
                "react" => {
                    if !w.decls.iter().any(|d| matches!(&d.node, WDecl::Reaction { name, .. } if name == src)) {
                        return Err(format!("`{from}` is not a reaction"));
                    }
                    format!("react({src}, rate: 100bp) @react_{src}_{n};")
                }
                other => return Err(format!("`{other}` cannot be created connected: transfer or react")),
            };
            let label = stmt.rsplit('@').next().unwrap_or("").trim_end_matches(';').to_string();
            let body = format!("add after rules.{last_rule}: rule {name} on {field} {{ {stmt} }}");
            Ok(Edit::new(world, base, body, format!("remove rules.{name};"), Some(format!("{world}.rules.{name}@{label}"))))
        }
        Op::Label { id, label } => {
            if label.is_empty() || !label.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') || label.starts_with(|c: char| c.is_ascii_digit()) {
                return Err(format!("`{label}` is not a label: lower-case letters, digits and `_`, not starting with a digit"));
            }
            let (_, _, mut stmt) = statement(id)?;
            let Stmt::Call { label: l, .. } = &mut stmt else { return Err("only call statements carry a label".to_string()) };
            if l.is_some() {
                return Err(format!("`{id}` already has a label"));
            }
            let used = g.nodes.iter().any(|n| n.id.ends_with(&format!("@{label}")));
            if used {
                return Err(format!("the label `{label}` is already used in this world"));
            }
            *l = Some(label.clone());
            let n = node(id)?;
            let rule_id = n.group.clone().ok_or("not a statement")?;
            let new_id = format!("{rule_id}@{label}");
            let mut e = edit_text(&g, id, &stmt_text(&stmt), base)?;
            e.selected = Some(new_id);
            Ok(e)
        }
        Op::Unwire { id, port } => match port.as_str() {
            "overflow" => {
                let (_, _, mut stmt) = statement(id)?;
                let Stmt::Call { call, .. } = &mut stmt else { return Err(format!("`{id}` has no arguments")) };
                let before = call.args.len();
                call.args.retain(|a| a.name.as_deref() != Some("overflow"));
                if call.args.len() == before {
                    return Err(format!("`{id}` has no overflow to remove"));
                }
                replace_with(id, &stmt)
            }
            "from" | "to" | "reaction" => Err(format!("`{port}` is required: wire a different one in its place instead of removing it (a transfer with no {port} is not a transfer)")),
            other => Err(format!("`{other}` is not a port a wire can be cut from; derived wires follow the reaction's formula")),
        },
        Op::Duplicate { ids } => {
            let mut taken: Vec<String> = Vec::new();
            let mut labels: Vec<String> = g.nodes.iter().filter_map(|n| n.id.rsplit_once('@').map(|(_, l)| l.to_string())).collect();
            for d in &w.decls {
                if let Some((n, _)) = crate::check::declared(&d.node) {
                    taken.push(n.to_string());
                }
            }
            let mut notes = Vec::new();
            let (mut redo, mut undo) = (Vec::new(), Vec::new());
            let mut selected = None;
            // After the last copy of the same rule, so copies keep the order of the originals.
            let mut after: std::collections::HashMap<String, String> = Default::default();
            for id in ids {
                let (rule, on, stmt) = statement(id)?;
                let Stmt::Call { call, label } = &stmt else { return Err(format!("`{id}` is not a call statement: copy statements that are calls")) };
                let base_name = format!("{rule}_copy");
                let name = (1..).map(|k| if k == 1 { base_name.clone() } else { format!("{base_name}{k}") }).find(|n| !taken.contains(n)).unwrap();
                taken.push(name.clone());
                let stem = label.clone().unwrap_or_else(|| format!("{}_{}", call.name, name));
                let new_label = (1..).map(|k| if k == 1 { format!("{stem}_copy") } else { format!("{stem}_copy{k}") }).find(|l| !labels.contains(l)).unwrap();
                labels.push(new_label.clone());
                let mut copy = stmt.clone();
                if let Stmt::Call { label, .. } = &mut copy {
                    *label = Some(new_label.clone());
                }
                let place = after.get(&rule).cloned().unwrap_or_else(|| rule.clone());
                redo.push(format!("add after rules.{place}: rule {name} on {on} {{ {} }}", stmt_text(&copy)));
                undo.push(format!("remove rules.{name};"));
                notes.push(format!("{id} -> rule {name}, label {new_label}: the copy runs as well, so the effect happens twice"));
                after.insert(rule.clone(), name.clone());
                selected = Some(format!("{world}.rules.{name}@{new_label}"));
            }
            undo.reverse();
            let _ = notes;
            Ok(Edit::new(world, base, redo.join(" "), undo.join(" "), selected))
        }
        Op::SetParam { id, param, value } if node(id)?.group.is_none() => {
            // A declaration: a config entry, a setting, a constant, an action's cost, a catalyst.
            let n = node(id)?;
            if !n.params.iter().any(|(k, _)| k == param) {
                return Err(format!("`{}` has no `{param}` to set", n.title));
            }
            if value.trim().is_empty() {
                return Err(format!("`{param}` needs a value"));
            }
            parse_value(value)?;
            let path = match id.strip_prefix(&prefix).and_then(|r| r.split_once('.')) {
                Some(("config", name)) => format!("{name}.{param}"),
                Some(("settings", name)) => name.to_string(),
                Some(("consts", name)) => format!("consts.{name}"),
                Some(("menus", kind)) => format!("{kind}.{param}"),
                Some(("reactions", name)) => format!("reactions.{name}.{param}"),
                _ => return Err(format!("`{id}` cannot be set")),
            };
            let old = n.params.iter().find(|(k, _)| k == param).map(|(_, v)| v.clone()).unwrap_or_default();
            // A catalyst that was absent has no old value to write back: refused as not swappable anyway.
            Ok(Edit::new(world, base, format!("set {path} = {value};"), format!("set {path} = {old};"), Some(id.clone())))
        }
        Op::SetParam { id, param, value } => set_arg(id, param, value),
        Op::Wire { from, to } => {
            let (a, b) = (node(from)?, node(to)?);
            let target = match (a.kind, b.kind) {
                (Kind::Store, Kind::Transfer) | (Kind::Reaction, Kind::React) => to,
                (Kind::Transfer, Kind::Store) => from,
                _ => return Err(format!("`{}` and `{}` cannot be wired: a store goes to a transfer, a transfer to a store, a reaction to a react", a.title, b.title)),
            };
            let (_, on, _) = statement(target)?;
            match (a.kind, b.kind) {
                (Kind::Store, Kind::Transfer) => set_arg(target, "from", short(from)),
                (Kind::Transfer, Kind::Store) => {
                    let store = short(to);
                    set_arg(target, "to", if store == on { "self" } else { store })
                }
                _ => {
                    // The reaction a `react` runs is its subject, which has no name to `set`.
                    let (_, _, mut stmt) = statement(target)?;
                    let Stmt::Call { call, .. } = &mut stmt else { return Err("not a call".into()) };
                    put_arg(call, None, name_value(short(from)));
                    replace_with(target, &stmt)
                }
            }
        }
        Op::Delete { id } => {
            let n = node(id)?;
            let rule_id = n.group.clone().ok_or_else(|| format!("`{id}` is not a statement; only statements can be deleted so far"))?;
            let siblings = g.nodes.iter().filter(|x| x.group.as_ref() == Some(&rule_id)).count();
            let (body, undo) = if siblings == 1 {
                // The whole rule goes; it comes back after the rule before it (or before the one after).
                let rule_name = rule_id.rsplit('.').next().unwrap_or_default().to_string();
                let rules: Vec<&Node<WDecl>> = w.decls.iter().filter(|d| matches!(d.node, WDecl::Rule { .. })).collect();
                let at = rules.iter().position(|d| matches!(&d.node, WDecl::Rule { name, .. } if *name == rule_name)).ok_or("the rule is not in the world")?;
                let text = decl_text(&rules[at].node);
                let place = match (at.checked_sub(1).and_then(|i| rules.get(i)), rules.get(at + 1)) {
                    (Some(prev), _) => format!("add after rules.{}: ", name_of(&prev.node)),
                    (None, Some(next)) => format!("add before rules.{}: ", name_of(&next.node)),
                    (None, None) => "add ".to_string(),
                };
                (format!("remove {};", rel(&rule_id)), format!("{place}{text}"))
            } else if id.contains('@') {
                (format!("remove {};", rel(id)), rule_body_replace(&g, &rule_id))
            } else {
                let rest: Vec<String> = g.nodes.iter().filter(|x| x.group.as_ref() == Some(&rule_id) && x.id != *id).map(|x| x.text.clone()).collect();
                (format!("replace {} {{ {} }}", rel(&rule_id), rest.join(" ")), rule_body_replace(&g, &rule_id))
            };
            Ok(Edit::new(world, base, body, undo, None))
        }
        Op::AddRule { kind } => {
            let mut names: Vec<String> = Vec::new();
            let (mut field, mut source, mut reaction, mut last_rule) = (None, None, None, None);
            for d in &w.decls {
                match &d.node {
                    WDecl::Field { name, holds: Some(_), .. } if field.is_none() => field = Some(name.clone()),
                    WDecl::Source { name, emits } if source.is_none() && !emits.is_empty() => source = Some((name.clone(), emits[0].clone())),
                    WDecl::Reaction { name, .. } if reaction.is_none() => reaction = Some(name.clone()),
                    WDecl::Rule { name, .. } => last_rule = Some(name.clone()),
                    _ => {}
                }
                if let Some((n, _)) = crate::check::declared(&d.node) {
                    names.push(n.to_string());
                }
            }
            let field = field.ok_or("the world has no field that holds substances")?;
            let last_rule = last_rule.ok_or("the world has no rule to place the new one after")?;
            let n = (1..).find(|n| !names.contains(&format!("new_{kind}_{n}"))).unwrap();
            let name = format!("new_{kind}_{n}");
            let stmt = match kind.as_str() {
                "transfer" => {
                    let (s, p) = source.ok_or("the world has no source that emits something")?;
                    format!("transfer({p}, from: {s}, to: self, amount: 1u) @transfer_{p}_{n};")
                }
                "react" => {
                    let r = reaction.ok_or("the world has no reaction")?;
                    format!("react({r}, rate: 100bp) @react_{r}_{n};")
                }
                "diffuse" => {
                    let (_, p) = source.ok_or("the world has no source that emits something")?;
                    format!("diffuse_conserved([{p}], rate: 100bp) @diffuse_conserved_{n};")
                }
                // A code box: a statement to type over. It starts as a diffusion of the first source's
                // substance (valid, and it moves nothing the world did not already allow), labelled
                // `code_N` so the page draws it as a code box.
                "code" => {
                    let (_, p) = source.ok_or("the world has no source that emits something")?;
                    format!("diffuse_conserved([{p}], rate: 100bp) @code_{n};")
                }
                other => return Err(format!("unknown kind `{other}`: transfer, react, diffuse or code")),
            };
            let label = stmt.rsplit('@').next().unwrap_or("").trim_end_matches(';').to_string();
            let body = format!("add after rules.{last_rule}: rule {name} on {field} {{ {stmt} }}");
            Ok(Edit::new(world, base, body, format!("remove rules.{name};"), Some(format!("{world}.rules.{name}@{label}"))))
        }
    }
}


/// Several ops as one edit: each is worked out against the file the earlier ones leave (so a positional id means what it
/// means then), their bodies are joined into one patch, and the undo runs them back in the opposite order.
fn op_batch(file: &File, world: &str, ops: &[Op], base: u64) -> Result<Edit, String> {
    if ops.is_empty() {
        return Err("a batch needs at least one edit".to_string());
    }
    let mut current = file.clone();
    let (mut redo, mut undo) = (Vec::new(), Vec::new());
    let mut selected = None;
    for (i, op) in ops.iter().enumerate() {
        let e = op_patch(&current, world, op, base).map_err(|m| format!("edit {} of {}: {m}", i + 1, ops.len()))?;
        let parsed = crate::parse(&e.patch).map_err(|d| format!("edit {}: {}", i + 1, d.message))?;
        current = crate::patch::apply(&current, &parsed).map_err(|p| format!("edit {}: {}: {}", i + 1, p.path, p.message))?;
        redo.push(e.redo);
        undo.push(e.undo);
        selected = e.selected.or(selected);
    }
    undo.reverse();
    Ok(Edit::new(world, base, redo.join(" "), undo.join(" "), selected))
}
