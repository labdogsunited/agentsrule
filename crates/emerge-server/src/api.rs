//! The engine API (spec §12.2): JSON-RPC 2.0 calls on the runs an engine holds. The WebSocket
//! server (`serve`) and the MCP adapter (`mcp`) both call `Engine::call`, so anything one can do
//! the other can.

use crate::journal::Journal;
use crate::run::{Run, Spec};
use emerge_ref::swap::Rejected;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const LANGUAGE: &str = include_str!("../../../language.md");
const EXAMPLE: &str = include_str!("../../../examples/dish.em");
const EXAMPLE_VIEWS: &str = include_str!("../../../examples/views/dish.em");
const LIFE: &str = include_str!("../../../examples/life.em");
const PRESET_VIEWS: &str = include_str!("../../../examples/views/presets.em");
const NURSERY: &str = include_str!("../../../examples/presets/nursery.em");
const LONG_NIGHT: &str = include_str!("../../../examples/presets/long_night.em");
const LONG_NIGHT_HARSH: &str = include_str!("../../../examples/presets/long_night_harsh.em");
const LEAN_TIMES: &str = include_str!("../../../examples/presets/lean_times.em");
const CABINET_LIFE: &str = include_str!("../../../examples/presets/cabinet-life.em");
const CABINET_HIGHLIFE: &str = include_str!("../../../examples/presets/cabinet-highlife.em");
const CABINET_SEEDS: &str = include_str!("../../../examples/presets/cabinet-seeds.em");
const TEST9: &str = include_str!("../../../archive/test9-dish.em");

/// A JSON-RPC error: code, message, and data for a rejected swap.
#[derive(Debug, Clone, PartialEq)]
pub struct Fault {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

fn fault(code: i64, message: impl Into<String>) -> Fault {
    Fault { code, message: message.into(), data: None }
}

/// Invalid parameters.
fn bad(message: impl Into<String>) -> Fault {
    fault(-32602, message)
}

/// A rejected swap: code -32001, with the kind of rejection in `data`.
fn rejected(e: &Rejected) -> Fault {
    let kind = match e {
        Rejected::Stale { .. } => "stale",
        Rejected::Invalid { .. } => "invalid",
        Rejected::NotSwappable(_) => "not_swappable",
        Rejected::Preflight(_) => "preflight",
    };
    let mut data = json!({"kind": kind});
    if let Rejected::Stale { current, .. } = e {
        data["current_revision"] = json!(current);
    }
    Fault { code: -32001, message: e.to_string(), data: Some(data) }
}

struct Entry {
    run: Run,
    /// Ticking in the background (`run.resume`) until paused.
    running: bool,
    /// Where the node editor's boxes sit, by box id (`world.layout_set`); a sidecar, never part of the source.
    layout: serde_json::Map<String, Value>,
    /// Sticky notes and network boxes of the node editor (`proposals/houdini-node-system.md` H1): presentation
    /// only, by id, in the same sidecar as `layout`. Never part of the source, never a revision.
    notes: serde_json::Map<String, Value>,
    frames: serde_json::Map<String, Value>,
    /// The graph edits made, newest last, and the ones undone (`world.undo`, `world.redo`).
    undo: Vec<Step>,
    redo: Vec<Step>,
}

/// One graph edit as the bodies of the patches that undo and redo it (`emerge_lang::graph::Edit`).
#[derive(Clone)]
struct Step {
    what: String,
    undo: String,
    redo: String,
}

/// Applies a graph edit to the run: the patch goes through `hotswap` (checked, preflighted,
/// journalled with `reason`), and on success the edit is remembered so that it can be undone.
fn apply_edit(e: &mut Entry, edit: emerge_lang::graph::Edit, base: u64, reason: &str, what: &str) -> Result<Value, Fault> {
    let mut r = e.run.hotswap(&edit.patch, base, reason).map_err(|r| rejected(&r))?;
    e.undo.push(Step { what: what.to_string(), undo: edit.undo, redo: edit.redo });
    e.redo.clear();
    if let (Some(id), Value::Object(m)) = (edit.selected, &mut r) {
        m.insert("selected".into(), json!(id));
    }
    Ok(r)
}

#[derive(Default)]
pub struct Engine {
    runs: BTreeMap<String, Entry>,
    next: u64,
    /// Where journals are written, as `<world>.jsonl` (set by whoever starts the server). A caller
    /// never names a path: the API can be reached by other programs, and must not write files they choose.
    journal_dir: Option<std::path::PathBuf>,
}

/// The longest one `run.step` call runs, so that a client waiting on it does not time out.
pub const STEP_BUDGET_S: f64 = 30.0;

/// A graph edit as the API takes it: `{"op": "set_param", "id": …, "param": …, "value": …}`,
/// `{"op": "wire", "from": …, "to": …}`, `{"op": "delete", "id": …}` or `{"op": "add_rule", "kind": …}`;
/// and a few words for the journal.
fn graph_op(v: &Value) -> Result<(emerge_lang::graph::Op, String), Fault> {
    use emerge_lang::graph::Op;
    let get = |k: &str| v[k].as_str().map(str::to_string).ok_or_else(|| bad(format!("`op.{k}` must be a string")));
    Ok(match v["op"].as_str() {
        Some("set_param") => (Op::SetParam { id: get("id")?, param: get("param")?, value: get("value")? }, format!("set {} of {}", get("param")?, get("id")?)),
        Some("wire") => (Op::Wire { from: get("from")?, to: get("to")? }, format!("wire {} to {}", get("from")?, get("to")?)),
        Some("delete") => (Op::Delete { id: get("id")? }, format!("delete {}", get("id")?)),
        Some("add_rule") => (Op::AddRule { kind: get("kind")? }, format!("add a {} rule", get("kind")?)),
        _ => return Err(bad("`op.op` must be set_param, wire, delete or add_rule")),
    })
}

fn text<'a>(p: &'a Value, key: &str) -> Result<&'a str, Fault> {
    p[key].as_str().ok_or_else(|| bad(format!("`{key}` must be a string")))
}

fn number(p: &Value, key: &str, default: Option<u64>) -> Result<u64, Fault> {
    match (&p[key], default) {
        (Value::Null, Some(d)) => Ok(d),
        (v, _) => v.as_u64().ok_or_else(|| bad(format!("`{key}` must be a whole number"))),
    }
}

impl Engine {
    pub fn new() -> Engine {
        Engine::default()
    }

    /// An engine that journals every world to `dir/<world>.jsonl`.
    pub fn with_journals(dir: std::path::PathBuf) -> Engine {
        Engine { journal_dir: Some(dir), ..Engine::default() }
    }

    fn entry(&mut self, p: &Value) -> Result<&mut Entry, Fault> {
        let id = text(p, "world")?;
        self.runs.get_mut(id).ok_or_else(|| fault(-32004, format!("no world `{id}`")))
    }

    /// The run of world `id`, to read (pictures, tests).
    pub fn run(&self, id: &str) -> Option<&Run> {
        self.runs.get(id).map(|e| &e.run)
    }

    /// Whether any run is ticking in the background.
    pub fn busy(&self) -> bool {
        self.runs.values().any(|e| e.running)
    }

    /// Ticks every running world for up to `budget` seconds in all (between calls).
    pub fn background(&mut self, budget: f64) {
        let t = std::time::Instant::now();
        while self.busy() && t.elapsed().as_secs_f64() < budget {
            self.tick_running();
        }
    }

    /// One tick of every running world.
    pub fn tick_running(&mut self) {
        for e in self.runs.values_mut().filter(|e| e.running) {
            if e.run.step(1) == 0 {
                e.running = false; // died out
            }
        }
    }

    /// One method call.
    pub fn call(&mut self, method: &str, p: &Value) -> Result<Value, Fault> {
        match method {
            "world.create" => {
                let lineages: Vec<String> = p["lineages"].as_array().ok_or_else(|| bad("`lineages` must be a list of names"))?.iter().filter_map(|l| l.as_str().map(String::from)).collect();
                let backend = p["backend"].as_str().unwrap_or("cpu");
                if backend != "cpu" && backend != "gpu" {
                    return Err(bad("`backend` is `cpu` or `gpu`"));
                }
                let spec = Spec {
                    source: text(p, "source")?.to_string(),
                    lineages,
                    seed: number(p, "seed", Some(1))?,
                    mutation: p["mutation"].as_bool().unwrap_or(true),
                    contract: p["contract"].as_bool().unwrap_or(false),
                    gpu: backend == "gpu",
                };
                if !p["journal"].is_null() {
                    return Err(bad("`journal` is not a parameter: the server writes journals where it was told to (`--journal-dir`)"));
                }
                self.next += 1;
                let id = format!("w{}", self.next);
                let journal = match &self.journal_dir {
                    Some(dir) => {
                        let path = dir.join(format!("{id}.jsonl"));
                        Journal::file(&path).map_err(|e| fault(-32603, format!("journal {}: {e}", path.display())))?
                    }
                    None => Journal::memory(),
                };
                let run = Run::create(spec, journal).map_err(|e| rejected(&e))?;
                let out = json!({"world": id, "revision": 0, "tick": 0, "population": run.population(), "backend": run.device});
                self.runs.insert(id, Entry { run, running: false, layout: Default::default(), notes: Default::default(), frames: Default::default(), undo: Vec::new(), redo: Vec::new() });
                Ok(out)
            }
            "world.validate" => {
                let names: Vec<String> = p["lineages"].as_array().map(|l| l.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                Ok(validate(text(p, "source")?, &names))
            }
            "world.list" => Ok(json!({"worlds": self.runs.iter().map(|(id, e)| json!({
                "world": id, "tick": e.run.tick(), "revision": e.run.revision(), "population": e.run.population(),
                "lineages": e.run.spec.lineages, "running": e.running, "backend": e.run.device,
            })).collect::<Vec<_>>()})),
            "world.source" => {
                let e = self.entry(p)?;
                Ok(json!({"revision": e.run.revision(), "source": e.run.live.source}))
            }
            "world.graph" => {
                // The node graph of the running world's source (`proposals/node-editor.md`): boxes
                // and wires, a projection of the text.
                let e = self.entry(p)?;
                let file = emerge_lang::parse(&e.run.live.source).map_err(|d| fault(-32005, format!("{}: {}", d.code, d.message)))?;
                let world = file.items.iter().find_map(|i| match &i.node {
                    emerge_lang::ast::Item::World(w) => Some(w.name.clone()),
                    _ => None,
                });
                let g = emerge_lang::graph::graph(&file, &world.ok_or_else(|| fault(-32005, "the source has no world".to_string()))?).map_err(|m| fault(-32005, m))?;
                Ok(json!({
                    "revision": e.run.revision(),
                    "world": g.world,
                    "positions": e.layout,
                    "notes": e.notes,
                    "frames": e.frames,
                    "history": {"undo": e.undo.iter().map(|s| s.what.clone()).collect::<Vec<_>>(), "redo": e.redo.iter().map(|s| s.what.clone()).collect::<Vec<_>>()},
                    "nodes": g.nodes.iter().map(|n| json!({"id": n.id, "kind": n.kind.as_str(), "title": n.title, "text": n.text, "step": n.step, "group": n.group, "line": n.line, "params": n.params})).collect::<Vec<_>>(),
                    "wires": g.wires.iter().map(|w| json!({"from": w.from, "to": w.to, "substance": w.substance})).collect::<Vec<_>>(),
                }))
            }
            "rules.lens" => {
                // The checked schedule of the running world's source (`proposals/science-panels.md`
                // §3.1): the steps of one tick in order, each with the rules and statements the
                // checked IR puts there. Static: it says what can happen, not what did.
                let e = self.entry(p)?;
                let file = emerge_lang::parse(&e.run.live.source).map_err(|d| fault(-32005, format!("{}: {}", d.code, d.message)))?;
                let name = file.items.iter().find_map(|i| match &i.node {
                    emerge_lang::ast::Item::World(w) => Some(w.name.clone()),
                    _ => None,
                });
                let name = name.ok_or_else(|| fault(-32005, "the source has no world".to_string()))?;
                let prepared = emerge_lang::prepare(&file).map_err(|d| fault(-32005, format!("{}: {}: {}", d.code, d.path, d.message)))?;
                let sched = emerge_lang::schedule::lower(&prepared, &name).map_err(|m| fault(-32005, m))?;
                let mut placed = Vec::new();
                let mut unplaced = Vec::new();
                for r in &sched.rules {
                    match r.placements() {
                        Ok(parts) => placed.extend(parts.into_iter().map(|(slot, stmts)| (slot, r, stmts))),
                        Err(why) => unplaced.push(json!({"rule": r.name, "on": r.on, "line": r.pos.line, "why": why})),
                    }
                }
                let steps: Vec<Value> = sched.steps.iter().map(|s| {
                    let rules: Vec<Value> = if s.slot == emerge_lang::schedule::Slot::None { vec![] } else {
                        placed.iter().filter(|(slot, _, _)| *slot == s.slot).map(|(_, r, stmts)| json!({
                            "rule": r.name, "on": r.on, "line": r.pos.line,
                            "statements": stmts.iter().map(|st| json!({"text": st.text, "label": st.label, "line": st.pos.line, "path": match &st.label { Some(_) => r.path(&name, st), None => format!("{name}.rules.{}#{}", r.name, r.stmts.iter().position(|x| std::ptr::eq(x, *st)).unwrap_or(0)) }})).collect::<Vec<_>>(),
                        })).collect()
                    };
                    json!({"id": s.id(), "phase": s.phase.name(), "name": s.name, "what": s.what, "reads": s.reads.text(), "settles": s.settles.text(), "creatures": s.creatures, "rules": rules})
                }).collect();
                Ok(json!({"world": name, "revision": e.run.revision(), "tick": e.run.tick(), "creatures": sched.creatures, "steps": steps, "unplaced": unplaced, "waste": sched.waste.iter().map(|(a, b)| json!({"substance": a, "sink": b})).collect::<Vec<_>>(),
                    "disposal": sched.disposal.as_ref().map(|d| json!({"decompose": d.decompose, "remains": d.remains})), "source": "checked schedule (the one `emerge order` prints)"}))
            }
            "rules.activity" => {
                // What each world-rule statement did, counted by the CPU runner since the world began or its rules
                // last changed (`proposals/science-panels.md` §3.3): cells looked at, cells a `when` let through,
                // and for a transfer the asked-for and delivered µ, for a reaction the cells where it ran and its
                // extent. The statements are the runner's laws in the order it built them.
                let e = self.entry(p)?;
                if !matches!(e.run.device.split(':').next(), Some("cpu")) { return Err(fault(-32005, "per-rule counters are kept by the CPU runner only".to_string())); }
                let file = emerge_lang::parse(&e.run.live.source).map_err(|d| fault(-32005, format!("{}: {}", d.code, d.message)))?;
                let prepared = emerge_lang::prepare(&file).map_err(|d| fault(-32005, format!("{}: {}: {}", d.code, d.path, d.message)))?;
                let name = prepared.items.iter().find_map(|i| match &i.node { emerge_lang::ast::Item::World(w) => Some(w.name.clone()), _ => None }).ok_or_else(|| fault(-32005, "the source has no world".to_string()))?;
                let world = prepared.items.iter().find_map(|i| match &i.node { emerge_lang::ast::Item::World(w) if w.name == name => Some(w), _ => None }).unwrap();
                fn walk(block: &emerge_lang::ast::Block, top: usize, rule: &str, on: &str, out: &mut Vec<Value>, name: &str) {
                    let nested = top != usize::MAX;
                    use emerge_lang::ast::Stmt;
                    for (k, st) in block.iter().enumerate() {
                        let top = if top == usize::MAX { k } else { top };
                        match &st.node {
                            // The path is the lens's: a labelled top-level call by its label, anything else by the top-level position.
                            Stmt::Call { call, label } => out.push(json!({"rule": rule, "on": on, "line": st.pos.line, "call": call.name, "text": emerge_lang::fmt::stmt_text(&st.node),
                                "path": match (label, nested) { (Some(l), false) => format!("{name}.rules.{rule}@{l}"), _ => format!("{name}.rules.{rule}#{top}") }})),
                            Stmt::When { then, otherwise, .. } => { walk(then, top, rule, on, out, name); if let Some(o) = otherwise { walk(o, top, rule, on, out, name); } }
                            _ => {}
                        }
                    }
                }
                let mut named: Vec<Value> = vec![];
                for d in &world.decls {
                    if let emerge_lang::ast::WDecl::Rule { name: rn, on, body } = &d.node { walk(body, usize::MAX, rn, on, &mut named, &name); }
                }
                let w = &e.run.live.world;
                if named.len() != w.laws.len() { return Err(fault(-32005, format!("the runner has {} laws but the source has {} effect statements: the counters cannot be tied to statements", w.laws.len(), named.len()))); }
                let ticks = e.run.tick().saturating_sub(w.law_since);
                let rows: Vec<Value> = named.into_iter().zip(&w.law_stats).map(|(mut n, st)| { let o = n.as_object_mut().unwrap();
                    for (k, v) in [("cells", st.cells), ("passed", st.passed), ("requests", st.requests), ("requested_mu", st.requested), ("delivered_mu", st.delivered), ("cut_by_origin", st.cut_by_origin), ("cut_by_dest", st.cut_by_dest), ("ran", st.ran), ("extent", st.extent)] { o.insert(k.into(), json!(v)); }
                    n }).collect();
                Ok(json!({"world": name, "revision": e.run.revision(), "tick": e.run.tick(), "since_tick": w.law_since, "ticks": ticks, "rows": rows,
                    "meaning": {"cells": "cells looked at (every cell, every tick)", "passed": "cells a `when` let through (all of them when there is none)", "requests": "transfer requests made", "requested_mu": "µ asked for", "delivered_mu": "µ that arrived", "cut_by_origin": "requests cut to what the origin held", "cut_by_dest": "requests cut to the room the destination had", "ran": "cells where a reaction had a positive extent", "extent": "total reaction extent in µ of reaction"},
                    "scope": "world rules only; creatures' own reaction genes are in stats.query"}))
            }
            "rules.matrix" => {
                // Which rules touch which declarations (`proposals/science-panels.md` §3.2). Found by
                // name in the checked statements' text, so it is a lower bound: a dependency hidden
                // behind a `let` or an expression is not followed, and a blank cell is "none found".
                let e = self.entry(p)?;
                let file = emerge_lang::parse(&e.run.live.source).map_err(|d| fault(-32005, format!("{}: {}", d.code, d.message)))?;
                let name = file.items.iter().find_map(|i| match &i.node {
                    emerge_lang::ast::Item::World(w) => Some(w.name.clone()),
                    _ => None,
                });
                let name = name.ok_or_else(|| fault(-32005, "the source has no world".to_string()))?;
                let prepared = emerge_lang::prepare(&file).map_err(|d| fault(-32005, format!("{}: {}: {}", d.code, d.path, d.message)))?;
                let sched = emerge_lang::schedule::lower(&prepared, &name).map_err(|m| fault(-32005, m))?;
                let world = prepared.items.iter().find_map(|i| match &i.node {
                    emerge_lang::ast::Item::World(w) if w.name == name => Some(w),
                    _ => None,
                }).ok_or_else(|| fault(-32005, "no world".to_string()))?;
                let mut cols: Vec<(String, &'static str)> = vec![];
                let mut formulas: std::collections::HashMap<String, (Vec<String>, Vec<String>)> = Default::default();
                for d in &world.decls {
                    use emerge_lang::ast::WDecl::*;
                    match &d.node {
                        Substance { name, .. } => cols.push((name.clone(), "substance")),
                        Field { name, .. } => cols.push((name.clone(), "field")),
                        Source { name, .. } => cols.push((name.clone(), "source")),
                        Sink { name, .. } => cols.push((name.clone(), "sink")),
                        Reaction { name, formula, .. } => {
                            cols.push((name.clone(), "reaction"));
                            formulas.insert(name.clone(), (formula.inputs.iter().map(|t| t.name.clone()).collect(), formula.outputs.iter().map(|t| t.name.clone()).collect()));
                        }
                        _ => {}
                    }
                }
                let col_of = |n: &str| cols.iter().position(|(c, _)| c == n);
                let mut rows = Vec::new();
                for r in &sched.rules {
                    for (k, st) in r.stmts.iter().enumerate() {
                        let cells = stmt_cells(&st.text, &col_of, &formulas, &cols);
                        let step = st.slot.as_ref().ok().or(r.slot.as_ref().ok()).and_then(|slot| sched.steps.iter().find(|s| s.slot == *slot)).map(|s| s.name);
                        let path = match &st.label { Some(_) => r.path(&name, st), None => format!("{name}.rules.{}#{k}", r.name) };
                        rows.push(json!({"rule": r.name, "on": r.on, "line": st.pos.line, "path": path, "step": step, "text": st.text,
                            "cells": cells.iter().map(|(c, kinds)| json!({"col": c, "kinds": kinds})).collect::<Vec<_>>(),
                            "opaque": st.text.contains("let ")}));
                    }
                }
                Ok(json!({"world": name, "revision": e.run.revision(), "tick": e.run.tick(),
                    "columns": cols.iter().map(|(n, k)| json!({"name": n, "kind": k})).collect::<Vec<_>>(), "rows": rows,
                    "kinds": {"condition": "read by a `when` condition (start-of-tick snapshot)", "transfer": "the substance moved by a transfer", "from": "source of a transfer", "to": "destination of a transfer", "overflow": "where a transfer's excess goes", "react": "a reaction the rule runs", "consumes": "an input of a reaction the rule runs", "produces": "an output of a reaction the rule runs", "diffuse": "spread between neighbouring cells", "mentions": "named in the statement in another way"},
                    "completeness": "names found in the checked statements; dependencies behind `let` variables or expressions are not followed, so a blank cell means none found, not none exist"}))
            }
            "history.query" => {
                // The sampled counts behind the charts, with their labels: what was sampled, how
                // often, and whether older samples were merged (`proposals/science-panels.md` §5).
                let e = self.entry(p)?;
                let h = &e.run.history;
                Ok(json!({
                    "tick": e.run.tick(), "revision": e.run.revision(),
                    "lineages": e.run.spec.lineages,
                    "every": h.every, "base_every": h.base_every, "thinned": h.every > h.base_every,
                    "kind": "samples of living creatures by founding lineage; births and deaths are the totals between neighbouring samples",
                    "samples": h.samples.iter().map(|s| json!({"tick": s.tick, "revision": s.revision, "counts": s.by_lineage, "births": s.births, "deaths": s.deaths, "drift": s.drift.map(|d| d.to_string())})).collect::<Vec<_>>(),
                }))
            }
            "world.graph_edit" => {
                // Replaces the text of one statement box: the patch is made from the graph's model
                // and goes through the same `hotswap` as any patch (checked, preflighted, journalled).
                let (id, text) = (text(p, "id")?.to_string(), text(p, "text")?.to_string());
                let base = number(p, "base_revision", None)?;
                let e = self.entry(p)?;
                let file = emerge_lang::parse(&e.run.live.source).map_err(|d| fault(-32005, format!("{}: {}", d.code, d.message)))?;
                let world = file.items.iter().find_map(|i| match &i.node {
                    emerge_lang::ast::Item::World(w) => Some(w.name.clone()),
                    _ => None,
                });
                let g = emerge_lang::graph::graph(&file, &world.ok_or_else(|| fault(-32005, "the source has no world".to_string()))?).map_err(|m| fault(-32005, m))?;
                let edit = emerge_lang::graph::edit_text(&g, &id, &text, base).map_err(|m| fault(-32602, m))?;
                // A syntax error is told in terms of the user's text, not of the patch made from it.
                if let Err(d) = emerge_lang::parse(&edit.patch) {
                    if let (1, Some(off)) = (d.line, edit.patch.find(&text)) {
                        let col = d.col.saturating_sub(off);
                        let at = if col == 0 || col > text.chars().count() { "at the end of the text".to_string() } else { format!("at column {col} of the text") };
                        return Err(Fault { code: -32001, message: format!("invalid: {} {at}: {}", d.code, d.message), data: Some(json!({"kind": "invalid"})) });
                    }
                }
                apply_edit(e, edit, base, &format!("graph edit of {id}"), &format!("edit {id}"))
            }
            "world.graph_op" => {
                // One edit of the graph (`emerge_lang::graph::Op`): the patch is made from the model and
                // goes through `hotswap`, so it is checked, preflighted and journalled like any swap.
                let base = number(p, "base_revision", None)?;
                let (op, what) = graph_op(&p["op"])?;
                let e = self.entry(p)?;
                let file = emerge_lang::parse(&e.run.live.source).map_err(|d| fault(-32005, format!("{}: {}", d.code, d.message)))?;
                let world = file.items.iter().find_map(|i| match &i.node {
                    emerge_lang::ast::Item::World(w) => Some(w.name.clone()),
                    _ => None,
                });
                let world = world.ok_or_else(|| fault(-32005, "the source has no world".to_string()))?;
                let edit = emerge_lang::graph::op_patch(&file, &world, &op, base).map_err(|m| fault(-32602, m))?;
                apply_edit(e, edit, base, &format!("graph {what}"), &what)
            }
            "world.undo" | "world.redo" => {
                // Takes back the last graph edit (or does it again): the patch that undoes it, written
                // against the current revision, goes through `hotswap` like any swap. If the world has
                // changed in a way that makes it fail, the edit stays where it was and the reason is given.
                let undoing = method == "world.undo";
                let e = self.entry(p)?;
                let step = (if undoing { e.undo.last() } else { e.redo.last() }).cloned().ok_or_else(|| bad(if undoing { "nothing to undo" } else { "nothing to redo" }))?;
                let file = emerge_lang::parse(&e.run.live.source).map_err(|d| fault(-32005, format!("{}: {}", d.code, d.message)))?;
                let world = file.items.iter().find_map(|i| match &i.node {
                    emerge_lang::ast::Item::World(w) => Some(w.name.clone()),
                    _ => None,
                });
                let world = world.ok_or_else(|| fault(-32005, "the source has no world".to_string()))?;
                let base = e.run.revision();
                let patch = emerge_lang::graph::patch_text(&world, base, if undoing { &step.undo } else { &step.redo });
                let r = e.run.hotswap(&patch, base, &format!("{} {}", if undoing { "undo" } else { "redo" }, step.what)).map_err(|r| rejected(&r))?;
                if undoing {
                    e.undo.pop();
                    e.redo.push(step);
                } else {
                    e.redo.pop();
                    e.undo.push(step);
                }
                Ok(r)
            }
            "world.layout_set" => {
                // Presentation metadata of the node editor, kept beside the world (and in
                // `<journal dir>/<world>.meta.json` when the server journals); the caller never names a
                // file. `positions` merges box positions by id. `notes` and `frames` each replace the
                // whole map: a note is {x, y, text}, a frame {x, y, w, h, label}. Nothing here is a
                // revision or reaches the simulation.
                let id = text(p, "world")?.to_string();
                let nums = |v: &Value, keys: &[&str]| keys.iter().all(|k| v[*k].as_f64().is_some_and(f64::is_finite));
                let mut positions = serde_json::Map::new();
                if !p["positions"].is_null() {
                    for (k, v) in p["positions"].as_object().ok_or_else(|| bad("`positions` must be an object of id: [x, y]"))? {
                        let ok = v.as_array().is_some_and(|a| a.len() == 2 && a.iter().all(|n| n.as_f64().is_some_and(f64::is_finite)));
                        if !ok {
                            return Err(bad(format!("`{k}` must be [x, y]")));
                        }
                        positions.insert(k.clone(), v.clone());
                    }
                }
                let checked = |key: &str, keys: &[&str], texts: &[(&str, usize)]| -> Result<Option<serde_json::Map<String, Value>>, Fault> {
                    if p[key].is_null() {
                        return Ok(None);
                    }
                    let m = p[key].as_object().ok_or_else(|| bad(format!("`{key}` must be an object of id: item")))?;
                    if m.len() > 500 {
                        return Err(bad(format!("`{key}` holds at most 500 items")));
                    }
                    for (k, v) in m {
                        if !nums(v, keys) {
                            return Err(bad(format!("{key} `{k}` needs finite numbers for {}", keys.join(", "))));
                        }
                        for (field, max) in texts {
                            if !v[*field].as_str().is_some_and(|t| t.chars().count() <= *max) {
                                return Err(bad(format!("{key} `{k}` needs `{field}` as text of at most {max} characters")));
                            }
                        }
                    }
                    Ok(Some(m.clone()))
                };
                let notes = checked("notes", &["x", "y"], &[("text", 2000)])?;
                let frames = checked("frames", &["x", "y", "w", "h"], &[("label", 200)])?;
                let dir = self.journal_dir.clone();
                let e = self.entry(p)?;
                for (k, v) in positions {
                    e.layout.insert(k, v);
                }
                if let Some(n) = notes {
                    e.notes = n;
                }
                if let Some(f) = frames {
                    e.frames = f;
                }
                if let Some(dir) = dir {
                    let body = serde_json::to_string_pretty(&json!({"world": id, "positions": e.layout, "notes": e.notes, "frames": e.frames})).unwrap_or_default();
                    std::fs::write(dir.join(format!("{id}.meta.json")), body).map_err(|err| fault(-32603, format!("could not write the layout: {err}")))?;
                }
                Ok(json!({"saved": e.layout.len(), "notes": e.notes.len(), "frames": e.frames.len()}))
            }
            "world.hotswap" => {
                let (patch, reason) = (text(p, "patch")?.to_string(), p["reason"].as_str().unwrap_or("").to_string());
                let base = number(p, "base_revision", None)?;
                let e = self.entry(p)?;
                e.run.hotswap(&patch, base, &reason).map_err(|r| rejected(&r))
            }
            "run.step" => {
                let ticks = number(p, "ticks", Some(1))?;
                let e = self.entry(p)?;
                let t = std::time::Instant::now();
                let mut done = 0;
                while done < ticks && t.elapsed().as_secs_f64() < STEP_BUDGET_S {
                    let n = e.run.step((ticks - done).min(10));
                    done += n;
                    if n == 0 {
                        break; // died out
                    }
                }
                let mut out = json!({"ticks_run": done, "tick": e.run.tick(), "population": e.run.population()});
                if done < ticks && e.run.alive() {
                    out["note"] = json!(format!("stopped after {STEP_BUDGET_S} s; call again to run on"));
                }
                Ok(out)
            }
            "run.resume" | "run.pause" => {
                let e = self.entry(p)?;
                e.running = method == "run.resume" && e.run.alive();
                Ok(json!({"running": e.running, "tick": e.run.tick()}))
            }
            "stats.query" => Ok(self.entry(p)?.run.stats()),
            "view.snapshot" => {
                let agents = number(p, "agents", Some(200))? as usize;
                let substance = p["substance"].as_str().unwrap_or("body").to_string();
                self.entry(p)?.run.snapshot(&substance, agents).map_err(bad)
            }
            "entities.query" => {
                let limit = number(p, "limit", Some(20))? as usize;
                self.entry(p)?.run.entities(limit).map_err(bad)
            }
            "genome.explain" => {
                let e = self.entry(p)?;
                let id = match &p["entity"] {
                    Value::Null => e.run.first_entity().ok_or_else(|| bad("the world is empty"))?,
                    v => v.as_u64().ok_or_else(|| bad("`entity` must be an entity ID"))?,
                };
                Ok(json!({"entity": id, "explanation": e.run.explain(id).map_err(bad)?}))
            }
            "view.pick" => {
                // What is at a point of the world: the cell's amounts and the creatures near it.
                let coord = |k: &str| p[k].as_f64().map(|v| v as f32).ok_or_else(|| bad(format!("`{k}` must be a number")));
                let (x, y) = (coord("x")?, coord("y")?);
                let radius = p["radius"].as_f64().map_or(6.0, |v| v as f32).clamp(0.0, 1000.0);
                let limit = number(p, "limit", Some(8))?.min(50) as usize;
                self.entry(p)?.run.pick(x, y, radius, limit).map_err(bad)
            }
            "genome.graph" => {
                // One entity's genome as boxes (genes, in modules) and signals, for the node editor's
                // genome view; read-only.
                let e = self.entry(p)?;
                let id = match &p["entity"] {
                    Value::Null => e.run.first_entity().ok_or_else(|| bad("the world is empty"))?,
                    v => v.as_u64().ok_or_else(|| bad("`entity` must be an entity ID"))?,
                };
                let (lineage, age, g) = e.run.genome_graph(id).map_err(bad)?;
                Ok(json!({
                    "entity": id, "lineage": lineage, "age": age, "tick": e.run.tick(),
                    "modules": g.modules,
                    "keep": g.keep,
                    "genes": g.genes.iter().map(|x| json!({"index": x.index, "module": x.module, "kind": x.kind, "title": x.title, "text": x.text, "expressed": x.expressed, "active": x.active, "reads": x.reads, "writes": x.writes})).collect::<Vec<_>>(),
                }))
            }
            "accounts.start" => {
                let clade = match &p["clade"] { Value::Null => None, v => Some(v.as_u64().ok_or_else(|| bad("`clade` must be an entity ID"))?) };
                self.entry(p)?.run.start_accounts(clade).map_err(bad)
            }
            "accounts.query" => self.entry(p)?.run.accounts().map_err(bad),
            "ancestry.query" => {
                let id = match &p["entity"] {
                    Value::Null => self.entry(p)?.run.first_entity().ok_or_else(|| bad("the world is empty"))?,
                    v => v.as_u64().ok_or_else(|| bad("`entity` must be an entity ID"))?,
                };
                let limit = p["limit"].as_u64().unwrap_or(200).min(5000) as usize;
                self.entry(p)?.run.ancestry(id, limit).map_err(bad)
            }
            "runs.compare" => {
                // Two worlds side by side, read-only (`proposals/science-panels.md` §12): what differs in how
                // they were made, and their population histories on the ticks both sampled.
                let (ia, ib) = (text(p, "a")?.to_string(), text(p, "b")?.to_string());
                let get = |id: &str| self.runs.get(id).ok_or_else(|| fault(-32004, format!("no world `{id}`")));
                let (ra, rb) = (&get(&ia)?.run, &get(&ib)?.run);
                let desc = |r: &crate::run::Run| json!({"seed": r.spec.seed, "mutation": r.spec.mutation, "backend": r.device, "contract": r.spec.contract, "revision": r.revision(), "tick": r.tick(), "lineages": r.spec.lineages,
                    "source_lines": r.live.source.lines().count(), "every": r.history.every});
                let (sa, sb) = (&ra.spec, &rb.spec);
                let mut differs: Vec<String> = vec![];
                if sa.seed != sb.seed { differs.push(format!("seed ({} against {})", sa.seed, sb.seed)); }
                if sa.mutation != sb.mutation { differs.push("mutation".into()); }
                if ra.device != rb.device { differs.push(format!("backend ({} against {})", ra.device, rb.device)); }
                if ra.live.source != rb.live.source { differs.push(format!("source ({} lines changed or moved)", ra.live.source.lines().filter(|l| !rb.live.source.lines().any(|m| m == *l)).count() + rb.live.source.lines().filter(|l| !ra.live.source.lines().any(|m| m == *l)).count())); }
                if sa.lineages != sb.lineages { differs.push("lineages".into()); }
                if ra.revision() != 0 || rb.revision() != 0 { differs.push("a world was edited after creation (its samples carry revisions)".into()); }
                let ticks_b: std::collections::HashSet<u64> = rb.history.samples.iter().map(|x| x.tick).collect();
                let common: Vec<&emerge_view::frame::Sample> = ra.history.samples.iter().filter(|x| ticks_b.contains(&x.tick)).collect();
                let at = |r: &crate::run::Run, tick: u64, name: &str| -> Value {
                    let i = r.spec.lineages.iter().position(|l| l == name);
                    match (r.history.samples.iter().find(|x| x.tick == tick), i) { (Some(x), Some(i)) => json!(x.by_lineage[i]), _ => Value::Null }
                };
                let mut names: Vec<String> = sa.lineages.clone();
                for n in &sb.lineages { if !names.contains(n) { names.push(n.clone()); } }
                let series: Vec<Value> = names.iter().map(|n| json!({"lineage": n, "ticks": common.iter().map(|x| x.tick).collect::<Vec<_>>(),
                    "a": common.iter().map(|x| at(ra, x.tick, n)).collect::<Vec<_>>(), "b": common.iter().map(|x| at(rb, x.tick, n)).collect::<Vec<_>>()})).collect();
                let paired = sa.seed == sb.seed && ra.live.source == rb.live.source && sa.mutation == sb.mutation && ra.device == rb.device;
                Ok(json!({"a": {"world": ia, "run": desc(ra)}, "b": {"world": ib, "run": desc(rb)}, "differs": differs,
                    "reading": if paired { "same source, seed, mutation and backend: any difference between them is not explained by the setup" }
                               else if differs.is_empty() { "no setup difference found" }
                               else { "these are single trials that differ in the settings listed; one run each shows what happened, not how likely it is" },
                    "common_samples": common.len(), "samples_a": ra.history.samples.len(), "samples_b": rb.history.samples.len(),
                    "sampling": "only ticks both runs sampled are compared; counts are living creatures by founding lineage", "series": series,
                    "worlds_kept": self.runs.keys().collect::<Vec<_>>()}))
            }
            "region.query" => {
                let r = |k: &str| -> Result<[f32; 2], Fault> {
                    let a = p[k].as_array().filter(|a| a.len() == 2).ok_or_else(|| bad(format!("`{k}` must be [from, to] in world units")))?;
                    Ok([a[0].as_f64().ok_or_else(|| bad(format!("`{k}` must be numbers")))? as f32, a[1].as_f64().ok_or_else(|| bad(format!("`{k}` must be numbers")))? as f32])
                };
                let (x, y) = (r("x")?, r("y")?);
                self.entry(p)?.run.region(x, y).map_err(bad)
            }
            "genome.atlas" => {
                let clade = match &p["clade"] { Value::Null => None, v => Some(v.as_u64().ok_or_else(|| bad("`clade` must be an entity ID"))?) };
                self.entry(p)?.run.atlas(clade).map_err(bad)
            }
            "accounts.import" => {
                // A `--accounts` file of the CPU runner, read as the same budget the live panel shows: the file's
                // manifest, then every group's account lines summed over the windows from `from` to `to`.
                // The file holds no stocks, so nothing can be reconciled; the explained change is all it gives.
                let csv = text(p, "csv")?;
                let (from, to) = (p["from"].as_u64().unwrap_or(0), p["to"].as_u64().unwrap_or(u64::MAX));
                let mut manifest = String::new();
                let mut head: Option<Vec<usize>> = None;
                let (mut groups, mut order): (std::collections::HashMap<String, ([u64; emerge_ref::world::ACCOUNT_COLUMNS.len()], u64, u64, u64)>, Vec<String>) = Default::default();
                let (mut first, mut last) = (u64::MAX, 0);
                for line in csv.lines() {
                    if let Some(c) = line.strip_prefix("# ") { if manifest.is_empty() { manifest = c.to_string(); } continue; }
                    let cells: Vec<&str> = line.split(',').collect();
                    if head.is_none() {
                        if cells.len() < 3 || cells[0] != "tick" { return Err(bad("not an accounts file: the header must begin `tick,group,entities`")); }
                        let idx: Option<Vec<usize>> = emerge_ref::world::ACCOUNT_COLUMNS.iter().map(|c| cells.iter().position(|h| h == c)).collect();
                        head = Some(idx.ok_or_else(|| bad(format!("the file's columns are not this build's account catalogue: {}", cells[3..].join(","))))?);
                        continue;
                    }
                    let idx = head.as_ref().unwrap();
                    let tick: u64 = cells.first().and_then(|t| t.parse().ok()).ok_or_else(|| bad(format!("bad row: {line}")))?;
                    if tick < from || tick > to || cells.len() != idx.len() + 3 { if cells.len() != idx.len() + 3 { return Err(bad(format!("a row has {} values, the header {}", cells.len(), idx.len() + 3))); } continue; }
                    first = first.min(tick); last = last.max(tick);
                    let g = cells[1].to_string();
                    if !groups.contains_key(&g) { order.push(g.clone()); }
                    let e = groups.entry(g).or_insert(([0; emerge_ref::world::ACCOUNT_COLUMNS.len()], 0, 0, 0));
                    for (k, &i) in idx.iter().enumerate() { e.0[k] += cells[i].parse::<u64>().map_err(|_| bad(format!("bad number `{}`", cells[i])))?; }
                    e.1 = cells[2].parse().unwrap_or(0);
                    e.2 += 1;
                    e.3 = tick;
                }
                if head.is_none() { return Err(bad("the file has no header")); }
                let list: Vec<Value> = order.iter().map(|g| { let (t, n, windows, _) = &groups[g]; let lines = crate::run::budget_lines(t);
                    json!({"group": g, "entities_at_last_window": n, "windows": windows, "lines": lines.iter().map(|(l, s, x)| json!({"name": l, "sign": s, "mu": x})).collect::<Vec<_>>(),
                        "explained_mu": lines.iter().map(|(_, s, x)| s * x).sum::<i128>(), "births": t[0], "starved": t[1], "aged": t[2]}) }).collect();
                Ok(json!({"manifest": manifest, "from_tick": if first == u64::MAX { Value::Null } else { json!(first) }, "to_tick": last, "unit": "µ", "groups": list,
                    "unreconciled": "not available: the file holds counters only, not the stocks at the ends of the interval"}))
            }
            "genome.census" => Ok(self.entry(p)?.run.census()),
            "genome.compare" => {
                // Two living entities' genes side by side, matched by kind and text; read-only. Genes in
                // both are listed once with their activity in each.
                let e = self.entry(p)?;
                let id = |k: &str| p[k].as_u64().ok_or_else(|| bad(format!("`{k}` must be an entity ID")));
                let (a, b) = (id("a")?, id("b")?);
                let (la, _, ga) = e.run.genome_graph(a).map_err(bad)?;
                let (lb, _, gb) = e.run.genome_graph(b).map_err(bad)?;
                let key = |g: &emerge_ref::print::GeneBox| format!("{}\u{1}{}", g.kind, g.text);
                let mut rest: Vec<Option<&emerge_ref::print::GeneBox>> = gb.genes.iter().map(Some).collect();
                let (mut shared, mut only_a) = (vec![], vec![]);
                for g in &ga.genes {
                    match rest.iter().position(|o| o.is_some_and(|o| key(o) == key(g))) {
                        Some(i) => {
                            let o = rest[i].take().unwrap();
                            shared.push(json!({"kind": g.kind, "title": g.title, "text": g.text, "a": {"index": g.index, "expressed": g.expressed, "active": g.active}, "b": {"index": o.index, "expressed": o.expressed, "active": o.active}}));
                        }
                        None => only_a.push(json!({"index": g.index, "kind": g.kind, "title": g.title, "text": g.text, "expressed": g.expressed, "active": g.active})),
                    }
                }
                // Mutation-aware: an unmatched gene of A and one of B with the same kind and name are one gene
                // that changed (a parameter differs), paired in gene order.
                let mut changed = vec![];
                let mut left_a = vec![];
                for g in only_a.drain(..) {
                    let hit = rest.iter().position(|o| o.is_some_and(|o| o.kind == g["kind"].as_str().unwrap_or("") && o.title == g["title"].as_str().unwrap_or("")));
                    match hit {
                        Some(i) => { let o = rest[i].take().unwrap(); changed.push(json!({"kind": o.kind, "title": o.title, "a": g, "b": {"index": o.index, "text": o.text, "expressed": o.expressed, "active": o.active}})); }
                        None => left_a.push(g),
                    }
                }
                let only_a = left_a;
                let only_b: Vec<Value> = rest.iter().flatten().map(|g| json!({"index": g.index, "kind": g.kind, "title": g.title, "text": g.text, "expressed": g.expressed, "active": g.active})).collect();
                Ok(json!({"tick": e.run.tick(), "a": {"entity": a, "lineage": la, "genes": ga.genes.len()}, "b": {"entity": b, "lineage": lb, "genes": gb.genes.len()},
                    "matching": "genes match when kind and text are equal; then unmatched genes with the same kind and name pair as changed (a parameter differs); repeated genes pair off one to one", "shared": shared, "changed": changed, "only_a": only_a, "only_b": only_b}))
            }
            "journal.read" => Ok(json!({"lines": self.entry(p)?.run.journal.lines})),
            "view.palettes" => {
                // The palettes a view may name, each as 17 colours sampled along it (a categorical one
                // as its own colours), so a page can draw a ramp without knowing the renderer's tables.
                use emerge_lang::view::PaletteKind;
                let list: Vec<Value> = emerge_lang::view::PALETTES.iter().map(|(name, kind)| {
                    let colours: Vec<[u8; 3]> = if *kind == PaletteKind::Categorical { (0..10).map(emerge_view::palette::category).collect() } else { (0..17).map(|i| emerge_view::palette::sample(name, i as f64 / 16.0)).collect() };
                    json!({"name": name, "kind": format!("{kind:?}").to_lowercase(), "colours": colours})
                }).collect();
                Ok(json!({"palettes": list}))
            }
            "docs.query" => Ok(docs(p["query"].as_str().unwrap_or(""))),
            "docs.examples" => {
                let dish_views = format!("{EXAMPLE_VIEWS}\n{PRESET_VIEWS}");
                Ok(json!({"examples": [
                {"id": "dish", "name": "Reference dish", "about": "chemistry, a conserved field, world rules, menus and two lineages (Drifter, Builder)", "source": EXAMPLE, "views": EXAMPLE_VIEWS, "lineages": ["Drifter", "Builder"], "resizable": true},
                {"id": "life", "name": "Game of Life (glider gun)", "about": "a state field with `life_like`: no creatures, no chemistry; runs on the CPU", "source": LIFE, "views": "", "lineages": [], "resizable": false},
                {"id": "test9", "name": "Test-9 dish", "about": "the dish of test 9: Drifter must find body synthesis (a body reaction plus take(nitrate))", "source": TEST9, "views": EXAMPLE_VIEWS, "lineages": ["Drifter", "Builder"], "resizable": true},
                {"id": "nursery", "name": "Sunlit Nursery", "preset": {"version": 1, "group": "presets", "file": "examples/presets/nursery.em", "needs": "creatures", "controls": [
                    {"label": "light supply", "target": "rule sunlight, transfer @transfer_light, amount", "live": true, "unit": "mu per cell per tick", "range": [0, 200], "default": 40, "patch": "set rules.sunlight@transfer_light.amount = {v}mu;"},
                    {"label": "mineral supply", "target": "rule weather, transfer @transfer_nitrate, amount", "live": true, "unit": "mu per cell per tick", "range": [0, 100], "default": 10, "patch": "set rules.weather@transfer_nitrate.amount = {v}mu;", "note": "no visible effect through tick 600 (seeds 1-3 equal at 3mu and 0mu): the initial 20u of nitrate per cell is not used up by then"},
                    {"label": "diffusion", "target": "rule spread, diffuse_conserved, rate", "live": true, "unit": "bp", "range": [0, 1000], "default": 300, "patch": "set rules.spread@diffuse_conserved.rate = {v}bp;"},
                    {"label": "initial population", "target": "lineage Builder genesis count", "live": false, "unit": "creatures", "range": [1, 20000], "default": 2000}],
                    "watch": "population, births and deaths, energy carried; compare the Nitrate and Biolume views", "verified": "seed 1, CPU, mutation off, 1000 ticks: births in every window, 4_000 to 9_000 creatures. Light supply halved (20mu) gives about 40 percent fewer creatures at tick 400 in seeds 1-3; the mineral control showed no effect through tick 600. The successful metabolism was supplied in the seed genome; this is not a discovery."},
                 "about": "a luminous ecosystem with a working metabolism (Builder, mutation off): what limits population growth?", "source": NURSERY, "views": dish_views, "lineages": ["Builder"], "resizable": false, "mutation": false, "backend": "cpu"},
                {"id": "long_night", "name": "The Long Night", "preset": {"version": 1, "group": "presets", "file": "examples/presets/long_night.em", "needs": "creatures and tick conditions", "controls": [
                    {"label": "light supply outside the night", "target": "rule sunlight, transfer @transfer_light, amount", "live": true, "unit": "mu per cell per tick", "range": [0, 200], "default": 40, "patch": "set rules.sunlight@transfer_light.amount = {v}mu;"},
                    {"label": "blackout start", "target": "rule sunlight, when tick < 300", "live": false, "unit": "tick", "default": 300, "note": "the bound is not a labelled statement, so a patch cannot reach it; edit the source and restart"},
                    {"label": "blackout end", "target": "rule sunlight, when tick >= 400", "live": false, "unit": "tick", "default": 400, "note": "as for the start"}],
                    "watch": "population through ticks 300 to 400 and the recovery after; the Light view", "verified": "seeds 1-3, CPU, mutation off, 1000 ticks: about 6_000 creatures at tick 300, 8 to 15 at tick 400, 5_500 to 6_500 at tick 1000. Changes after the start affect future ticks only; restart to compare."},
                 "about": "sunlight stops for ticks 300 to 399: who survives on stored resources?", "source": LONG_NIGHT, "views": dish_views, "lineages": ["Builder"], "resizable": false, "mutation": false, "backend": "cpu"},
                {"id": "long_night_harsh", "name": "The Long Night (harsh)", "preset": {"version": 1, "group": "presets", "file": "examples/presets/long_night_harsh.em", "needs": "creatures and tick conditions", "controls": [
                    {"label": "blackout end", "target": "rule sunlight, when tick >= 450", "live": false, "unit": "tick", "default": 450}],
                    "watch": "the population reaching zero", "verified": "seeds 1-3, CPU, mutation off: extinct by tick 425 in every seed. Kept as the harsher setting; no rescue injections."},
                 "about": "the same world with a night to tick 449: extinction", "source": LONG_NIGHT_HARSH, "views": dish_views, "lineages": ["Builder"], "resizable": false, "mutation": false, "backend": "cpu"},
                {"id": "lean_times", "name": "Lean Times", "preset": {"version": 1, "group": "presets", "file": "examples/presets/lean_times.em", "needs": "creatures", "controls": [
                    {"label": "base upkeep", "target": "upkeep base", "live": false},
                    {"label": "per-gene upkeep", "target": "upkeep per_gene", "live": false},
                    {"label": "per-signal upkeep", "target": "upkeep per_signal", "live": false}],
                    "watch": "lineage shares over time; open both genomes side by side", "verified": "archived test-6 world reproduced, seed 1, CPU, mutation off: Bloated share 0.23 at tick 100, 0.0 at tick 600. Its eight extra genes are unused on purpose; not evidence that complexity is a liability in general."},
                 "about": "Builder against Bloated (Builder plus eight unused senses): what does unneeded machinery cost?", "source": LEAN_TIMES, "views": dish_views, "lineages": ["Builder", "Bloated"], "resizable": false, "mutation": false, "backend": "cpu"},
                {"id": "cabinet_life", "name": "Automata Cabinet: Conway soup", "preset": {"version": 1, "group": "presets", "file": "examples/presets/cabinet-life.em", "needs": "state fields", "controls": [
                    {"label": "birth counts", "target": "rule conway, life_like born", "live": false},
                    {"label": "survival counts", "target": "rule conway, life_like survives", "live": false},
                    {"label": "scatter density", "target": "genesis scatter density", "live": false}],
                    "watch": "the field settling; step one tick at a time", "verified": "parses, checks and runs on the CPU; outcomes not characterised."},
                 "about": "Conway rules from a random soup on a wrapping 64 x 64 grid (CPU only)", "source": CABINET_LIFE, "views": "", "lineages": [], "resizable": false, "mutation": false, "backend": "cpu"},
                {"id": "cabinet_highlife", "name": "Automata Cabinet: birth on 3 and 6", "preset": {"version": 1, "group": "presets", "file": "examples/presets/cabinet-highlife.em", "needs": "state fields", "controls": [
                    {"label": "birth counts", "target": "rule conway, life_like born", "live": false}],
                    "watch": "compare with the Conway soup from the same seed", "verified": "parses, checks and runs on the CPU; outcomes not characterised."},
                 "about": "one extra birth count (6) over Conway, same soup", "source": CABINET_HIGHLIFE, "views": "", "lineages": [], "resizable": false, "mutation": false, "backend": "cpu"},
                {"id": "cabinet_seeds", "name": "Automata Cabinet: births on 2", "preset": {"version": 1, "group": "presets", "file": "examples/presets/cabinet-seeds.em", "needs": "state fields", "controls": [
                    {"label": "birth counts", "target": "rule conway, life_like born", "live": false}],
                    "watch": "a sparse soup where nothing survives", "verified": "parses, checks and runs on the CPU; outcomes not characterised."},
                 "about": "birth on 2 neighbours, no survival, sparse soup", "source": CABINET_SEEDS, "views": "", "lineages": [], "resizable": false, "mutation": false, "backend": "cpu"},
            ]}))},
            "docs.example" => Ok(json!({"name": "examples/dish.em", "about": "the reference world: chemistry, a conserved field, world rules, menus and two lineages (Drifter, Builder); a new world is easiest written as a changed copy of it", "source": EXAMPLE, "views": EXAMPLE_VIEWS})),
            _ => Err(fault(-32601, format!("no method `{method}`"))),
        }
    }

    /// One JSON-RPC 2.0 message (a request or a notification); the reply, if one is due.
    pub fn handle(&mut self, message: &str) -> Option<String> {
        let req: Value = match serde_json::from_str(message) {
            Ok(v) => v,
            Err(e) => return Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}}).to_string()),
        };
        let id = req.get("id").cloned();
        let method = req["method"].as_str().unwrap_or("");
        let result = self.call(method, &req["params"]);
        let id = id?;
        Some(match result {
            Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
            Err(f) => {
                let mut err = json!({"code": f.code, "message": f.message});
                if let Some(d) = f.data {
                    err["data"] = d;
                }
                json!({"jsonrpc": "2.0", "id": id, "error": err})
            }
        }
        .to_string())
    }
}

/// Parses, checks and compiles a source without running it: the diagnostics, and the parts a run
/// would leave out.
pub fn validate(source: &str, lineages: &[String]) -> Value {
    let file = match emerge_lang::parse(source) {
        Ok(f) => f,
        Err(d) => return json!({"valid": false, "errors": [{"code": d.code, "line": d.line, "col": d.col, "message": d.message}]}),
    };
    let problems: Vec<Value> = emerge_lang::check::check(&file).iter().map(|p| json!({"code": p.code, "path": p.path, "line": p.pos.line, "col": p.pos.col, "message": p.message, "fix": p.fix})).collect();
    if !problems.is_empty() {
        return json!({"valid": false, "errors": problems});
    }
    let warnings: Vec<Value> = emerge_lang::prepare(&file).map(|f| emerge_lang::view::check(&f).1).unwrap_or_default().iter().map(|p| json!({"code": p.code, "path": p.path, "message": p.message})).collect();
    let names: Vec<&str> = if lineages.is_empty() { emerge_ref::compile::lineage_names(&file) } else { lineages.iter().map(String::as_str).collect() };
    match emerge_ref::compile::compile(source, &names, emerge_ref::compile::Options { seed: 1, allow_partial: true, mutation: true }) {
        Ok(c) => json!({"valid": true, "lineages": names, "left_out": c.partial.iter().map(|e| format!("{}: {}", e.path, e.message)).collect::<Vec<_>>(), "warnings": warnings, "notes": c.notes}),
        Err(e) => json!({"valid": false, "errors": [{"code": e.code, "path": e.path, "line": e.pos.line, "col": e.pos.col, "message": e.message}]}),
    }
}

/// Sections of the language reference whose heading or text mentions `query` (all headings if empty).
pub fn docs(query: &str) -> Value {
    let mut sections: Vec<(String, String)> = Vec::new();
    for line in LANGUAGE.lines() {
        if line.starts_with("## ") || line.starts_with("### ") {
            sections.push((line.trim_start_matches('#').trim().to_string(), String::new()));
        } else if let Some(s) = sections.last_mut() {
            s.1 += line;
            s.1 += "\n";
        }
    }
    if query.is_empty() {
        return json!({"sections": sections.iter().map(|s| &s.0).collect::<Vec<_>>(), "hint": "pass `query` to get the text of the sections that mention it"});
    }
    let q = query.to_lowercase();
    let hits: Vec<Value> = sections.iter().filter(|(h, t)| h.to_lowercase().contains(&q) || t.to_lowercase().contains(&q)).take(4).map(|(h, t)| json!({"section": h, "text": t.trim()})).collect();
    json!({"query": query, "sections": hits})
}

/// The declarations a rule statement names and how (`rules.matrix`): by name in its one-line text.
fn stmt_cells(text: &str, col_of: &dyn Fn(&str) -> Option<usize>, formulas: &std::collections::HashMap<String, (Vec<String>, Vec<String>)>, cols: &[(String, &'static str)]) -> Vec<(String, Vec<&'static str>)> {
    let mut out: Vec<(usize, &'static str)> = vec![];
    let ids = |seg: &str| -> Vec<String> { seg.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|t| !t.is_empty()).map(String::from).collect() };
    let mut add = |n: &str, kind: &'static str, out: &mut Vec<(usize, &'static str)>| if let Some(c) = col_of(n) { if !out.contains(&(c, kind)) { out.push((c, kind)); } };
    let (cond, body) = match text.strip_prefix("when ") { Some(rest) => match rest.split_once('{') { Some((c, b)) => (c, b), None => (rest, "") }, None => ("", text) };
    for n in ids(cond) { add(&n, "condition", &mut out); }
    // Each call: the first argument, and the `from:`, `to:` and `overflow:` names.
    for (call, kind) in [("transfer(", "transfer"), ("react(", "react"), ("diffuse_conserved([", "diffuse")] {
        let mut rest = body;
        while let Some(i) = rest.find(call) {
            rest = &rest[i + call.len()..];
            let end = rest.find(if kind == "diffuse" { ']' } else { ')' }).unwrap_or(rest.len());
            let args = &rest[..end];
            if kind == "diffuse" { for n in ids(args) { add(&n, kind, &mut out); } continue; }
            let first = args.split(',').next().unwrap_or("");
            for n in ids(first) { add(&n, kind, &mut out); if kind == "react" { if let Some((ins, outs)) = formulas.get(&n) { ins.iter().for_each(|x| add(x, "consumes", &mut out)); outs.iter().for_each(|x| add(x, "produces", &mut out)); } } }
            for (key, k) in [("from:", "from"), ("to:", "to"), ("overflow:", "overflow")] {
                if let Some(j) = args.find(key) { if let Some(n) = ids(args[j + key.len()..].split(',').next().unwrap_or("")).first() { add(n, k, &mut out); } }
            }
        }
    }
    let seen: Vec<usize> = out.iter().map(|(c, _)| *c).collect();
    for n in ids(body) { if let Some(c) = col_of(&n) { if !seen.contains(&c) { add(&n, "mentions", &mut out); } } }
    let mut by: Vec<(String, Vec<&'static str>)> = vec![];
    for (c, k) in out {
        match by.iter_mut().find(|(n, _)| *n == cols[c].0) { Some((_, v)) => v.push(k), None => by.push((cols[c].0.clone(), vec![k])) }
    }
    by
}
