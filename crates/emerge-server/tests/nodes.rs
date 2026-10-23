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
