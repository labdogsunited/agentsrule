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
                self.runs.insert(id, Entry { run, running: false });
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
                    "nodes": g.nodes.iter().map(|n| json!({"id": n.id, "kind": n.kind.as_str(), "title": n.title, "text": n.text, "step": n.step, "group": n.group, "line": n.line})).collect::<Vec<_>>(),
                    "wires": g.wires.iter().map(|w| json!({"from": w.from, "to": w.to, "substance": w.substance})).collect::<Vec<_>>(),
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
                let patch = emerge_lang::graph::edit_patch(&g, &id, &text, base).map_err(|m| fault(-32602, m))?;
                // A syntax error is told in terms of the user's text, not of the patch made from it.
                if let Err(d) = emerge_lang::parse(&patch) {
                    if let (1, Some(off)) = (d.line, patch.find(&text)) {
                        let col = d.col.saturating_sub(off);
                        let at = if col == 0 || col > text.chars().count() { "at the end of the text".to_string() } else { format!("at column {col} of the text") };
                        return Err(Fault { code: -32001, message: format!("invalid: {} {at}: {}", d.code, d.message), data: Some(json!({"kind": "invalid"})) });
                    }
                }
                e.run.hotswap(&patch, base, &format!("graph edit of {id}")).map_err(|r| rejected(&r))
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
                    Value::Null => e.run.live.world.entities.first().map(|x| x.id).ok_or_else(|| bad("the world is empty"))?,
                    v => v.as_u64().ok_or_else(|| bad("`entity` must be an entity ID"))?,
                };
                Ok(json!({"entity": id, "explanation": e.run.explain(id).map_err(bad)?}))
            }
            "journal.read" => Ok(json!({"lines": self.entry(p)?.run.journal.lines})),
            "docs.query" => Ok(docs(p["query"].as_str().unwrap_or(""))),
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
