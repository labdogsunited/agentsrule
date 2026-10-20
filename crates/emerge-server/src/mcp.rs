//! The MCP adapter (spec §14): MCP tools over stdio, each one a call of the engine API (`api.rs`),
//! so an AI client can do exactly what the API can. Dual-era, as the MCP specification allows: a
//! client that opens with `initialize` is served under a handshake revision (2025-11-25 and
//! earlier), and one that sends `server/discover` or per-request `_meta` under 2026-07-28.
//! One JSON-RPC message per line; stdout carries nothing else; the server exits at end of input.

use crate::api::Engine;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const MODERN: &[&str] = &["2026-07-28"];
const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";

const INSTRUCTIONS: &str = "Emerge runs evolving creatures in worlds written in the `.em` language. \
To make a world, read `docs.example` (the reference world) and `docs.query` (the language), write a source, \
check it with `world.validate`, then `world.create` it; keep the returned `world` ID for the other tools. \
`run.step` advances it; `stats.query`, `view.snapshot`, `entities.query` and `genome.explain` observe it. \
`world.hotswap` changes it while it runs: send a patch (`patch Dish { set rules.NAME@LABEL.amount = 30mu; }`) \
with the world's current `base_revision`; a rejected swap changes nothing and says why.";

fn schema(props: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": required})
}

/// The tools: one per API method, with the method's name.
pub fn tools() -> Vec<Value> {
    let world = json!({"type": "string", "description": "the world ID returned by world.create"});
    let t = |name: &str, description: &str, input: Value| json!({"name": name, "description": description, "inputSchema": input});
    vec![
        t("docs.example", "The reference world `examples/dish.em` in full: a starting point for writing a world.", json!({"type": "object", "additionalProperties": false})),
        t("docs.query", "Sections of the `.em` language reference that mention `query`; with no query, the list of sections.", schema(json!({"query": {"type": "string"}}), &[])),
        t("world.validate", "Parse, check and compile a `.em` source without running it: errors with codes and positions, and the parts a run would leave out.", schema(json!({"source": {"type": "string"}, "lineages": {"type": "array", "items": {"type": "string"}}}), &["source"])),
        t("world.create", "Compile a `.em` source and start a world at tick 0, revision 0, seeded with the named lineages. Returns its `world` ID.", schema(json!({"source": {"type": "string"}, "lineages": {"type": "array", "items": {"type": "string"}}, "seed": {"type": "integer"}, "mutation": {"type": "boolean", "description": "children vary (default true)"}, "backend": {"type": "string", "enum": ["cpu", "gpu"]}}), &["source", "lineages"])),
        t("world.source", "The world's current source and revision.", schema(json!({"world": world}), &["world"])),
        t("world.hotswap", "Change a running world with a patch, between ticks. Needs the current revision as `base_revision`; a stale, invalid, not swappable or preflight-failing patch is rejected and changes nothing.", schema(json!({"world": world, "base_revision": {"type": "integer"}, "patch": {"type": "string", "description": "a `patch Dish { ... }` item"}, "reason": {"type": "string", "description": "why, for the journal"}}), &["world", "base_revision", "patch"])),
        t("run.step", "Run the world for `ticks` ticks (default 1); stops early if it dies out.", schema(json!({"world": world, "ticks": {"type": "integer"}}), &["world"])),
        t("stats.query", "Tick, revision, population (by lineage), births, deaths, mean amounts carried, entity reaction extent.", schema(json!({"world": world}), &["world"])),
        t("view.snapshot", "One substance of the field over the grid, and a sample of entities with positions and headings, stamped with tick and revision.", schema(json!({"world": world, "substance": {"type": "string"}, "agents": {"type": "integer"}}), &["world"])),
        t("entities.query", "A sample of entities: ID, lineage, age, position, active genes, what they carry.", schema(json!({"world": world, "limit": {"type": "integer"}}), &["world"])),
        t("genome.explain", "An entity's genome in plain language, module by module, with its signal network.", schema(json!({"world": world, "entity": {"type": "integer"}}), &["world"])),
        t("journal.read", "The world's journal: its header and every swap, committed or rejected.", schema(json!({"world": world}), &["world"])),
    ]
}

/// Serves one MCP client: requests in, replies out, until the input ends.
pub struct Server {
    engine: Engine,
    /// The revision a handshake client agreed on, if it opened with `initialize`.
    legacy: Option<String>,
}

impl Default for Server {
    fn default() -> Self {
        Server { engine: Engine::new(), legacy: None }
    }
}

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: &Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut e = json!({"code": code, "message": message});
    if let Some(d) = data {
        e["data"] = d;
    }
    json!({"jsonrpc": "2.0", "id": id, "error": e})
}

impl Server {
    pub fn new() -> Server {
        Server::default()
    }

    /// One message; the reply, if one is due (notifications get none).
    pub fn handle(&mut self, line: &str) -> Option<Value> {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => return Some(error(&Value::Null, -32700, &format!("parse error: {e}"), None)),
        };
        let method = msg["method"].as_str().unwrap_or("").to_string();
        let id = msg.get("id").cloned()?; // a notification (initialized, cancelled) needs no answer
        let p = &msg["params"];
        // A modern request names its version; it must be one this server speaks.
        let modern = p["_meta"].get(VERSION_KEY).and_then(Value::as_str).map(String::from);
        if let Some(v) = &modern {
            if !MODERN.contains(&v.as_str()) {
                let all: Vec<&str> = MODERN.iter().chain(LEGACY).copied().collect();
                return Some(error(&id, -32022, "Unsupported protocol version", Some(json!({"supported": all, "requested": v}))));
            }
        }
        let complete = |mut v: Value| {
            if modern.is_some() {
                v["resultType"] = json!("complete");
            }
            v
        };
        let server_info = json!({"name": "emerge-server", "version": env!("CARGO_PKG_VERSION")});
        Some(match method.as_str() {
            "initialize" => {
                let asked = p["protocolVersion"].as_str().unwrap_or("");
                let version = if LEGACY.contains(&asked) { asked } else { LEGACY[0] };
                self.legacy = Some(version.to_string());
                reply(&id, json!({"protocolVersion": version, "capabilities": {"tools": {}}, "serverInfo": server_info, "instructions": INSTRUCTIONS}))
            }
            "server/discover" => {
                let all: Vec<&str> = MODERN.iter().chain(LEGACY).copied().collect();
                reply(&id, json!({"resultType": "complete", "supportedVersions": all, "capabilities": {"tools": {}}, "_meta": {"io.modelcontextprotocol/serverInfo": server_info}, "instructions": INSTRUCTIONS}))
            }
            "ping" => reply(&id, complete(json!({}))),
            "tools/list" => reply(&id, complete(json!({"tools": tools()}))),
            "tools/call" => {
                let name = p["name"].as_str().unwrap_or("");
                if !tools().iter().any(|t| t["name"] == name) {
                    return Some(error(&id, -32602, &format!("Unknown tool: {name}"), None));
                }
                let args = if p["arguments"].is_null() { json!({}) } else { p["arguments"].clone() };
                // An engine fault is a tool execution error: the model can read it and try again.
                let result = match self.engine.call(name, &args) {
                    Ok(v) => json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default()}], "structuredContent": v, "isError": false}),
                    Err(f) => {
                        let text = match &f.data {
                            Some(d) => format!("{} ({d})", f.message),
                            None => f.message.clone(),
                        };
                        json!({"content": [{"type": "text", "text": text}], "isError": true})
                    }
                };
                reply(&id, complete(result))
            }
            _ => error(&id, -32601, &format!("Method not found: {method}"), None),
        })
    }

    /// Ticks running worlds between requests (see `run.step` for stepping on request).
    pub fn idle(&mut self) {
        if self.engine.busy() {
            self.engine.background(0.02);
        }
    }

    pub fn busy(&self) -> bool {
        self.engine.busy()
    }
}

/// The stdio transport: a reader thread hands lines over, so running worlds can tick meanwhile.
pub fn stdio() {
    let (lines, input) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if lines.send(line).is_err() {
                return;
            }
        }
    });
    let mut server = Server::new();
    let mut out = std::io::stdout().lock();
    loop {
        let line = if server.busy() {
            match input.try_recv() {
                Ok(l) => l,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    server.idle();
                    continue;
                }
                Err(_) => return,
            }
        } else {
            match input.recv() {
                Ok(l) => l,
                Err(_) => return, // end of input: the client is done
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(r) = server.handle(&line) {
            if writeln!(out, "{r}").and_then(|_| out.flush()).is_err() {
                return;
            }
        }
    }
}
