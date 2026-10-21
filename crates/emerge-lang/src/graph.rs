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
use crate::fmt::{decl_text, stmt_text};
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
        nodes.push(GNode { id: rule_id.clone(), kind, title: name_of(&d.node), text: decl_text(&d.node), step: None, group: None, line: d.pos.line });

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
            });
            let Stmt::Call { call, .. } = &st.node else { continue };
            match kind {
                Kind::Transfer => {
                    let s = subject(call).into_iter().next();
                    let from = arg_name(call, "from").and_then(|n| target(&n));
                    let to = arg_name(call, "to").and_then(|n| target(&n));
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

/// The patch that replaces the text of one statement box (`proposals/node-editor.md` §6, N2):
/// `replace rules.R@label { … }` for a labelled statement, and for one without a label the rule's
/// whole body with that statement swapped. `base` is the revision the graph was drawn at. Only
/// statements can be edited; the other boxes are not yet (a reaction or a store has no `replace`).
pub fn edit_patch(g: &Graph, id: &str, text: &str, base: u64) -> Result<String, String> {
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
    Ok(format!("patch {} base {base} {{ {body} }}", g.world))
}
