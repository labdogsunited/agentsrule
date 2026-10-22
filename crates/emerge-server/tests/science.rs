//! Science panels, stage S1 (`proposals/science-panels.md` §15): the Rule Lens schedule is the
//! checked schedule `emerge order` prints, the lineage history says how it was sampled, and asking
//! for either changes nothing in the run.

use emerge_server::api::Engine;
use serde_json::{json, Value};

const DISH: &str = include_str!("../../../examples/dish.em");
const TEST9: &str = include_str!("../../../archive/test9-dish.em");
const LIFE: &str = include_str!("../../../examples/life.em");
const NIGHT: &str = include_str!("../../../examples/presets/long_night.em");

fn small(s: &str) -> String {
    s.replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;")
}

/// The text of `emerge order` cut into one block per numbered step, keyed by the step's name.
fn order_blocks(src: &str) -> Vec<(String, Vec<String>)> {
    let file = emerge_lang::prepare(&emerge_lang::parse(src).unwrap()).unwrap();
    let text = emerge_lang::order::order(&file, "Dish").or_else(|_| emerge_lang::order::order(&file, "Life")).unwrap();
    let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        if let Some((n, rest)) = t.split_once(". ") {
            if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() && line.starts_with("  ") && !line.starts_with("   ") {
                let name = rest.split("  ").next().unwrap().trim().to_string();
                blocks.push((name, vec![]));
                continue;
            }
        }
        if let Some((_, lines)) = blocks.last_mut() {
            if !t.is_empty() && !["Set", "Move", "Transform", "Restructure"].contains(&t) {
                lines.push(t.to_string());
            }
        }
    }
    blocks
}

fn lens_matches_order(src: &str, lineages: &[&str]) {
    let mut engine = Engine::new();
    let w = engine.call("world.create", &json!({"source": src, "lineages": lineages, "seed": 1})).unwrap()["world"].as_str().unwrap().to_string();
    let lens = engine.call("rules.lens", &json!({"world": w})).unwrap();
    let blocks = order_blocks(src);
    let steps = lens["steps"].as_array().unwrap();
    assert_eq!(steps.len(), blocks.len(), "the same steps");
    for (s, (name, lines)) in steps.iter().zip(&blocks) {
        assert_eq!(s["name"].as_str().unwrap(), name);
        let mut want: Vec<String> = Vec::new();
        for r in s["rules"].as_array().unwrap() {
            want.push(format!("rule {} on {}", r["rule"].as_str().unwrap(), r["on"].as_str().unwrap()));
            want.extend(r["statements"].as_array().unwrap().iter().map(|st| st["text"].as_str().unwrap().to_string()));
        }
        let got: Vec<String> = lines.iter().filter(|l| l.starts_with("rule ") || !(l.starts_with("reads ") || l.starts_with("waste ") || l.starts_with("decompose") || l.starts_with("remains"))).cloned().collect();
        assert_eq!(got, want, "step {name}");
    }
    assert!(lens["revision"].is_number() && lens["tick"].is_number(), "an observation says which revision and tick it is of");
}

#[test]
fn the_rule_lens_is_the_checked_schedule() {
    lens_matches_order(&small(DISH), &["Builder"]);
    lens_matches_order(&small(&TEST9.to_string()), &["Builder"]);
    lens_matches_order(LIFE, &[]);
    lens_matches_order(&small(NIGHT), &["Builder"]);
}

#[test]
fn history_says_how_it_was_sampled_and_asking_changes_nothing() {
    let create = |engine: &mut Engine| engine.call("world.create", &json!({"source": small(DISH), "lineages": ["Builder"], "seed": 3})).unwrap()["world"].as_str().unwrap().to_string();
    let mut engine = Engine::new();
    let (a, b) = (create(&mut engine), create(&mut engine));
    for step in 0..6 {
        engine.call("run.step", &json!({"world": a, "ticks": 20})).unwrap();
        engine.call("run.step", &json!({"world": b, "ticks": 20})).unwrap();
        // Only A is observed, over and over.
        engine.call("rules.lens", &json!({"world": a})).unwrap();
        let h: Value = engine.call("history.query", &json!({"world": a})).unwrap();
        assert_eq!(h["lineages"], json!(["Builder"]));
        assert_eq!(h["tick"], 20 * (step + 1));
    }
    let h = engine.call("history.query", &json!({"world": a})).unwrap();
    assert_eq!(h["thinned"], false, "121 samples (tick 0 and every tick after) are not thinned");
    assert_eq!(h["samples"].as_array().unwrap().len(), 121);
    assert_eq!(engine.run(&a).unwrap().digest(), engine.run(&b).unwrap().digest(), "observation does not change the trajectory");
}

const LOOPER: &str = include_str!("fixtures/looper-lineage.em");

/// Genome inspection, stage G1 (`proposals/genome-editor.md` §13): an authored genome with two
/// writers of one signal, a gene that reads its own output, and a signal nothing reads. Every
/// writer and reader must be in the answer, in gene order, with nothing invented.
#[test]
fn the_genome_graph_lists_every_writer_and_reader_including_self_feedback() {
    let mut engine = Engine::new();
    let w = engine.call("world.create", &json!({"source": format!("{DISH}\n{LOOPER}"), "lineages": ["Looper"], "seed": 1, "mutation": false})).unwrap()["world"].as_str().unwrap().to_string();
    engine.call("run.step", &json!({"world": w, "ticks": 2})).unwrap();
    let g = engine.call("genome.graph", &json!({"world": w})).unwrap();
    let genes = g["genes"].as_array().unwrap();
    assert_eq!(genes.iter().map(|x| x["index"].as_u64().unwrap()).collect::<Vec<_>>(), (0..genes.len() as u64).collect::<Vec<_>>(), "genes in execution order, numbered");
    let find = |text: &str| genes.iter().find(|x| x["title"] == text || x["text"].as_str().unwrap().contains(text)).unwrap_or_else(|| panic!("no gene {text}"));
    let signals_of = |x: &Value, key: &str| x[key].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect::<Vec<_>>();
    // A signal's writers and readers, over the whole genome.
    let writers = |k: u64| genes.iter().filter(|x| signals_of(x, "writes").contains(&k)).count();
    let readers = |k: u64| genes.iter().filter(|x| signals_of(x, "reads").contains(&k)).count();
    let weighs: Vec<&Value> = genes.iter().filter(|x| x["title"] == "weigh").collect();
    assert_eq!(weighs.len(), 3);
    // `memory` is the signal the first two weigh genes write; the first also reads it (self-feedback).
    let memory = signals_of(weighs[0], "writes")[0];
    assert_eq!(signals_of(weighs[1], "writes"), vec![memory], "a second writer of the same signal");
    assert!(signals_of(weighs[0], "reads").contains(&memory), "the self-feedback is in the answer");
    assert_eq!((writers(memory), readers(memory)), (2, 2), "two writers; the feedback weigh and the turn action read it");
    let unused = signals_of(weighs[2], "writes")[0];
    assert_eq!((writers(unused), readers(unused)), (1, 0), "a signal nothing reads is still listed");
    assert!(find("turn").is_object());
    for x in genes {
        assert!(x["expressed"].is_boolean() && x["active"].is_boolean() && x["module"].is_number());
    }
    assert!(g["modules"].as_array().unwrap().len() >= 2);
}

/// Node editor H1 (`proposals/houdini-node-system.md` §13): organising the network (positions, notes,
/// network boxes) is presentation. It changes no revision, adds no journal line, and the run after
/// it equals the run without it.
#[test]
fn organising_the_network_changes_nothing_in_the_run() {
    let create = |engine: &mut Engine| engine.call("world.create", &json!({"source": small(DISH), "lineages": ["Builder"], "seed": 2})).unwrap()["world"].as_str().unwrap().to_string();
    let mut engine = Engine::new();
    let (a, b) = (create(&mut engine), create(&mut engine));
    let graph = engine.call("world.graph", &json!({"world": a})).unwrap();
    let id = graph["nodes"][0]["id"].as_str().unwrap().to_string();
    let rev = graph["revision"].clone();
    let journal = engine.call("journal.read", &json!({"world": a})).unwrap()["lines"].as_array().unwrap().len();

    engine.call("run.step", &json!({"world": a, "ticks": 30})).unwrap();
    engine.call("world.layout_set", &json!({"world": a, "positions": {id.clone(): [10.5, 20.0]}, "notes": {"n1": {"x": 1, "y": 2, "text": "supply rules"}}, "frames": {"f1": {"x": 0, "y": 0, "w": 300, "h": 200, "label": "light"}}})).unwrap();
    engine.call("run.step", &json!({"world": a, "ticks": 30})).unwrap();
    engine.call("run.step", &json!({"world": b, "ticks": 60})).unwrap();
    assert_eq!(engine.run(&a).unwrap().digest(), engine.run(&b).unwrap().digest(), "the same state after 60 ticks with and without layout edits");

    let after = engine.call("world.graph", &json!({"world": a})).unwrap();
    assert_eq!(after["revision"], rev, "no revision advanced");
    assert_eq!(engine.call("journal.read", &json!({"world": a})).unwrap()["lines"].as_array().unwrap().len(), journal, "no journal line");
    assert_eq!(after["positions"][&id], json!([10.5, 20.0]));
    assert_eq!(after["notes"]["n1"]["text"], "supply rules");
    assert_eq!(after["frames"]["f1"]["label"], "light");

    // Notes and frames are replaced as a whole; a bad one is refused and changes nothing.
    engine.call("world.layout_set", &json!({"world": a, "notes": {}})).unwrap();
    assert_eq!(engine.call("world.graph", &json!({"world": a})).unwrap()["notes"], json!({}));
    for bad in [json!({"notes": {"n": {"x": 1, "text": "no y"}}}), json!({"frames": {"f": {"x": 0, "y": 0, "w": "wide", "h": 1, "label": ""}}}), json!({"positions": {"x": [1]}}), json!({"notes": {"n": {"x": 1, "y": 1, "text": "t".repeat(2001)}}})] {
        let mut p = bad.clone();
        p["world"] = json!(a);
        assert!(engine.call("world.layout_set", &p).is_err(), "{bad}");
    }
    assert_eq!(engine.call("world.graph", &json!({"world": a})).unwrap()["frames"]["f1"]["label"], "light");
}

/// Stage S2 (`proposals/science-panels.md` §7): the census counts every living entity (so the lineage
/// counts add up to `stats`), comparing a genome with itself finds every gene shared, and a Builder
/// against a Drifter shows differences on both sides. Asking changes nothing.
#[test]
fn the_census_is_exact_and_a_comparison_matches_genes_by_kind_and_text() {
    let mut engine = Engine::new();
    let mk = || json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": 1, "mutation": false});
    let w = engine.call("world.create", &mk()).unwrap()["world"].as_str().unwrap().to_string();
    let twin = engine.call("world.create", &mk()).unwrap()["world"].as_str().unwrap().to_string();
    engine.call("run.step", &json!({"world": w, "ticks": 5})).unwrap();
    engine.call("run.step", &json!({"world": twin, "ticks": 5})).unwrap();
    let c = engine.call("genome.census", &json!({"world": w})).unwrap();
    let rows = c["lineages"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let total: u64 = rows.iter().map(|r| r["count"].as_u64().unwrap()).sum();
    let ents = engine.call("entities.query", &json!({"world": w, "limit": 10})).unwrap();
    assert_eq!(total, ents["total"].as_u64().unwrap(), "the census covers the whole population");
    for r in rows {
        let hist: u64 = r["histogram"].as_array().unwrap().iter().map(|h| h["count"].as_u64().unwrap()).sum();
        assert_eq!(hist, r["count"].as_u64().unwrap(), "the histogram holds every entity once");
    }
    assert!(c["coverage"].as_str().unwrap().contains("exact"));
    let mut pick = |lineage: &str| {
        let all = engine.call("entities.query", &json!({"world": w, "limit": 100000})).unwrap();
        all["entities"].as_array().unwrap().iter().find(|e| e["lineage"] == lineage).map(|e| e["id"].as_u64().unwrap()).unwrap_or_else(|| panic!("no {lineage}"))
    };
    let (d, b) = (pick("Drifter"), pick("Builder"));
    let same = engine.call("genome.compare", &json!({"world": w, "a": b, "b": b})).unwrap();
    assert!(same["only_a"].as_array().unwrap().is_empty() && same["only_b"].as_array().unwrap().is_empty());
    assert_eq!(same["shared"].as_array().unwrap().len() as u64, same["a"]["genes"].as_u64().unwrap());
    let diff = engine.call("genome.compare", &json!({"world": w, "a": d, "b": b})).unwrap();
    let n = |k: &str| diff[k].as_array().unwrap().len();
    assert_eq!(n("shared") + n("only_a"), diff["a"]["genes"].as_u64().unwrap() as usize, "every gene of A is shared or only in A");
    assert_eq!(n("shared") + n("only_b"), diff["b"]["genes"].as_u64().unwrap() as usize);
    assert!(n("only_a") + n("only_b") > 0, "Drifter and Builder differ");
    assert_eq!(engine.run(&w).unwrap().digest(), engine.run(&twin).unwrap().digest(), "asking leaves the trajectory unchanged");
}

/// Stage S4 (`proposals/science-panels.md` §3.2): the read/write matrix finds, by name, what each
/// statement of the Long Night touches and says how. Asking changes nothing.
#[test]
fn the_rule_matrix_names_what_each_statement_touches() {
    let mut engine = Engine::new();
    let w = engine.call("world.create", &json!({"source": small(NIGHT), "lineages": ["Builder"], "seed": 1, "mutation": false})).unwrap()["world"].as_str().unwrap().to_string();
    let m = engine.call("rules.matrix", &json!({"world": w})).unwrap();
    let cell = |path: &str, col: &str| -> Vec<String> {
        let row = m["rows"].as_array().unwrap().iter().find(|r| r["path"] == path).unwrap_or_else(|| panic!("no row {path}"));
        row["cells"].as_array().unwrap().iter().find(|c| c["col"] == col).map(|c| c["kinds"].as_array().unwrap().iter().map(|k| k.as_str().unwrap().to_string()).collect()).unwrap_or_default()
    };
    assert_eq!(cell("Dish.rules.sunlight#0", "light"), ["transfer"]);
    assert_eq!(cell("Dish.rules.sunlight#0", "sun"), ["from"]);
    assert_eq!(cell("Dish.rules.sunlight#0", "void"), ["overflow"]);
    assert_eq!(cell("Dish.rules.rotting@react_rot", "rot"), ["react"]);
    assert_eq!(cell("Dish.rules.rotting@react_rot", "body"), ["consumes"]);
    assert_eq!(cell("Dish.rules.rotting@react_rot", "carbon"), ["produces"]);
    assert_eq!(cell("Dish.rules.spread@diffuse_conserved", "sugar"), ["diffuse"]);
    assert!(cell("Dish.rules.weather@transfer_carbon", "sun").is_empty(), "nothing is invented");
    assert!(m["completeness"].as_str().unwrap().contains("not none exist"));
    // A condition that reads a substance is marked as a condition read.
    let cond = small(NIGHT).replace("when tick < 300 or tick >= 400", "when self.nitrate < 25u");
    let w2 = engine.call("world.create", &json!({"source": cond, "lineages": ["Builder"], "seed": 1, "mutation": false}));
    {
        let w2 = w2.expect("the condition variant is a valid world");
        let m2 = engine.call("rules.matrix", &json!({"world": w2["world"]})).unwrap();
        let row = m2["rows"].as_array().unwrap().iter().find(|r| r["path"] == "Dish.rules.sunlight#0").unwrap();
        let k: Vec<&Value> = row["cells"].as_array().unwrap().iter().filter(|c| c["col"] == "nitrate").map(|c| &c["kinds"]).collect();
        assert_eq!(k, [&json!(["condition"])], "{row}");
    }
}

/// Stage S3 (`proposals/science-panels.md` §8): the budget of each lineage reconciles to the
/// integer µ: stock change equals the sum of the runtime's account lines. Starting and asking change
/// nothing in the run, and a GPU-less ask says what is missing.
#[test]
fn the_resource_budget_reconciles_to_the_integer() {
    let mut engine = Engine::new();
    let mk = || json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": 1, "mutation": true});
    let w = engine.call("world.create", &mk()).unwrap()["world"].as_str().unwrap().to_string();
    let twin = engine.call("world.create", &mk()).unwrap()["world"].as_str().unwrap().to_string();
    assert!(engine.call("accounts.query", &json!({"world": w})).is_err(), "no window yet");
    engine.call("run.step", &json!({"world": w, "ticks": 5})).unwrap();
    engine.call("run.step", &json!({"world": twin, "ticks": 5})).unwrap();
    engine.call("accounts.start", &json!({"world": w})).unwrap();
    engine.call("run.step", &json!({"world": w, "ticks": 120})).unwrap();
    engine.call("run.step", &json!({"world": twin, "ticks": 120})).unwrap();
    let b = engine.call("accounts.query", &json!({"world": w})).unwrap();
    assert_eq!(b["from_tick"], 5);
    assert_eq!(b["to_tick"], 125);
    for l in b["lineages"].as_array().unwrap() {
        eprintln!("{l}");
        assert_eq!(l["unreconciled_mu"], 0, "{}: the budget closes", l["lineage"]);
    }
    // The digest hashes the whole world, accounts included, so compare what the run did instead.
    let look = |engine: &mut Engine, x: &str| (engine.call("entities.query", &json!({"world": x, "limit": 1_000_000})).unwrap(), engine.call("stats.query", &json!({"world": x})).unwrap());
    assert_eq!(look(&mut engine, &w), look(&mut engine, &twin), "keeping accounts does not change the run");
}
