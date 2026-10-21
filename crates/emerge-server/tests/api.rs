//! The engine API over a real WebSocket (`proposals/engine-api.md` H5), and the view clause of exit
//! test 10: while a world runs, a committed swap shows in `view.snapshot` within a second. CPU runner.

use serde_json::{json, Value};
use std::net::TcpListener;
use tungstenite::{connect, Message};

const DISH: &str = include_str!("../../../examples/dish.em");

struct Client {
    ws: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    id: u64,
}

impl Client {
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        self.ws.send(Message::Text(json!({"jsonrpc": "2.0", "id": self.id, "method": method, "params": params}).to_string().into())).unwrap();
        loop {
            if let Message::Text(t) = self.ws.read().unwrap() {
                let v: Value = serde_json::from_str(&t).unwrap();
                assert_eq!(v["id"], self.id);
                return v;
            }
        }
    }
    fn ok(&mut self, method: &str, params: Value) -> Value {
        let v = self.call(method, params);
        assert!(v.get("error").is_none(), "{method}: {v}");
        v["result"].clone()
    }
}

fn start(journals: Option<std::path::PathBuf>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || emerge_server::serve::serve(listener, journals));
    port
}

fn client(port: u16) -> Client {
    let (ws, _) = connect(format!("ws://127.0.0.1:{port}")).unwrap();
    Client { ws, id: 0 }
}

fn server() -> Client {
    client(start(None))
}

#[test]
fn the_api_over_websocket() {
    let mut c = server();
    let source = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;");
    assert_eq!(c.ok("world.validate", json!({"source": source}))["valid"], true);
    assert_eq!(c.ok("world.validate", json!({"source": source.replace("potential 9", "potential 30")}))["valid"], false);
    let w = c.ok("world.create", json!({"source": source, "lineages": ["Drifter", "Builder"], "seed": 4}))["world"].clone();
    assert_eq!(c.ok("run.step", json!({"world": w, "ticks": 30}))["tick"], 30);
    let stats = c.ok("stats.query", json!({"world": w}));
    assert!(stats["population"].as_u64().unwrap() > 0 && stats["population_by_lineage"]["Builder"].as_u64().unwrap() > 0, "{stats}");
    let snap = c.ok("view.snapshot", json!({"world": w, "substance": "carbon", "agents": 10}));
    assert_eq!((snap["field"]["w"].clone(), snap["field"]["values"].as_array().unwrap().len()), (json!(96), 96 * 96));
    assert_eq!(snap["agents"]["shown"].as_array().unwrap().len(), 10);
    // A committed swap, then a stale one with its code and the current revision.
    let swap = c.ok("world.hotswap", json!({"world": w, "base_revision": 0, "patch": "patch Dish {\n    set rules.rotting@react_rot.rate = 80bp;\n}\n", "reason": "test"}));
    assert_eq!(swap["revision"], 1);
    let stale = c.call("world.hotswap", json!({"world": w, "base_revision": 0, "patch": "patch Dish {\n    set upkeep.base = 25mu;\n}\n"}));
    assert_eq!((stale["error"]["code"].clone(), stale["error"]["data"]["kind"].clone(), stale["error"]["data"]["current_revision"].clone()), (json!(-32001), json!("stale"), json!(1)));
    let id = c.ok("entities.query", json!({"world": w, "limit": 3}))["entities"][0]["id"].clone();
    let text = c.ok("genome.explain", json!({"world": w, "entity": id}))["explanation"].as_str().unwrap().to_string();
    assert!(text.contains("module m0:") && text.contains("Signal network:"), "{text}");
    assert!(c.ok("docs.query", json!({"query": "hot-swap"}))["sections"].as_array().is_some());
    assert_eq!(c.call("no.such", json!({}))["error"]["code"], -32601);
    // Exit test 10, view clause: the world runs; a swap commits; the next snapshots show it.
    assert_eq!(c.ok("run.resume", json!({"world": w}))["running"], true);
    std::thread::sleep(std::time::Duration::from_millis(300));
    let t0 = c.ok("stats.query", json!({"world": w}))["tick"].as_u64().unwrap();
    let swapped = std::time::Instant::now();
    let r = c.ok("world.hotswap", json!({"world": w, "base_revision": 1, "patch": "patch Dish {\n    set rules.sunlight@transfer_light.amount = 50mu;\n}\n", "reason": "live"}));
    let seen = loop {
        let s = c.ok("view.snapshot", json!({"world": w, "agents": 1}));
        if s["revision"] == 2 {
            break swapped.elapsed().as_secs_f64();
        }
        assert!(swapped.elapsed().as_secs_f64() < 5.0, "the snapshot never showed revision 2");
    };
    let t1 = c.ok("stats.query", json!({"world": w}))["tick"].as_u64().unwrap();
    println!("live swap: pause {:.2} ms, compile {:.0} ms, seen in a snapshot after {:.0} ms; ticks {t0} -> {t1} while running", r["pause_ms"].as_f64().unwrap(), r["compile_ms"].as_f64().unwrap(), seen * 1e3);
    assert!(seen < 1.0 && r["pause_ms"].as_f64().unwrap() < 200.0 && t1 > t0, "no restart: the world kept running");
    c.ok("run.pause", json!({"world": w}));
    let lines = c.ok("journal.read", json!({"world": w}))["lines"].as_array().unwrap().len();
    assert_eq!(lines, 4, "header, two committed swaps and the stale one");
}

#[test]
fn web_pages_cannot_connect() {
    // Browsers let any site open a socket to localhost; a handshake with an Origin is refused.
    use tungstenite::client::IntoClientRequest;
    let port = start(None);
    let mut req = format!("ws://127.0.0.1:{port}").into_client_request().unwrap();
    req.headers_mut().insert("Origin", "https://evil.example".parse().unwrap());
    match connect(req) {
        Err(tungstenite::Error::Http(resp)) => assert_eq!(resp.status(), 403),
        other => panic!("a web page got in: {:?}", other.map(|(_, r)| r.status())),
    }
}

#[test]
fn callers_cannot_choose_where_files_are_written() {
    let source = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 32, h: 32)").replace("count: 2_000;", "count: 50;");
    let dir = std::env::temp_dir().join(format!("emerge-journals-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut c = client(start(Some(dir.clone())));
    let target = dir.join("victim.txt");
    std::fs::write(&target, "keep me").unwrap();
    let r = c.call("world.create", json!({"source": source, "lineages": ["Drifter"], "journal": target.to_str().unwrap()}));
    assert_eq!(r["error"]["code"], -32602, "{r}");
    // The server names journals itself, and never overwrites one: a second server's `w1` fails.
    c.ok("world.create", json!({"source": source, "lineages": ["Drifter"]}));
    let first = std::fs::read_to_string(dir.join("w1.jsonl")).unwrap();
    assert!(first.contains("\"kind\":\"header\""));
    let mut again = client(start(Some(dir.clone())));
    let r = again.call("world.create", json!({"source": source, "lineages": ["Drifter"]}));
    assert!(r["error"]["message"].as_str().unwrap().contains("exists"), "{r}");
    assert_eq!(std::fs::read_to_string(dir.join("w1.jsonl")).unwrap(), first);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep me");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn world_graph_draws_the_running_source() {
    let mut c = server();
    let w = c.ok("world.create", json!({"source": DISH, "lineages": ["Drifter"], "seed": 1}))["world"].as_str().unwrap().to_string();
    let g = c.ok("world.graph", json!({"world": w}));
    assert_eq!(g["world"], "Dish");
    let nodes = g["nodes"].as_array().unwrap();
    assert!(nodes.iter().any(|n| n["id"] == "Dish.rules.sunlight@transfer_light" && n["kind"] == "transfer" && n["step"] == "Transfers"));
    assert!(g["wires"].as_array().unwrap().iter().any(|x| x["from"] == "Dish.sources.sun" && x["to"] == "Dish.rules.sunlight@transfer_light" && x["substance"] == "light"));
    // An unknown world is an error, not a panic.
    assert!(c.call("world.graph", json!({"world": "nope"})).get("error").is_some());
}

fn small_dish() -> String {
    DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 32, h: 32)").replace("count: 2_000;", "count: 50;")
}

fn text_of(g: &Value, id: &str) -> String {
    g["nodes"].as_array().unwrap().iter().find(|n| n["id"] == id).unwrap_or_else(|| panic!("no box {id}"))["text"].as_str().unwrap().to_string()
}

#[test]
fn editing_a_box_swaps_the_running_world_and_the_journal_replays() {
    let dir = std::env::temp_dir().join(format!("emerge-graph-edit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut c = client(start(Some(dir.clone())));
    let w = c.ok("world.create", json!({"source": small_dish(), "lineages": ["Drifter"], "seed": 2}))["world"].as_str().unwrap().to_string();
    c.ok("run.step", json!({"world": w, "ticks": 20}));
    let g = c.ok("world.graph", json!({"world": w}));
    let id = "Dish.rules.sunlight@transfer_light";
    assert!(text_of(&g, id).contains("amount: 40mu"));
    let base = g["revision"].as_u64().unwrap();

    // A labelled statement: the amount of sunlight changes, the revision moves, the world runs on.
    let edited = text_of(&g, id).replace("40mu", "20mu");
    c.ok("world.graph_edit", json!({"world": w, "id": id, "text": edited, "base_revision": base}));
    let g2 = c.ok("world.graph", json!({"world": w}));
    assert_eq!(g2["revision"].as_u64().unwrap(), base + 1);
    assert!(text_of(&g2, id).contains("amount: 20mu"), "{}", text_of(&g2, id));
    assert_eq!(c.ok("run.step", json!({"world": w, "ticks": 10}))["tick"], 30);

    // A statement without a label: the rule's body is replaced with that statement swapped.
    let smell = g2["nodes"].as_array().unwrap().iter().find(|n| n["group"] == "Dish.rules.smell").unwrap().clone();
    let id2 = smell["id"].as_str().unwrap();
    assert!(id2.contains('#'), "{id2}");
    c.ok("world.graph_edit", json!({"world": w, "id": id2, "text": smell["text"].as_str().unwrap().replace("keep: 0.9", "keep: 0.8"), "base_revision": base + 1}));
    let g3 = c.ok("world.graph", json!({"world": w}));
    assert!(text_of(&g3, id2).contains("keep: 0.8"), "{}", text_of(&g3, id2));

    // A bad edit, and a stale one, change nothing.
    let rev = g3["revision"].as_u64().unwrap();
    let bad = c.call("world.graph_edit", json!({"world": w, "id": id, "text": "transfer(light, from: sun", "base_revision": rev}));
    assert!(bad["error"]["message"].as_str().unwrap().contains("at the end of the text"), "{bad}");
    let unknown = c.call("world.graph_edit", json!({"world": w, "id": id, "text": text_of(&g3, id).replace("transfer(light,", "transfer(unobtainium,"), "base_revision": rev}));
    assert_eq!(unknown["error"]["data"]["kind"], "invalid", "{unknown}");
    let stale = c.call("world.graph_edit", json!({"world": w, "id": id, "text": text_of(&g3, id), "base_revision": base}));
    assert_eq!(stale["error"]["data"]["kind"], "stale", "{stale}");
    let not_a_statement = c.call("world.graph_edit", json!({"world": w, "id": "Dish.fields.cells", "text": "x", "base_revision": rev}));
    assert!(not_a_statement.get("error").is_some());
    let g4 = c.ok("world.graph", json!({"world": w}));
    assert_eq!((g4["revision"].as_u64().unwrap(), text_of(&g4, id)), (rev, text_of(&g3, id)));

    // The journal holds the edits, and replays to the same state.
    let journal = std::fs::read_to_string(dir.join(format!("{w}.jsonl"))).unwrap();
    assert!(journal.contains("graph edit of Dish.rules.sunlight@transfer_light"), "the swap is journalled with its reason");
    let replayed = emerge_server::journal::replay(&journal).expect("the journal replays");
    // The journal is replayed to its last entry, the second swap at tick 30; swaps that were refused are not in it.
    assert_eq!((replayed.tick(), replayed.revision()), (30, rev));
    std::fs::remove_dir_all(&dir).unwrap();
}

fn ids(g: &Value) -> Vec<String> {
    g["nodes"].as_array().unwrap().iter().map(|n| n["id"].as_str().unwrap().to_string()).collect()
}

#[test]
fn graph_operations_are_patches_that_swap_the_running_world() {
    let dir = std::env::temp_dir().join(format!("emerge-graph-ops-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut c = client(start(Some(dir.clone())));
    let w = c.ok("world.create", json!({"source": small_dish(), "lineages": ["Drifter"], "seed": 3}))["world"].as_str().unwrap().to_string();
    c.ok("run.step", json!({"world": w, "ticks": 10}));
    let rev = |c: &mut Client| c.ok("world.graph", json!({"world": w}))["revision"].as_u64().unwrap();
    let op = |c: &mut Client, op: Value| {
        let base = rev(c);
        c.call("world.graph_op", json!({"world": w, "base_revision": base, "op": op}))
    };
    let source = |c: &mut Client| c.ok("world.source", json!({"world": w}))["source"].as_str().unwrap().to_string();
    let original = source(&mut c);

    // A parameter: changed in place, and changed back, the source is the original byte for byte.
    let id = "Dish.rules.sunlight@transfer_light";
    let r = op(&mut c, json!({"op": "set_param", "id": id, "param": "amount", "value": "3u"}));
    assert!(r.get("error").is_none(), "{r}");
    let g = c.ok("world.graph", json!({"world": w}));
    let node = g["nodes"].as_array().unwrap().iter().find(|n| n["id"] == id).unwrap().clone();
    assert!(node["params"].as_array().unwrap().iter().any(|p| p == &json!(["amount", "3u"])), "{node}");
    assert_ne!(source(&mut c), original);
    op(&mut c, json!({"op": "set_param", "id": id, "param": "amount", "value": "40mu"}));
    assert_eq!(source(&mut c), original, "an edit and its undo give the source back");

    // A new box is a new rule with one statement; it runs.
    let r = op(&mut c, json!({"op": "add_rule", "kind": "react"}));
    let new_id = r["result"]["selected"].as_str().unwrap_or_else(|| panic!("{r}")).to_string();
    assert!(new_id.starts_with("Dish.rules.new_react_1@react_"), "{new_id}");
    assert_eq!(c.ok("run.step", json!({"world": w, "ticks": 5}))["tick"], 15);
    // Wire a reaction to it: the react box now runs that reaction.
    let r = op(&mut c, json!({"op": "wire", "from": "Dish.reactions.rot", "to": new_id}));
    assert!(r.get("error").is_none(), "{r}");
    let g = c.ok("world.graph", json!({"world": w}));
    assert!(g["wires"].as_array().unwrap().iter().any(|x| x["from"] == "Dish.reactions.rot" && x["to"] == new_id.as_str()), "{}", g["wires"]);
    // A transfer box: add, and wire a source that cannot supply its substance: refused, nothing changes.
    let r = op(&mut c, json!({"op": "add_rule", "kind": "transfer"}));
    let tid = r["result"]["selected"].as_str().unwrap().to_string();
    let before = rev(&mut c);
    let bad = op(&mut c, json!({"op": "wire", "from": "Dish.sources.spark", "to": tid}));
    assert!(bad.get("error").is_some(), "{bad}");
    assert_eq!(rev(&mut c), before);
    // Two stores cannot be wired to each other.
    let nonsense = op(&mut c, json!({"op": "wire", "from": "Dish.fields.cells", "to": "Dish.sources.rock"}));
    assert!(nonsense.get("error").is_some());

    // Delete: a statement of a rule with others, and a rule's only statement (the rule goes too).
    op(&mut c, json!({"op": "delete", "id": "Dish.rules.weather@transfer_body"}));
    op(&mut c, json!({"op": "delete", "id": "Dish.rules.rotting@react_rot"}));
    let g = c.ok("world.graph", json!({"world": w}));
    let have = ids(&g);
    assert!(!have.iter().any(|i| i == "Dish.rules.weather@transfer_body" || i.starts_with("Dish.rules.rotting")), "{have:?}");
    assert!(have.iter().any(|i| i == "Dish.rules.weather@transfer_carbon"));
    assert_eq!(c.ok("run.step", json!({"world": w, "ticks": 5}))["tick"], 20);

    // Positions are kept beside the world, in a file the server names.
    c.ok("world.layout_set", json!({"world": w, "positions": {id: [120.5, 40.0]}}));
    let g = c.ok("world.graph", json!({"world": w}));
    assert_eq!(g["positions"][id], json!([120.5, 40.0]));
    assert!(c.call("world.layout_set", json!({"world": w, "positions": {id: "left"}})).get("error").is_some());
    let meta = std::fs::read_to_string(dir.join(format!("{w}.meta.json"))).unwrap();
    assert!(meta.contains("120.5"), "{meta}");

    // Every edit is in the journal with its reason, and the journal replays.
    let journal = std::fs::read_to_string(dir.join(format!("{w}.jsonl"))).unwrap();
    for reason in ["graph set amount of", "graph add a react rule", "graph wire Dish.reactions.rot", "graph delete Dish.rules.rotting@react_rot"] {
        assert!(journal.contains(reason), "journal lacks `{reason}`");
    }
    emerge_server::journal::replay(&journal).expect("the journal replays");
    std::fs::remove_dir_all(&dir).unwrap();
}
