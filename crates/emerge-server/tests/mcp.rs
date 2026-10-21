//! The MCP adapter over stdio (`proposals/engine-api.md` H6) and exit test 11's script: through MCP
//! only, a client writes a world from a description, runs it, reads stats and a snapshot, hot-swaps
//! a rule and explains a genome. The script plays the AI's part with fixed text; a real model is not
//! called. Both protocol eras are checked. CPU runner.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    id: u64,
    meta: Option<Value>,
}

impl Mcp {
    fn start(modern: bool) -> Mcp {
        let mut child = Command::new(env!("CARGO_BIN_EXE_emerge-server")).arg("mcp").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let (stdin, stdout) = (child.stdin.take().unwrap(), BufReader::new(child.stdout.take().unwrap()));
        let meta = modern.then(|| json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"}, "io.modelcontextprotocol/clientCapabilities": {}}));
        Mcp { child, stdin, stdout, id: 0, meta }
    }
    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }
    fn request(&mut self, method: &str, mut params: Value) -> Value {
        self.id += 1;
        if let Some(m) = &self.meta {
            params["_meta"] = m.clone();
        }
        self.send(json!({"jsonrpc": "2.0", "id": self.id, "method": method, "params": params}));
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("not one JSON message per line ({e}): {line}"));
        assert_eq!(v["id"], self.id);
        v
    }
    /// A tool call that must succeed; its structured result.
    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let v = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        assert_eq!(v["result"]["isError"], false, "{name}: {v}");
        let r = &v["result"];
        let text: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, r["structuredContent"], "the text content repeats the structured result");
        r["structuredContent"].clone()
    }
    fn finish(mut self) {
        drop(self.stdin);
        let status = self.child.wait().unwrap();
        assert!(status.success(), "the server exits cleanly when its input closes");
    }
}

#[test]
fn exit_test_11_through_mcp_only() {
    let mut m = Mcp::start(false);
    let init = m.request("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "script", "version": "1"}}));
    assert_eq!((init["result"]["protocolVersion"].clone(), init["result"]["capabilities"]["tools"].is_object()), (json!("2025-06-18"), true));
    m.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let tools = m.request("tools/list", json!({}))["result"]["tools"].as_array().unwrap().clone();
    for want in ["docs_example", "world_validate", "world_create", "run_step", "stats_query", "view_snapshot", "world_hotswap", "genome_explain"] {
        assert!(tools.iter().any(|t| t["name"] == want && t["inputSchema"]["type"] == "object"), "no tool {want}");
    }
    // "A small dish under strong light, with builders only": the AI's part, as fixed edits of the example.
    let example = m.tool("docs_example", json!({}))["source"].as_str().unwrap().to_string();
    let source = example
        .replace("Grid(w: 256, h: 256)", "Grid(w: 64, h: 64)")
        .replace("amount: 40mu, overflow: void) @transfer_light", "amount: 80mu, overflow: void) @transfer_light")
        .replace("count: 2_000;", "count: 200;");
    let bad = m.request("tools/call", json!({"name": "world_validate", "arguments": {"source": source.replace("potential 9", "potential 30")}}));
    assert_eq!(bad["result"]["structuredContent"]["valid"], false, "an uphill reaction is caught before running");
    assert_eq!(m.tool("world_validate", json!({"source": source, "lineages": ["Builder"]}))["valid"], true);
    let w = m.tool("world_create", json!({"source": source, "lineages": ["Builder"], "seed": 3}))["world"].clone();
    assert_eq!(m.tool("run_step", json!({"world": w, "ticks": 80}))["tick"], 80);
    let stats = m.tool("stats_query", json!({"world": w}));
    assert!(stats["population"].as_u64().unwrap() > 0 && stats["births"].as_u64().unwrap() > 0, "{stats}");
    let snap = m.tool("view_snapshot", json!({"world": w, "substance": "body", "agents": 5}));
    assert_eq!(snap["field"]["values"].as_array().unwrap().len(), 64 * 64);
    // A rule hot-swapped; a stale retry of it is a tool error the model can read.
    let swap = m.tool("world_hotswap", json!({"world": w, "base_revision": 0, "patch": "patch Dish {\n    set rules.rotting@react_rot.rate = 200bp;\n}\n", "reason": "faster recycling"}));
    assert_eq!(swap["revision"], 1);
    let stale = m.request("tools/call", json!({"name": "world_hotswap", "arguments": {"world": w, "base_revision": 0, "patch": "patch Dish {\n    set upkeep.base = 25mu;\n}\n"}}));
    assert_eq!(stale["result"]["isError"], true);
    assert!(stale["result"]["content"][0]["text"].as_str().unwrap().contains("stale"));
    m.tool("run_step", json!({"world": w, "ticks": 10}));
    let text = m.tool("genome_explain", json!({"world": w}))["explanation"].as_str().unwrap().to_string();
    assert!(text.contains("runs energy + carbon + nitrate -> heat + body") && text.contains("divides once it holds"), "{text}");
    assert_eq!(m.request("tools/call", json!({"name": "no.such", "arguments": {}}))["error"]["code"], -32602);
    // Names a strict client accepts; a dotted name from before 2026-10-21 is told the new one.
    for t in &tools {
        let name = t["name"].as_str().unwrap();
        assert!(!name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'), "tool name {name}");
    }
    let old = m.request("tools/call", json!({"name": "stats.query", "arguments": {"world": w}}));
    assert!(old["error"]["message"].as_str().unwrap().contains("`stats_query`"), "{old}");
    m.finish();
}

#[test]
fn a_modern_client_discovers_and_calls() {
    let mut m = Mcp::start(true);
    let d = m.request("server/discover", json!({}));
    assert!(d["result"]["supportedVersions"].as_array().unwrap().contains(&json!("2026-07-28")));
    assert_eq!(d["result"]["resultType"], "complete");
    let list = m.request("tools/list", json!({}));
    assert_eq!(list["result"]["resultType"], "complete");
    assert!(m.tool("docs_query", json!({"query": "patch"}))["sections"].as_array().is_some());
    m.meta.as_mut().unwrap()["io.modelcontextprotocol/protocolVersion"] = json!("1900-01-01");
    let old = m.request("tools/list", json!({}));
    assert_eq!(old["error"]["code"], -32022);
    assert!(old["error"]["data"]["supported"].as_array().unwrap().contains(&json!("2025-11-25")));
    m.finish();
}
