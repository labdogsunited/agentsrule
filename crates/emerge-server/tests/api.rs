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
