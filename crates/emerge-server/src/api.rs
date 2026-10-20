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

    fn entry(&mut self, p: &Value) -> Result<&mut Entry, Fault> {
        let id = text(p, "world")?;
        self.runs.get_mut(id).ok_or_else(|| fault(-32004, format!("no world `{id}`")))
    }

    /// Whether any run is ticking in the background.
    pub fn busy(&self) -> bool {
        self.runs.values().any(|e| e.running)
    }

    /// Ticks every running world for up to `budget` seconds in all (between calls).
    pub fn background(&mut self, budget: f64) {
        let t = std::time::Instant::now();
        while self.busy() && t.elapsed().as_secs_f64() < budget {
            for e in self.runs.values_mut().filter(|e| e.running) {
                if e.run.step(1) == 0 {
                    e.running = false; // died out
                }
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
                let journal = match p["journal"].as_str() {
                    Some(path) => Journal::file(path.as_ref()).map_err(|e| fault(-32603, format!("journal `{path}`: {e}")))?,
                    None => Journal::memory(),
                };
                let run = Run::create(spec, journal).map_err(|e| rejected(&e))?;
                self.next += 1;
                let id = format!("w{}", self.next);
                let out = json!({"world": id, "revision": 0, "tick": 0, "population": run.population(), "backend": run.device});
                self.runs.insert(id, Entry { run, running: false });
                Ok(out)
            }
            "world.validate" => {
                let names: Vec<String> = p["lineages"].as_array().map(|l| l.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                Ok(validate(text(p, "source")?, &names))
            }
            "world.source" => {
                let e = self.entry(p)?;
                Ok(json!({"revision": e.run.revision(), "source": e.run.live.source}))
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
                let done = e.run.step(ticks);
                Ok(json!({"ticks_run": done, "tick": e.run.tick(), "population": e.run.population()}))
            }
            "run.resume" | "run.pause" => {
                let e = self.entry(p)?;
                e.running = method == "run.resume" && e.run.population() > 0;
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
            "docs.example" => Ok(json!({"name": "examples/dish.em", "about": "the reference world: chemistry, a conserved field, world rules, menus and two lineages (Drifter, Builder); a new world is easiest written as a changed copy of it", "source": EXAMPLE})),
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
    let names: Vec<&str> = if lineages.is_empty() { emerge_ref::compile::lineage_names(&file) } else { lineages.iter().map(String::as_str).collect() };
    match emerge_ref::compile::compile(source, &names, emerge_ref::compile::Options { seed: 1, allow_partial: true, mutation: true }) {
        Ok(c) => json!({"valid": true, "lineages": names, "left_out": c.partial.iter().map(|e| format!("{}: {}", e.path, e.message)).collect::<Vec<_>>(), "notes": c.notes}),
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
