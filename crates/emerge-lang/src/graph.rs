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
    /// `use NAME version N as I(…)`: an instance of an operator, its bindings as wires and its value parameters as
    /// parameters (`proposals/reusable-operators.md`).
    Instance,
    /// `operator NAME version N { … }`: a definition.
    Operator,
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
            Kind::Instance => "instance",
            Kind::Operator => "operator",
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
        WDecl::Operator { .. } => "operators",
        WDecl::Use { .. } => "uses",
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
        // A definition is its name and version (`operators.NutrientSupply@v1`); an instance is its own name.
        WDecl::Operator { name, version, .. } => format!("{name}@v{version}"),
        WDecl::Use { name, .. } => name.clone(),
    }
}

/// The parameters of operator `def` version `version` of `w` (none if it is not there).
pub fn operator_params(w: &World, def: &str, version: i64) -> Vec<OParam> {
    w.decls.iter().find_map(|d| match &d.node {
        WDecl::Operator { name, version: v, params, .. } if name == def && *v == version => Some(params.iter().map(|p| p.node.clone()).collect()),
        _ => None,
    }).unwrap_or_default()
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
            WDecl::Use { .. } => Kind::Instance,
            WDecl::Operator { .. } => Kind::Operator,
            _ => Kind::Decl,
        };
        let rule_id = id(&d.node);
        if let WDecl::Use { def, version, name, args } = &d.node {
            // An instance: every parameter of its definition, as given or as its default (`(default)` after it), and a
            // wire for each declaration it is bound to: what it takes from in, what it fills out.
            let params = operator_params(w, def, *version);
            let given = |k: &str| args.iter().find(|a| a.name.as_deref() == Some(k)).map(|a| value_text(&a.value));
            let shown: Vec<(String, String)> = params.iter().map(|p| (p.name.clone(), given(&p.name).or_else(|| p.default.as_ref().map(value_text)).unwrap_or_default())).collect();
            let substances: Vec<&String> = params.iter().filter(|p| p.kind == "substance").filter_map(|p| shown.iter().find(|(k, _)| *k == p.name).map(|(_, v)| v)).collect();
            let carried = (substances.len() == 1).then(|| substances[0].clone());
            for p in &params {
                let Some((_, bound)) = shown.iter().find(|(k, _)| *k == p.name) else { continue };
                match p.kind.as_str() {
                    "source" => if let Some(s) = store(bound) { wires.push(Wire { from: s, to: rule_id.clone(), substance: carried.clone() }) },
                    "field" | "sink" => if let Some(s) = store(bound) { wires.push(Wire { from: rule_id.clone(), to: s, substance: carried.clone() }) },
                    "reaction" => if let Some((r, _)) = reaction(bound) { wires.push(Wire { from: r, to: rule_id.clone(), substance: None }) },
                    _ => {}
                }
            }
            nodes.push(GNode { id: rule_id, kind, title: format!("{name} · {def} v{version}"), text: decl_text(&d.node), step: None, group: None, line: d.pos.line, params: shown });
            continue;
        }
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
                Stmt::When { .. } => (Kind::Code, "when".to_string()),
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
    // A top-level statement (a `when` to add to, or whose condition to set) is its own container, with no way down.
    let Some((container, path)) = id.split_once('/') else { return Ok((id, Vec::new())) };
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

/// What changes in a nested statement: its text, one argument, its subject, or the statement itself; or, for a `when`
/// (top-level or nested), its condition or a statement added to one of its blocks.
enum NestedChange {
    Text(String),
    Arg(String, String),
    Subject(String),
    Delete,
    Condition(String),
    Add { otherwise: bool, text: String },
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
            // An `else` left empty goes with its last statement.
            if last.0 && otherwise.as_ref().is_some_and(|b| b.is_empty()) {
                *otherwise = None;
            }
        }
        NestedChange::Condition(text) => {
            let Stmt::When { cond, .. } = nested_mut(&mut top.node, &route).ok_or_else(gone)? else { return Err(format!("`{id}` is not a `when`: only a `when` has a condition")) };
            match parse_value(&text)? {
                Value::Expr(e) => *cond = e,
                _ => return Err("a condition is an expression".to_string()),
            }
        }
        NestedChange::Add { otherwise: other, text } => {
            let stmts = parse_block(&text)?;
            if stmts.is_empty() {
                return Err("nothing to add: type a statement".to_string());
            }
            let Stmt::When { then, otherwise, .. } = nested_mut(&mut top.node, &route).ok_or_else(gone)? else { return Err(format!("`{id}` is not a `when`: statements are added inside a `when`")) };
            let block = if other { otherwise.get_or_insert_with(Vec::new) } else { then };
            block.extend(stmts.into_iter().map(|mut n| { n.trivia = Default::default(); n }));
        }
        NestedChange::Subject(name) => {
            let Stmt::Call { call, .. } = nested_mut(&mut top.node, &route).ok_or_else(gone)? else { return Err(format!("`{id}` is not a call")) };
            put_arg(call, None, name_value(&name));
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
    /// Statements added at the end of a `when`'s `then` block, or of its `else` (made if it has none): `id` is the `when`,
    /// a top-level one (`R#2`) or one inside another (`R#2/t0`). One revision; the way back is the rule as it was.
    AddInside { id: String, otherwise: bool, text: String },
    /// A `when`'s condition, replaced by `cond` (source text of an expression), checked like any edit.
    Condition { id: String, cond: String },
    /// An instance replaced by the rules it writes out, in its place: ordinary source from then on.
    Detach { id: String },
    /// The instance's definition copied as the next version of it in the world, with `text` (the definition's text, its
    /// version number set by the edit), and the instance moved to it. No other instance changes.
    Fork { id: String, text: String },
    /// The instance moved to `version` of its definition; `def` is that version's text when the world does not have it
    /// yet (from the library). A version no instance uses any more is taken away in the same patch.
    Upgrade { id: String, version: i64, def: Option<String> },
    /// An instance of the definition `def` (its text) named `name`, with `args`: the definition is added to the world if
    /// it is not there yet (the same name and version with other content is refused).
    InsertOperator { def: String, name: String, args: Vec<(String, String)> },
    /// The rules that hold `ids` made into the operator `name` version `version` and an instance `instance` of it in
    /// their place (`reuse::operator_from`).
    MakeOperator { ids: Vec<String>, name: String, instance: String, version: i64 },
    /// New rules, each from (a name to start from, its field, its statements as text), with fresh names after the last
    /// rule: a template inserted.
    AddRules { rules: Vec<(String, String, String)> },
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

    // An instance by its box id: its place among the declarations and the parts of its `use`.
    let instance = |id: &str| -> Result<(usize, String, i64, String, Vec<Arg>), String> {
        w.decls.iter().enumerate().find_map(|(i, d)| match &d.node {
            WDecl::Use { def, version, name, args } if format!("{world}.uses.{name}") == id => Some((i, def.clone(), *version, name.clone(), args.clone())),
            _ => None,
        }).ok_or_else(|| format!("no instance `{id}`"))
    };
    // Where something goes back to the place of declaration `i` among the rules and instances.
    let anchor = |i: usize, skip: &[&str]| -> String {
        let key = |d: &WDecl| match d { WDecl::Rule { name, .. } if !skip.contains(&name.as_str()) => Some(format!("rules.{name}")), WDecl::Use { name, .. } if !skip.contains(&name.as_str()) => Some(format!("uses.{name}")), _ => None };
        if let Some(p) = w.decls[..i].iter().rev().find_map(|d| key(&d.node)) { return format!("add after {p}: "); }
        if let Some(n) = w.decls[i + 1..].iter().find_map(|d| key(&d.node)) { return format!("add before {n}: "); }
        "add ".to_string()
    };
    let use_text = |def: &str, version: i64, name: &str, args: &[Arg]| decl_text(&WDecl::Use { def: def.to_string(), version, name: name.to_string(), args: args.to_vec() });
    let parse_decl = |text: &str| -> Result<WDecl, String> {
        let f = crate::parse(&format!("world T {{ {text} }}")).map_err(|d| format!("{}: {}", d.code, d.message))?;
        f.items.into_iter().find_map(|i| match i.node { Item::World(w) => w.decls.into_iter().next().map(|d| d.node), _ => None }).ok_or_else(|| "not a declaration".to_string())
    };
    let versions_of = |name: &str| -> Vec<i64> { w.decls.iter().filter_map(|d| match &d.node { WDecl::Operator { name: n, version, .. } if n == name => Some(*version), _ => None }).collect() };
    let def_text = |name: &str, version: i64| -> Option<String> { w.decls.iter().find_map(|d| match &d.node { WDecl::Operator { name: n, version: v, .. } if n == name && *v == version => Some(decl_text(&d.node)), _ => None }) };
    let users = |name: &str, version: i64| w.decls.iter().filter(|d| matches!(&d.node, WDecl::Use { def, version: v, .. } if def == name && *v == version)).count();
    match op {
        Op::SetParam { id, param, value } if id.starts_with(&format!("{world}.uses.")) => {
            let (_, _, _, name, args) = instance(id)?;
            parse_value(value)?;
            let old = args.iter().find(|a| a.name.as_deref() == Some(param.as_str())).map(|a| value_text(&a.value));
            let undo = match old { Some(v) => format!("set uses.{name}.{param} = {v};"), None => format!("remove uses.{name}.{param};") };
            Ok(Edit::new(world, base, format!("set uses.{name}.{param} = {value};"), undo, Some(id.clone())))
        }
        Op::Wire { from, to } if from.starts_with(&format!("{world}.uses.")) || to.starts_with(&format!("{world}.uses.")) => {
            // A declaration wired to an instance binds the one parameter of its kind; with several, say which in Parameters.
            let (inst, other) = if to.starts_with(&format!("{world}.uses.")) { (to, from) } else { (from, to) };
            let (_, def, version, _, _) = instance(inst)?;
            let kind = other.strip_prefix(&prefix).and_then(|r| r.split('.').next()).map(|k| match k { "sources" => "source", "sinks" => "sink", "fields" => "field", "reactions" => "reaction", _ => "" }).unwrap_or("");
            let fit: Vec<String> = operator_params(w, &def, version).into_iter().filter(|p| p.kind == kind).map(|p| p.name).collect();
            match fit.as_slice() {
                [one] => op_patch(file, world, &Op::SetParam { id: inst.clone(), param: one.clone(), value: short(other).to_string() }, base),
                [] => Err(format!("`{def}` has no {} parameter for `{}`", if kind.is_empty() { "such" } else { kind }, short(other))),
                many => Err(format!("`{def}` has several {kind} parameters ({}): set the one you mean in Parameters", many.join(", "))),
            }
        }
        Op::Delete { id } if id.starts_with(&format!("{world}.uses.")) => {
            let (i, def, version, name, args) = instance(id)?;
            Ok(Edit::new(world, base, format!("remove uses.{name};"), format!("{}{}", anchor(i, &[&name]), use_text(&def, version, &name, &args)), None))
        }
        Op::Detach { id } => {
            let (i, def, version, name, args) = instance(id)?;
            let prepared = crate::prepare(file).map_err(|p| format!("{}: {}: {}", p.code, p.path, p.message))?;
            let pw = prepared.items.iter().find_map(|it| match &it.node { Item::World(x) if x.name == world => Some(x), _ => None }).ok_or("no world")?;
            let inst = name.clone();
            let def_rules: Vec<String> = w.decls.iter().find_map(|d| match &d.node {
                WDecl::Operator { name: n, version: v, rules, .. } if *n == def && *v == version => Some(rules.iter().filter_map(|r| match &r.node { WDecl::Rule { name: rule, .. } => Some(format!("{inst}_{rule}")), _ => None }).collect()),
                _ => None,
            }).ok_or_else(|| format!("`{def}` version {version} is not in the world"))?;
            let texts: Vec<(String, String)> = def_rules.iter().filter_map(|r| pw.decls.iter().find(|d| matches!(&d.node, WDecl::Rule { name: n, .. } if n == r)).map(|d| (r.clone(), decl_text(&d.node)))).collect();
            let mut redo = vec![format!("remove uses.{name};")];
            let mut place = anchor(i, &[&name]);
            for (r, t) in &texts {
                redo.push(format!("{place}{t}"));
                place = format!("add after rules.{r}: ");
            }
            let mut undo: Vec<String> = texts.iter().map(|(r, _)| format!("remove rules.{r};")).collect();
            undo.push(format!("{}{}", anchor(i, &[&name]), use_text(&def, version, &name, &args)));
            Ok(Edit::new(world, base, redo.join(" "), undo.join(" "), texts.first().map(|(r, _)| format!("{world}.rules.{r}"))))
        }
        Op::Fork { id, text } => {
            let (_, def, version, name, _) = instance(id)?;
            let WDecl::Operator { name: n, params, rules, .. } = parse_decl(text)? else { return Err("the text is not an operator definition".into()) };
            if n != def {
                return Err(format!("a new version of `{def}` keeps its name (the text names `{n}`)"));
            }
            let next = versions_of(&def).into_iter().max().unwrap_or(version) + 1;
            let new = decl_text(&WDecl::Operator { name: def.clone(), version: next, params, rules });
            Ok(Edit::new(world, base, format!("add {new} set uses.{name}.version = {next};"), format!("set uses.{name}.version = {version}; remove operators.{def}@v{next};"), Some(id.clone())))
        }
        Op::Upgrade { id, version: to, def: given } => {
            let (_, def, from, name, _) = instance(id)?;
            if *to == from {
                return Err(format!("`{name}` already uses version {to}"));
            }
            let (mut redo, mut undo) = (Vec::new(), Vec::new());
            match (def_text(&def, *to), given) {
                (Some(have), Some(g)) if crate::parse(&format!("world T {{ {g} }}")).ok().map(|f| crate::format(&f)) != crate::parse(&format!("world T {{ {have} }}")).ok().map(|f| crate::format(&f)) => {
                    return Err(format!("the world's `{def}` version {to} is not this one: publish it as a new version first"));
                }
                (Some(_), _) => {}
                (None, Some(g)) => {
                    let WDecl::Operator { name: n, version: v, .. } = parse_decl(g)? else { return Err("not an operator definition".into()) };
                    if n != def || v != *to {
                        return Err(format!("the definition given is `{n}` version {v}, not `{def}` version {to}"));
                    }
                    redo.push(format!("add {g}"));
                    undo.push(format!("remove operators.{def}@v{to};"));
                }
                (None, None) => return Err(format!("the world has no `{def}` version {to}")),
            }
            redo.push(format!("set uses.{name}.version = {to};"));
            undo.insert(0, format!("set uses.{name}.version = {from};"));
            // The old version goes if this instance was its last user.
            if users(&def, from) == 1 {
                let old = def_text(&def, from).unwrap_or_default();
                redo.push(format!("remove operators.{def}@v{from};"));
                undo.insert(0, format!("add {old}"));
            }
            Ok(Edit::new(world, base, redo.join(" "), undo.join(" "), Some(id.clone())))
        }
        Op::InsertOperator { def, name, args } => {
            let WDecl::Operator { name: d, version, params, .. } = parse_decl(def)? else { return Err("not an operator definition".into()) };
            let canon = |t: &str| crate::parse(&format!("world T {{ {t} }}")).ok().map(|f| crate::format(&f));
            let mut redo = Vec::new();
            let mut undo = Vec::new();
            match def_text(&d, version) {
                Some(have) if canon(&have) != canon(def) => return Err(format!("this world's `{d}` version {version} is not the library's: publish one of them as a new version")),
                Some(_) => {}
                None => {
                    redo.push(format!("add {def}"));
                    undo.push(format!("remove operators.{d}@v{version};"));
                }
            }
            let taken: Vec<String> = w.decls.iter().filter_map(|x| match &x.node { WDecl::Use { name, .. } => Some(name.clone()), WDecl::Rule { name, .. } => Some(name.clone()), _ => None }).collect();
            let stem: String = d.chars().enumerate().flat_map(|(i, c)| if c.is_ascii_uppercase() && i > 0 { vec!['_', c.to_ascii_lowercase()] } else { vec![c.to_ascii_lowercase()] }).collect();
            let inst = if name.is_empty() { (1..).map(|k| format!("{stem}_{k}")).find(|n| !taken.contains(n)).unwrap() } else { name.clone() };
            let mut a = Vec::new();
            for (k, v) in args {
                if !params.iter().any(|p| p.node.name == *k) {
                    return Err(format!("`{d}` has no parameter `{k}`"));
                }
                a.push(Arg { name: Some(k.clone()), value: parse_value(v)? });
            }
            let last = w.decls.iter().rev().find_map(|x| match &x.node { WDecl::Rule { name, .. } => Some(format!("add after rules.{name}: ")), WDecl::Use { name, .. } => Some(format!("add after uses.{name}: ")), _ => None }).unwrap_or_else(|| "add ".into());
            redo.push(format!("{last}{}", use_text(&d, version, &inst, &a)));
            undo.insert(0, format!("remove uses.{inst};"));
            Ok(Edit::new(world, base, redo.join(" "), undo.join(" "), Some(format!("{world}.uses.{inst}"))))
        }
        Op::MakeOperator { ids, name, instance: inst, version } => {
            let (d, rules, args) = crate::reuse::operator_from(file, world, ids, name, *version)?;
            if !versions_of(name).is_empty() && versions_of(name).contains(version) {
                return Err(format!("the world already has `{name}` version {version}"));
            }
            let first = w.decls.iter().position(|x| matches!(&x.node, WDecl::Rule { name, .. } if *name == rules[0])).unwrap();
            let skip: Vec<&str> = rules.iter().map(String::as_str).collect();
            let place = anchor(first, &skip);
            let mut redo: Vec<String> = rules.iter().map(|r| format!("remove rules.{r};")).collect();
            redo.push(format!("add {}", decl_text(&d)));
            redo.push(format!("{place}{}", use_text(name, *version, inst, &args)));
            let mut undo = vec![format!("remove uses.{inst};"), format!("remove operators.{name}@v{version};")];
            let mut back = place;
            for r in &rules {
                let text = w.decls.iter().find(|x| matches!(&x.node, WDecl::Rule { name, .. } if name == r)).map(|x| decl_text(&x.node)).unwrap_or_default();
                undo.push(format!("{back}{text}"));
                back = format!("add after rules.{r}: ");
            }
            Ok(Edit::new(world, base, redo.join(" "), undo.join(" "), Some(format!("{world}.uses.{inst}"))))
        }
        Op::AddRules { rules } => {
            if rules.is_empty() {
                return Err("no rules to add".into());
            }
            let mut taken: Vec<String> = w.decls.iter().filter_map(|d| crate::check::declared(&d.node).map(|(n, _)| n.to_string())).collect();
            let fields: Vec<&str> = w.decls.iter().filter_map(|d| match &d.node { WDecl::Field { name, .. } => Some(name.as_str()), _ => None }).collect();
            let mut place = w.decls.iter().rev().find_map(|d| match &d.node { WDecl::Rule { name, .. } => Some(format!("rules.{name}")), WDecl::Use { name, .. } => Some(format!("uses.{name}")), _ => None });
            let (mut redo, mut undo) = (Vec::new(), Vec::new());
            let mut selected = None;
            for (hint, on, text) in rules {
                if !fields.contains(&on.as_str()) {
                    return Err(format!("this world has no field `{on}` for the rule to work on"));
                }
                let body = parse_block(text)?;
                let n = (1..).map(|k| if k == 1 { hint.clone() } else { format!("{hint}_{k}") }).find(|n| !taken.contains(n)).unwrap();
                taken.push(n.clone());
                let at = match &place { Some(p) => format!("add after {p}: "), None => "add ".to_string() };
                redo.push(format!("{at}rule {n} on {on} {{ {} }}", body.iter().map(|s| stmt_text(&s.node)).collect::<Vec<_>>().join(" ")));
                undo.push(format!("remove rules.{n};"));
                selected.get_or_insert_with(|| match body.first().map(|s| &s.node) { Some(Stmt::Call { label: Some(l), .. }) => format!("{world}.rules.{n}@{l}"), _ => format!("{world}.rules.{n}#0") });
                place = Some(format!("rules.{n}"));
            }
            undo.reverse();
            Ok(Edit::new(world, base, redo.join(" "), undo.join(" "), selected))
        }
        Op::AddInside { id, otherwise, text } => nested_edit(file, world, id, NestedChange::Add { otherwise: *otherwise, text: text.clone() }, base),
        Op::Condition { id, cond } => nested_edit(file, world, id, NestedChange::Condition(cond.clone()), base),
        Op::Wire { from, to } if from.contains('/') || to.contains('/') => {
            // A wire to or from a statement inside a `when`: the same three connections as at the top level.
            let kind_of = |id: &str| -> Result<Kind, String> {
                if id.contains('/') {
                    let c = crate::contract::contract(file, world)?;
                    c.nested.iter().find(|n| n.id == id).map(|n| n.kind).ok_or_else(|| format!("no box `{id}`"))
                } else {
                    node(id).map(|n| n.kind)
                }
            };
            let on = |id: &str| -> Option<String> {
                let rule = id.strip_prefix(&format!("{world}.rules."))?.split(['#', '@']).next()?;
                w.decls.iter().find_map(|d| match &d.node { WDecl::Rule { name, on, .. } if name == rule => Some(on.clone()), _ => None })
            };
            match (kind_of(from)?, kind_of(to)?) {
                (Kind::Store, Kind::Transfer) => nested_edit(file, world, to, NestedChange::Arg("from".into(), short(from).into()), base),
                (Kind::Transfer, Kind::Store) => {
                    let store = short(to);
                    let v = if on(from).as_deref() == Some(store) { "self" } else { store };
                    nested_edit(file, world, from, NestedChange::Arg("to".into(), v.into()), base)
                }
                (Kind::Reaction, Kind::React) => nested_edit(file, world, to, NestedChange::Subject(short(from).into()), base),
                _ => Err("these cannot be wired: a store goes to a transfer, a transfer to a store, a reaction to a react".to_string()),
            }
        }
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
