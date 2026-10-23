//! The rest of the node system (`proposals/houdini-node-system.md`, H1 to H6 "not done" lists): pasting into another world,
//! inserting a node on a wire, an organisation that outlives the server, and (further down) nested authoring, the Look
//! pipeline and reuse. Each semantic edit is one revision and one undo; presentation is never a revision.

use emerge_server::api::Engine;
use serde_json::{json, Value};

const DISH: &str = include_str!("../../../examples/dish.em");
const LIFE: &str = include_str!("../../../examples/life.em");

fn small(s: &str) -> String {
    s.replace("Grid(w: 256, h: 256)", "Grid(w: 64, h: 64)").replace("count: 2_000;", "count: 200;")
}

fn dish(engine: &mut Engine) -> String {
    engine.call("world.create", &json!({"source": small(DISH), "lineages": ["Builder"], "seed": 1, "mutation": false})).unwrap()["world"].as_str().unwrap().to_string()
}

fn graph(engine: &mut Engine, w: &str) -> Value {
    engine.call("world.graph", &json!({"world": w})).unwrap()
}

fn node<'a>(g: &'a Value, id: &str) -> Option<&'a Value> {
    g["nodes"].as_array().unwrap().iter().find(|n| n["id"] == id)
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("emerge-nodes-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// Statements copied in one world are pasted into another as new rules there, checked by that world: one revision, one
/// undo. A world without the field they worked on refuses them with the reason and changes nothing.
#[test]
fn pasting_into_another_world_is_checked_there() {
    let mut engine = Engine::new();
    let (a, b) = (dish(&mut engine), dish(&mut engine));
    let ga = graph(&mut engine, &a);
    let text = node(&ga, "Dish.rules.weather@transfer_nitrate").unwrap()["text"].as_str().unwrap().to_string();
    let r = engine.call("world.graph_op", &json!({"world": b, "base_revision": 0, "op": {"op": "paste", "items": [{"on": "cells", "text": text}]}})).unwrap();
    assert_eq!(r["selected"], "Dish.rules.pasted_1@transfer_nitrate");
    let gb = graph(&mut engine, &b);
    assert_eq!(gb["revision"], 1);
    assert_eq!(node(&gb, "Dish.rules.pasted_1@transfer_nitrate").unwrap()["text"], json!(text));
    engine.call("world.undo", &json!({"world": b})).unwrap();
    assert!(node(&graph(&mut engine, &b), "Dish.rules.pasted_1").is_none(), "one undo takes the paste back");
    // Into a world without `cells`: refused, nothing changes.
    let life = engine.call("world.create", &json!({"source": LIFE, "lineages": [], "seed": 1})).unwrap()["world"].as_str().unwrap().to_string();
    let e = engine.call("world.graph_op", &json!({"world": life, "base_revision": 0, "op": {"op": "paste", "items": [{"on": "cells", "text": text}]}})).unwrap_err();
    assert!(e.message.contains("no field `cells`"), "{}", e.message);
    assert_eq!(engine.run(&life).unwrap().revision(), 0);
    // A statement naming what the world lacks is refused by its checker.
    let e = engine.call("world.graph_op", &json!({"world": b, "base_revision": 2, "op": {"op": "paste", "items": [{"on": "cells", "text": "transfer(unobtainium, from: rock, to: self, amount: 1mu) @x;"}]}})).unwrap_err();
    assert!(e.code == -32001, "{e:?}");
    assert_eq!(engine.run(&b).unwrap().revision(), 2);
}

/// A sink dropped on the wire from a transfer to its destination becomes that transfer's overflow; any other drop on a
/// wire has no insertion and says so.
#[test]
fn a_sink_dropped_on_a_destination_wire_becomes_the_overflow() {
    let mut engine = Engine::new();
    let w = dish(&mut engine);
    let t = "Dish.rules.weather@transfer_nitrate".to_string();
    // The optional end is cut first (the dish routes every inflow's excess to `void`).
    engine.call("world.graph_op", &json!({"world": w, "base_revision": 0, "op": {"op": "unwire", "id": t, "port": "overflow"}})).unwrap();
    let g = graph(&mut engine, &w);
    assert!(!node(&g, &t).unwrap()["text"].as_str().unwrap().contains("overflow"));
    let to = g["wires"].as_array().unwrap().iter().find(|x| x["from"] == t.as_str()).unwrap()["to"].as_str().unwrap().to_string();
    let dry = engine.call("world.graph_op", &json!({"world": w, "base_revision": 1, "dry_run": true, "op": {"op": "insert", "id": "Dish.sinks.void", "from": t, "to": to}})).unwrap();
    assert!(dry["patch"].as_str().unwrap().contains("overflow: void"), "{dry}");
    engine.call("world.graph_op", &json!({"world": w, "base_revision": 1, "op": {"op": "insert", "id": "Dish.sinks.void", "from": t, "to": to}})).unwrap();
    let g1 = graph(&mut engine, &w);
    assert!(node(&g1, &t).unwrap()["text"].as_str().unwrap().contains("overflow: void"));
    assert_eq!(g1["revision"], 2);
    // A store is not a sink; a pair of boxes with no wire between them is no wire.
    let e = engine.call("world.graph_op", &json!({"world": w, "base_revision": 2, "op": {"op": "insert", "id": "Dish.fields.cells", "from": t, "to": to}})).unwrap_err();
    assert!(e.message.contains("no insertion is defined"), "{}", e.message);
    assert!(engine.call("world.graph_op", &json!({"world": w, "base_revision": 2, "op": {"op": "insert", "id": "Dish.sinks.void", "from": to, "to": t}})).is_err());
    engine.call("world.undo", &json!({"world": w})).unwrap();
    assert!(!node(&graph(&mut engine, &w), &t).unwrap()["text"].as_str().unwrap().contains("overflow"), "one undo takes the insertion back");
}

/// The network's organisation (positions, notes, boxes, routing dots) is kept by the world's name in the state directory,
/// so the same world created by a new server gets it back. None of it is a revision.
#[test]
fn the_organisation_outlives_the_server() {
    let dir = scratch("layout");
    let mut engine = Engine::new().with_state_dir(dir.clone());
    let w = dish(&mut engine);
    engine.call("world.layout_set", &json!({"world": w, "positions": {"Dish.fields.cells": [700.0, 40.0]}, "notes": {"n1": {"x": 1, "y": 2, "text": "kept"}}, "frames": {"b1": {"x": 0, "y": 0, "w": 300, "h": 200, "label": "supply"}}, "routes": {"world|Dish.sources.rock>Dish.rules.weather@transfer_carbon>carbon": [[300, 10], [320, 80]]}})).unwrap();
    assert!(engine.call("world.layout_set", &json!({"world": w, "routes": {"x": [[1, 2, 3]]}})).is_err(), "a route is a list of points");
    assert_eq!(engine.run(&w).unwrap().revision(), 0);
    drop(engine);
    let mut again = Engine::new().with_state_dir(dir.clone());
    let made = again.call("world.create", &json!({"source": small(DISH), "lineages": ["Builder"], "seed": 2, "mutation": false})).unwrap();
    assert_eq!(made["layout_restored"], "Dish");
    let g = graph(&mut again, made["world"].as_str().unwrap());
    assert_eq!(g["positions"]["Dish.fields.cells"], json!([700.0, 40.0]));
    assert_eq!(g["notes"]["n1"]["text"], "kept");
    assert_eq!(g["frames"]["b1"]["label"], "supply");
    assert_eq!(g["routes"]["world|Dish.sources.rock>Dish.rules.weather@transfer_carbon>carbon"], json!([[300, 10], [320, 80]]));
    // Without a state directory nothing is kept or read.
    let mut plain = Engine::new();
    assert!(plain.call("world.create", &json!({"source": small(DISH), "lineages": ["Builder"], "seed": 2, "mutation": false})).unwrap()["layout_restored"].is_null());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The dish with two rules that hold `when`s: one around transfers (and an `else`), one around a reaction.
fn dish_with_whens(engine: &mut Engine) -> String {
    let rules = "    rule wind on cells {\n        when self.carbon >= 5u and below.nitrate < 3u {\n            transfer(carbon, from: self, to: right, amount: 3mu) @wind_carbon;\n        } else {\n            transfer(nitrate, from: self, to: left, amount: 2mu) @wind_else;\n        }\n    }\n\n    rule decay on cells {\n        when self.body > 1u {\n            react(rot, rate: 50bp) @decay_rot;\n        }\n    }\n\n";
    let s = small(DISH);
    let at = s.find("    rule spread on cells").unwrap();
    let src = format!("{}{rules}{}", &s[..at], &s[at..]);
    engine.call("world.create", &json!({"source": src, "lineages": ["Builder"], "seed": 1, "mutation": false})).unwrap()["world"].as_str().unwrap().to_string()
}

fn wires_of<'a>(g: &'a Value, id: &str) -> Vec<&'a Value> {
    g["wires"].as_array().unwrap().iter().filter(|x| x["from"] == id || x["to"] == id).collect()
}

/// H4: statements inside a `when` have their wires and ports, a condition's reads and a signal statement's reads and writes
/// are value-dependency wires, and the runner's counts reach a nested statement by its id.
#[test]
fn nested_statements_and_value_dependencies_are_wired() {
    let mut engine = Engine::new();
    let w = dish_with_whens(&mut engine);
    let g = graph(&mut engine, &w);
    // The nested transfer: from the cells (`self`) to the cells (a neighbour), each a resource wire with its edit.
    let t = wires_of(&g, "Dish.rules.wind#0/t0");
    assert!(t.iter().any(|x| x["from"] == "Dish.fields.cells" && x["to_port"] == "from" && x["edit"].is_string()), "{t:?}");
    assert!(t.iter().any(|x| x["to"] == "Dish.fields.cells" && x["from_port"] == "to"));
    // The nested react: its reaction (a reference) and its formula's inputs and outputs.
    let r = wires_of(&g, "Dish.rules.decay#0/t0");
    assert!(r.iter().any(|x| x["from"] == "Dish.reactions.rot" && x["kind"] == "declaration reference"));
    assert!(r.iter().any(|x| x["substance"] == "body" && x["derived"] == true));
    // The `when` reads carbon and nitrate (a neighbour's) from the cells: value dependencies, never edited by wiring.
    let c = wires_of(&g, "Dish.rules.wind#0");
    let subs: Vec<&str> = c.iter().filter(|x| x["kind"] == "value dependency").filter_map(|x| x["substance"].as_str()).collect();
    assert!(subs.contains(&"carbon") && subs.contains(&"nitrate"), "{subs:?}");
    let wind = node(&g, "Dish.rules.wind#0").unwrap();
    assert_eq!(wind["contract"]["condition"], "self.carbon >= 5u and below.nitrate < 3u");
    assert!(wind["contract"]["ports"].as_array().unwrap().iter().any(|p| p["key"] == "reads"));
    // A signal statement reads and writes its field's value.
    let smell = g["nodes"].as_array().unwrap().iter().find(|n| n["kind"] == "signal").unwrap();
    let sw = wires_of(&g, smell["id"].as_str().unwrap());
    assert!(sw.iter().any(|x| x["from_port"] == "writes" && x["to"] == "Dish.fields.scent"), "{sw:?}");
    assert!(sw.iter().any(|x| x["to_port"] == "reads" && x["from"] == "Dish.fields.scent"));
    assert_eq!(smell["contract"]["deps"]["writes"], json!(["scent"]));
    // Every wire's ends are boxes the graph has (top-level or nested).
    let ids: Vec<&str> = g["nodes"].as_array().unwrap().iter().chain(g["nested"].as_array().unwrap()).map(|n| n["id"].as_str().unwrap()).collect();
    for x in g["wires"].as_array().unwrap() {
        assert!(ids.contains(&x["from"].as_str().unwrap()) && ids.contains(&x["to"].as_str().unwrap()), "{x}");
    }
    // Measured: the runner's counts name the nested statement the way the graph does.
    engine.call("run.step", &json!({"world": w, "ticks": 3})).unwrap();
    let a = engine.call("rules.activity", &json!({"world": w})).unwrap();
    let rows: Vec<&str> = a["rows"].as_array().unwrap().iter().filter_map(|r| r["nested_id"].as_str()).collect();
    assert_eq!(rows, ["Dish.rules.wind#0/t0", "Dish.rules.wind#0/e0", "Dish.rules.decay#0/t0"]);
}

/// H4: a statement added inside an existing `when` (or its `else`, made when it has none), a condition set from the graph,
/// and a nested `react` rewired to another reaction: each one revision and one undo; a refused one changes nothing.
#[test]
fn a_when_grows_and_its_condition_is_edited_from_the_graph() {
    let mut engine = Engine::new();
    let w = dish_with_whens(&mut engine);
    let op = |engine: &mut Engine, base: u64, op: Value| engine.call("world.graph_op", &json!({"world": w, "base_revision": base, "op": op}));
    op(&mut engine, 0, json!({"op": "add_inside", "id": "Dish.rules.wind#0", "branch": "then", "text": "transfer(body, from: self, to: right, amount: 1mu) @wind_body;"})).unwrap();
    let g = graph(&mut engine, &w);
    assert_eq!(g["revision"], 1);
    assert!(g["nested"].as_array().unwrap().iter().any(|n| n["id"] == "Dish.rules.wind#0/t1" && n["text"].as_str().unwrap().contains("@wind_body")));
    // An `else` is made for a `when` that has none.
    op(&mut engine, 1, json!({"op": "add_inside", "id": "Dish.rules.decay#0", "branch": "else", "text": "react(photosynthesis, rate: 10bp) @decay_else;"})).unwrap();
    let g = graph(&mut engine, &w);
    assert!(g["nested"].as_array().unwrap().iter().any(|n| n["id"] == "Dish.rules.decay#0/e0"));
    // The condition, set from the graph.
    op(&mut engine, 2, json!({"op": "condition", "id": "Dish.rules.decay#0", "cond": "self.body > 2u"})).unwrap();
    assert_eq!(node(&graph(&mut engine, &w), "Dish.rules.decay#0").unwrap()["contract"]["condition"], "self.body > 2u");
    // A condition the checker refuses: nothing changes.
    assert!(op(&mut engine, 3, json!({"op": "condition", "id": "Dish.rules.decay#0", "cond": "self.unobtainium > 2u"})).is_err());
    assert_eq!(engine.run(&w).unwrap().revision(), 3);
    // A nested `react` wired to another reaction.
    op(&mut engine, 3, json!({"op": "wire", "from": "Dish.reactions.photosynthesis", "to": "Dish.rules.decay#0/t0"})).unwrap();
    let g = graph(&mut engine, &w);
    assert!(g["nested"].as_array().unwrap().iter().find(|n| n["id"] == "Dish.rules.decay#0/t0").unwrap()["text"].as_str().unwrap().starts_with("react(photosynthesis"));
    // Four edits, four undos: the rules are as they were.
    for _ in 0..4 {
        engine.call("world.undo", &json!({"world": w})).unwrap();
    }
    let g = graph(&mut engine, &w);
    let texts: Vec<&str> = g["nested"].as_array().unwrap().iter().map(|n| n["text"].as_str().unwrap()).collect();
    assert_eq!(texts.len(), 3, "{texts:?}");
    assert_eq!(node(&g, "Dish.rules.decay#0").unwrap()["contract"]["condition"], "self.body > 1u");
}

/// H4, code: the inline check is the swap's own check without the commit; the names offered for completion carry the box
/// that declares them (jump-to-definition) and the built-ins.
#[test]
fn the_code_box_checks_while_typing_and_knows_the_names() {
    let mut engine = Engine::new();
    let w = dish(&mut engine);
    let ok = engine.call("world.graph_edit", &json!({"world": w, "base_revision": 0, "dry_run": true, "id": "Dish.rules.weather@transfer_carbon", "text": "transfer(carbon, from: rock, to: self, amount: 30mu, overflow: void) @transfer_carbon;"})).unwrap();
    assert_eq!(ok["checked"], true);
    assert_eq!(engine.run(&w).unwrap().revision(), 0, "a check changes nothing");
    let bad = engine.call("world.graph_edit", &json!({"world": w, "base_revision": 0, "dry_run": true, "id": "Dish.rules.weather@transfer_carbon", "text": "transfer(carbn, from: rock, to: self, amount: 30mu) @transfer_carbon;"})).unwrap_err();
    assert!(bad.code == -32001 && bad.message.contains("carbn"), "{bad:?}");
    let syntax = engine.call("world.graph_edit", &json!({"world": w, "base_revision": 0, "dry_run": true, "id": "Dish.rules.weather@transfer_carbon", "text": "transfer(carbon, from: rock to: self) @t;"})).unwrap_err();
    assert!(syntax.data.as_ref().is_some_and(|d| d["column"].as_u64().is_some()), "a syntax error says where in the text: {syntax:?}");
    // A wire's dry run is checked too: a connection the checker refuses is refused before it is made.
    let refused = engine.call("world.graph_op", &json!({"world": w, "base_revision": 0, "dry_run": true, "op": {"op": "wire", "from": "Dish.sources.spark", "to": "Dish.rules.weather@transfer_carbon"}}));
    assert!(refused.is_err(), "spark does not emit carbon");
    let names = engine.call("world.names", &json!({"world": w})).unwrap();
    let find = |n: &str| names["names"].as_array().unwrap().iter().find(|x| x["name"] == n).cloned().unwrap();
    assert_eq!(find("carbon")["id"], "Dish.substances.carbon");
    assert_eq!(find("rock")["kind"], "source");
    assert_eq!(find("transfer")["kind"], "function");
    assert_eq!(find("below")["kind"], "direction");
}
