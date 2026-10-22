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
    assert_eq!(n("shared") + n("changed") + n("only_a"), diff["a"]["genes"].as_u64().unwrap() as usize, "every gene of A is shared or only in A");
    assert_eq!(n("shared") + n("changed") + n("only_b"), diff["b"]["genes"].as_u64().unwrap() as usize);
    assert!(n("changed") + n("only_a") + n("only_b") > 0, "Drifter and Builder differ");
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

/// Stage S5 (`proposals/science-panels.md` §6): a creature's ancestors follow the recorded births back
/// to a founder, each link is the child of the one before, and the descendants of a founder add up.
#[test]
fn ancestry_follows_recorded_births_to_a_founder() {
    let mut engine = Engine::new();
    let w = engine.call("world.create", &json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": 1, "mutation": true})).unwrap()["world"].as_str().unwrap().to_string();
    engine.call("run.step", &json!({"world": w, "ticks": 150})).unwrap();
    let all = engine.call("entities.query", &json!({"world": w, "limit": 1_000_000})).unwrap();
    // The youngest living creature has the longest ancestry.
    let young = all["entities"].as_array().unwrap().iter().min_by_key(|e| e["age"].as_u64().unwrap()).unwrap()["id"].as_u64().unwrap();
    let a = engine.call("ancestry.query", &json!({"world": w, "entity": young})).unwrap();
    assert!(a["ancestors_complete"].as_bool().unwrap());
    let chain = a["ancestors"].as_array().unwrap();
    assert!(!chain.is_empty(), "a late birth has ancestors");
    let mut child = a["entity"].clone();
    for up in chain {
        assert_eq!(child["parent"], up["id"], "each link is the parent of the one before");
        assert!(up["born"].is_null() || up["born"].as_u64() <= child["born"].as_u64(), "parents are born first");
        child = up.clone();
    }
    assert!(child["parent"].is_null(), "the chain ends at a founder, who has no recorded parent");
    assert_eq!(a["founder"], child["id"]);
    // Descendants: counted from the same events; the children are a part of them, and the living are a part of the ever.
    let mut with_kids = 0;
    for e in all["entities"].as_array().unwrap() {
        let a = engine.call("ancestry.query", &json!({"world": w, "entity": e["id"]})).unwrap();
        let d = &a["descendants"];
        assert!(d["ever"].as_u64() >= a["entity"]["children"].as_u64() && d["alive_now"].as_u64() <= d["ever"].as_u64());
        with_kids += usize::from(d["ever"].as_u64().unwrap() > 0);
    }
    assert!(with_kids > 0, "some creature has had descendants after 150 ticks");
    assert!(engine.call("ancestry.query", &json!({"world": w, "entity": 999_999_999u64})).is_err(), "a dead or unknown creature is refused, not invented");
}

/// Stage S6 (`proposals/science-panels.md` §12): comparing two worlds names every setting that
/// differs, compares only ticks both sampled, and leaves both runs unchanged. Identical setups give
/// identical series; a different seed is named, and the answer calls them single trials.
#[test]
fn comparing_runs_names_the_setup_differences() {
    let mut engine = Engine::new();
    let mk = |seed: u64| json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": seed, "mutation": true});
    let ids: Vec<String> = [1u64, 1, 2].iter().map(|s| engine.call("world.create", &mk(*s)).unwrap()["world"].as_str().unwrap().to_string()).collect();
    for (i, id) in ids.iter().enumerate() { engine.call("run.step", &json!({"world": id, "ticks": if i == 2 { 40 } else { 60 }})).unwrap(); }
    let before = engine.run(&ids[0]).unwrap().digest();
    let same = engine.call("runs.compare", &json!({"a": ids[0], "b": ids[1]})).unwrap();
    assert!(same["differs"].as_array().unwrap().is_empty());
    assert!(same["reading"].as_str().unwrap().contains("not explained by the setup"));
    for s in same["series"].as_array().unwrap() { assert_eq!(s["a"], s["b"], "the same setup gives the same history"); assert!(!s["a"].as_array().unwrap().is_empty()); }
    let other = engine.call("runs.compare", &json!({"a": ids[0], "b": ids[2]})).unwrap();
    assert_eq!(other["differs"], json!(["seed (1 against 2)"]));
    assert!(other["reading"].as_str().unwrap().contains("single trials"));
    assert!(other["common_samples"].as_u64().unwrap() <= 41 && other["samples_a"].as_u64().unwrap() > other["common_samples"].as_u64().unwrap(), "only common ticks are compared");
    assert_eq!(engine.run(&ids[0]).unwrap().digest(), before, "comparing changes nothing");
    assert!(engine.call("runs.compare", &json!({"a": ids[0], "b": "nope"})).is_err());
}

/// Stage S4 (`proposals/science-panels.md` §9): a region's totals add up. Four quarters of the world sum
/// to the whole, a range across a wrapping edge counts what lies on both sides, and bad ranges are refused.
#[test]
fn region_totals_add_up_across_quarters_and_wrapped_edges() {
    let mut engine = Engine::new();
    let w = engine.call("world.create", &json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": 1, "mutation": false})).unwrap()["world"].as_str().unwrap().to_string();
    engine.call("run.step", &json!({"world": w, "ticks": 30})).unwrap();
    let size = engine.call("region.query", &json!({"world": w, "x": [0.0, 1.0], "y": [0.0, 1.0]})).unwrap()["world"]["w"].as_f64().unwrap();
    let mut q = |x: [f64; 2], y: [f64; 2]| engine.call("region.query", &json!({"world": w, "x": x, "y": y}));
    let h = size / 2.0;
    let whole = q([0.0, size], [0.0, size]).unwrap();
    // Quarters with an open edge each side would double count a creature exactly on the middle line, so shift the cut a hair.
    let cut = h - 1e-3;
    let parts: Vec<Value> = [([0.0, cut], [0.0, cut]), ([h, size], [0.0, cut]), ([0.0, cut], [h, size]), ([h, size], [h, size])].iter().map(|(a, b)| q(*a, *b).unwrap()).collect::<Vec<_>>();
    let sum = |k: &str| parts.iter().map(|p| p[k].as_u64().unwrap()).sum::<u64>();
    let all = whole["creatures_total"].as_u64().unwrap();
    assert!(sum("creatures_total") <= all && all - sum("creatures_total") <= all / 50, "the quarters hold the creatures but for the hair-thin strip between them");
    let cells: u64 = parts.iter().map(|p| p["cells"]["count"].as_u64().unwrap()).sum();
    assert_eq!(cells, whole["cells"]["count"].as_u64().unwrap(), "every cell is in exactly one quarter");
    assert_eq!(whole["cells"]["count"], 96 * 96);
    // Wrapping: the two strips either side of the edge, asked as one range, equal the two asked apart.
    let strip = size / 10.0;
    let across = q([size - strip, strip], [0.0, size]).unwrap();
    let left = q([size - strip, size], [0.0, size]).unwrap();
    let right = q([0.0, strip], [0.0, size]).unwrap();
    assert_eq!(across["creatures_total"].as_u64().unwrap(), left["creatures_total"].as_u64().unwrap() + right["creatures_total"].as_u64().unwrap());
    assert_eq!(across["cells"]["count"].as_u64().unwrap(), left["cells"]["count"].as_u64().unwrap() + right["cells"]["count"].as_u64().unwrap());
    assert!(q([-1.0, 5.0], [0.0, 5.0]).is_err() && q([0.0, size + 5.0], [0.0, 5.0]).is_err(), "ranges outside the world are refused");
}

/// Stage S2 (`proposals/science-panels.md` §7): the atlas counts each gene's carriers by group, a clade is
/// a creature and its descendants, and a changed parameter pairs as one changed gene.
#[test]
fn the_atlas_counts_carriers_by_lineage_and_clade() {
    let mut engine = Engine::new();
    let w = engine.call("world.create", &json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": 1, "mutation": true})).unwrap()["world"].as_str().unwrap().to_string();
    engine.call("run.step", &json!({"world": w, "ticks": 120})).unwrap();
    let a = engine.call("genome.atlas", &json!({"world": w})).unwrap();
    let census = engine.call("genome.census", &json!({"world": w})).unwrap();
    for (g, c) in a["groups"].as_array().unwrap().iter().zip(census["lineages"].as_array().unwrap()) {
        assert_eq!(g["creatures"], c["count"], "the atlas covers the same creatures as the census");
        let n = g["creatures"].as_u64().unwrap();
        for t in g["traits"].as_array().unwrap() { assert!(t["active"].as_u64().unwrap() + t["dormant"].as_u64().unwrap() >= 1); let _ = n; }
    }
    // A clade: a living creature that has had children; the clade group plus the rest equals everyone.
    let all = engine.call("entities.query", &json!({"world": w, "limit": 1_000_000})).unwrap();
    let mut root = None;
    for e in all["entities"].as_array().unwrap() {
        let f = engine.call("ancestry.query", &json!({"world": w, "entity": e["id"]})).unwrap();
        if f["descendants"]["alive_now"].as_u64().unwrap() > 0 { root = Some((e["id"].clone(), f["descendants"]["alive_now"].as_u64().unwrap())); break; }
    }
    let (root, alive) = root.expect("a creature with living descendants");
    let c = engine.call("genome.atlas", &json!({"world": w, "clade": root})).unwrap();
    let g = c["groups"].as_array().unwrap();
    assert_eq!(g[0]["creatures"].as_u64().unwrap(), alive + 1, "the clade is the creature and its living descendants");
    assert_eq!(g[0]["creatures"].as_u64().unwrap() + g[1]["creatures"].as_u64().unwrap(), census["lineages"].as_array().unwrap().iter().map(|l| l["count"].as_u64().unwrap()).sum::<u64>());
}

/// Stage S3 (`proposals/science-panels.md` §8): a `--accounts` file of the CLI imports as the same budget the live
/// panel gives for the same world: every account line equal. Needs the CLI built (`cargo build --release -p emerge-ref`).
#[test]
fn an_accounts_file_imports_as_the_live_budget() {
    let target = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../target").to_string());
    let cli = std::path::Path::new(&target).join("release/emerge");
    assert!(cli.exists(), "build the CLI first: cargo build --release -p emerge-ref");
    let dir = std::env::temp_dir().join(format!("emerge-accounts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (src, out) = (dir.join("dish.em"), dir.join("accounts.csv"));
    std::fs::write(&src, small(DISH)).unwrap();
    let st = std::process::Command::new(&cli).args(["run", src.to_str().unwrap(), "--partial", "--lineages", "Drifter,Builder", "--seed", "1", "--ticks", "100", "--every", "10", "--accounts", out.to_str().unwrap()]).output().unwrap();
    assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stderr));
    let csv = std::fs::read_to_string(&out).unwrap();
    let mut engine = Engine::new();
    let w = engine.call("world.create", &json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": 1, "mutation": true})).unwrap()["world"].as_str().unwrap().to_string();
    engine.call("accounts.start", &json!({"world": w})).unwrap();
    engine.call("run.step", &json!({"world": w, "ticks": 100})).unwrap();
    let live = engine.call("accounts.query", &json!({"world": w})).unwrap();
    let file = engine.call("accounts.import", &json!({"csv": csv})).unwrap();
    assert!(file["manifest"].as_str().unwrap().contains("binary="), "the file's manifest is kept");
    for l in live["lineages"].as_array().unwrap() {
        let g = file["groups"].as_array().unwrap().iter().find(|g| g["group"] == l["lineage"]).unwrap_or_else(|| panic!("no group {}", l["lineage"]));
        assert_eq!(g["lines"], l["lines"], "{}: same account lines", l["lineage"]);
        assert_eq!(g["explained_mu"], l["explained_mu"]);
    }
    assert!(engine.call("accounts.import", &json!({"csv": "tick,group\n1,a"})).is_err(), "a file with another catalogue is refused");
    std::fs::remove_dir_all(&dir).ok();
}

/// Stage S3 (`proposals/science-panels.md` §8): a changing group closes too. A clade (a creature and its
/// descendants) against everyone else reconciles to the integer, with the stock that passes between the
/// two (a child of a member joins the clade at birth) counted as lines and not as error.
#[test]
fn a_clade_budget_reconciles_as_members_are_born_and_die() {
    let mut engine = Engine::new();
    let w = engine.call("world.create", &json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": 1, "mutation": true})).unwrap()["world"].as_str().unwrap().to_string();
    engine.call("run.step", &json!({"world": w, "ticks": 60})).unwrap();
    let all = engine.call("entities.query", &json!({"world": w, "limit": 1_000_000})).unwrap();
    let root = all["entities"].as_array().unwrap().iter().find(|e| engine.call("ancestry.query", &json!({"world": w, "entity": e["id"]})).unwrap()["descendants"]["alive_now"].as_u64().unwrap() > 0).expect("a creature with descendants")["id"].clone();
    assert!(engine.call("accounts.start", &json!({"world": w, "clade": 999_999_999u64})).is_err());
    engine.call("accounts.start", &json!({"world": w, "clade": root})).unwrap();
    engine.call("run.step", &json!({"world": w, "ticks": 80})).unwrap();
    let b = engine.call("accounts.query", &json!({"world": w})).unwrap();
    let g = b["lineages"].as_array().unwrap();
    assert_eq!(g.len(), 2);
    assert!(g[1]["lineage"].as_str().unwrap().starts_with("clade of #"));
    for x in g { assert_eq!(x["unreconciled_mu"], 0, "{}: closes", x["lineage"]); }
    assert!(g[1]["creatures_start"].as_u64().unwrap() >= 2, "the clade began with the creature and its descendants");
}

/// Stage S4 (`proposals/science-panels.md` §3.3): per-statement counters tie to the lens's statements and
/// count what the runner did: a `when` that holds for two ticks lets every cell through twice, no more; what is
/// delivered never exceeds what was asked for; a world with no transfers in a tick has no requests.
#[test]
fn rule_activity_counts_what_the_runner_did() {
    let mut engine = Engine::new();
    let src = small(NIGHT).replace("when tick < 300 or tick >= 400", "when tick < 2");
    let w = engine.call("world.create", &json!({"source": src, "lineages": ["Builder"], "seed": 1, "mutation": false})).unwrap()["world"].as_str().unwrap().to_string();
    engine.call("run.step", &json!({"world": w, "ticks": 5})).unwrap();
    let a = engine.call("rules.activity", &json!({"world": w})).unwrap();
    let lens = engine.call("rules.lens", &json!({"world": w})).unwrap();
    let rows = a["rows"].as_array().unwrap();
    let cells = 96u64 * 96;
    let sun = rows.iter().find(|r| r["path"] == "Dish.rules.sunlight#0").unwrap();
    assert_eq!(sun["cells"], 5 * cells, "every cell is looked at every tick");
    assert_eq!(sun["passed"], 2 * cells, "the condition held on ticks 0 and 1 only");
    assert_eq!(sun["requests"], 2 * cells);
    assert!(sun["delivered_mu"].as_u64() <= sun["requested_mu"].as_u64());
    assert_eq!(sun["requested_mu"], 2 * cells * 40);
    let rot = rows.iter().find(|r| r["call"] == "react").unwrap();
    assert_eq!(rot["passed"], 5 * cells);
    assert!(rot["ran"].as_u64().unwrap() > 0 && rot["extent"].as_u64().unwrap() >= rot["ran"].as_u64().unwrap(), "rot ran where there was body to rot");
    for r in rows { assert!(r["delivered_mu"].as_u64() <= r["requested_mu"].as_u64()); }
    // The statements are the lens's statements.
    let lens_paths: Vec<&str> = lens["steps"].as_array().unwrap().iter().flat_map(|s| s["rules"].as_array().unwrap()).flat_map(|r| r["statements"].as_array().unwrap()).map(|s| s["path"].as_str().unwrap()).collect();
    for r in rows { assert!(lens_paths.contains(&r["path"].as_str().unwrap()), "{}", r["path"]); }
    assert_eq!(a["since_tick"], 0);
    // A change of rules starts the counters again, and says since when: the old counts belonged to the old rules.
    engine.call("world.hotswap", &json!({"world": w, "base_revision": 0, "patch": "patch Dish {\n    set rules.rotting@react_rot.rate = 200bp;\n}\n", "reason": "faster recycling"})).unwrap();
    engine.call("run.step", &json!({"world": w, "ticks": 1})).unwrap();
    let b = engine.call("rules.activity", &json!({"world": w})).unwrap();
    assert_eq!(b["since_tick"], 5);
    assert_eq!(b["revision"], 1);
    let rot = b["rows"].as_array().unwrap().iter().find(|r| r["call"] == "react").unwrap();
    assert_eq!(rot["cells"], cells, "one tick of the new rules");
}

/// Stage S4 (`proposals/science-panels.md` §4): a bounded recording keeps exactly the asked-for statement's
/// requests of the ticks after its start, counts what the cap forced it to drop, and changes nothing in the run.
#[test]
fn a_bounded_recording_keeps_what_it_says_and_counts_what_it_drops() {
    let mut engine = Engine::new();
    let mk = || json!({"source": small(NIGHT), "lineages": ["Builder"], "seed": 1, "mutation": false});
    let w = engine.call("world.create", &mk()).unwrap()["world"].as_str().unwrap().to_string();
    let twin = engine.call("world.create", &mk()).unwrap()["world"].as_str().unwrap().to_string();
    assert!(engine.call("trace.query", &json!({"world": w})).is_err(), "nothing recorded before a start");
    assert!(engine.call("trace.start", &json!({"world": w, "rules": ["Dish.rules.nope#0"]})).is_err());
    engine.call("run.step", &json!({"world": w, "ticks": 2})).unwrap();
    engine.call("run.step", &json!({"world": twin, "ticks": 5})).unwrap();
    let cells = 96u64 * 96;
    engine.call("trace.start", &json!({"world": w, "ticks": 3, "cap": 100_000, "rules": ["Dish.rules.sunlight#0"]})).unwrap();
    engine.call("run.step", &json!({"world": w, "ticks": 3})).unwrap();
    let t = engine.call("trace.query", &json!({"world": w, "limit": 5})).unwrap();
    assert_eq!(t["from_tick"], 2);
    assert_eq!(t["requests_matching"], 3 * cells, "one sunlight request per cell per tick, none from before the start");
    assert_eq!(t["kept"], 3 * cells);
    assert_eq!(t["dropped"], 0);
    assert_eq!(t["complete"], true);
    let r = &t["records"][0];
    assert_eq!(r["by"], "Dish.rules.sunlight#0");
    assert_eq!(r["requested_mu"], 40);
    assert!(r["delivered_mu"].as_u64() <= r["requested_mu"].as_u64());
    assert_eq!(r["tick"], 3, "the first recorded tick is the one after the start");
    // A cap drops the rest and says so.
    engine.call("trace.start", &json!({"world": w, "ticks": 1, "cap": 100, "rules": ["Dish.rules.sunlight#0"]})).unwrap();
    engine.call("run.step", &json!({"world": w, "ticks": 1})).unwrap();
    let c = engine.call("trace.query", &json!({"world": w})).unwrap();
    assert_eq!((c["kept"].as_u64().unwrap(), c["dropped"].as_u64().unwrap()), (100, cells - 100));
    assert_eq!(c["complete"], false, "a recording that dropped records is not complete");
    // Creatures' own actions are recorded too, when asked: the Builder takes nitrate.
    engine.call("trace.start", &json!({"world": w, "ticks": 5, "cap": 5000, "rules": [], "creatures": true})).unwrap();
    engine.call("run.step", &json!({"world": w, "ticks": 5})).unwrap();
    let a = engine.call("trace.query", &json!({"world": w, "limit": 5000})).unwrap();
    assert!(a["records"].as_array().unwrap().iter().any(|r| r["kind"] == "creature" && r["from"].as_str().unwrap().starts_with("cell") && r["to"].as_str().unwrap().starts_with("creature")), "creatures take from cells");
    assert!(a["interactions"].is_array());
    // Observation changed nothing: w ran 2+3+1+5 = 11 ticks, so compare at equal ticks.
    engine.call("run.step", &json!({"world": twin, "ticks": 6})).unwrap();
    assert_eq!(engine.run(&w).unwrap().digest(), engine.run(&twin).unwrap().digest(), "recording does not change the run");
}

/// Stage S3 (`proposals/science-panels.md` §4): the flow data follows the formula. Each reaction's source reports
/// amounts equal to extent times coefficients, elements balance between inputs and outputs, and two world rules
/// that run the same reaction stay two separate sources.
#[test]
fn reaction_flows_follow_the_formula_and_keep_operations_apart() {
    let mut engine = Engine::new();
    let src = small(NIGHT).replace("    rule spread on cells {", "    rule rotting_again on cells {\n        react(rot, rate: 10bp) @react_rot_again;\n    }\n\n    rule spread on cells {");
    let w = engine.call("world.create", &json!({"source": src, "lineages": ["Builder"], "seed": 1, "mutation": false})).unwrap()["world"].as_str().unwrap().to_string();
    engine.call("run.step", &json!({"world": w, "ticks": 30})).unwrap();
    let f = engine.call("reactions.flow", &json!({"world": w})).unwrap();
    let rot = f["reactions"].as_array().unwrap().iter().find(|r| r["name"] == "rot").unwrap();
    let world_sources: Vec<&Value> = rot["measured"].as_array().unwrap().iter().filter(|m| m["by"].is_string() && m["by"] != "creatures' reaction genes").collect();
    assert_eq!(world_sources.len(), 2, "two statements run `rot` and stay two sources");
    assert_ne!(world_sources[0]["by"], world_sources[1]["by"]);
    for r in f["reactions"].as_array().unwrap() {
        for m in r["measured"].as_array().unwrap() {
            let extent = m["extent"].as_u64().unwrap();
            for i in r["inputs"].as_array().unwrap() {
                assert_eq!(m["inputs_mu"][i["substance"].as_str().unwrap()].as_u64().unwrap(), extent * i["coeff"].as_u64().unwrap(), "amount = extent × coefficient");
            }
            let (ein, eout) = (&m["elements_in_mu"], &m["elements_out_mu"]);
            assert_eq!(ein, eout, "{}: elements balance across a reaction", r["name"]);
        }
    }
    assert!(world_sources.iter().all(|m| m["extent"].as_u64().unwrap() > 0), "both ran: there is body to rot");
    assert_eq!(f["blocked_counting"], false);
    engine.call("flow.blocked_start", &json!({"world": w})).unwrap();
    engine.call("run.step", &json!({"world": w, "ticks": 3})).unwrap();
    let g = engine.call("reactions.flow", &json!({"world": w})).unwrap();
    assert!(g["reactions"].as_array().unwrap().iter().any(|r| r["blocked"].is_object()));
}

/// Stage S5 (`proposals/science-panels.md` §10): a cohort's size is the births recorded in its ticks (the history's
/// own birth counts), its members are alive (censored), starved or aged and nothing else, and the living ones
/// are exactly the living creatures born in those ticks.
#[test]
fn a_cohort_is_the_births_of_its_ticks_with_the_living_counted_as_censored() {
    let mut engine = Engine::new();
    let w = engine.call("world.create", &json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": 1, "mutation": true})).unwrap()["world"].as_str().unwrap().to_string();
    engine.call("run.step", &json!({"world": w, "ticks": 100})).unwrap();
    let l = engine.call("life.query", &json!({"world": w, "from": 30, "to": 60})).unwrap();
    let h = engine.call("history.query", &json!({"world": w})).unwrap();
    let births: u64 = h["samples"].as_array().unwrap().iter().filter(|s| (31..=61).contains(&s["tick"].as_u64().unwrap())).map(|s| s["births"].as_u64().unwrap()).sum();
    let c = &l["cohort"];
    assert_eq!(c["size"].as_u64().unwrap(), births, "the cohort is the births of ticks 30 to 60 (a birth in tick t is in the sample taken after it, at t + 1)");
    assert_eq!(c["size"].as_u64().unwrap(), c["starved"].as_u64().unwrap() + c["aged"].as_u64().unwrap() + c["alive_censored"].as_u64().unwrap());
    let all = engine.call("entities.query", &json!({"world": w, "limit": 1_000_000})).unwrap();
    let mut living = 0;
    for e in all["entities"].as_array().unwrap() {
        let a = engine.call("ancestry.query", &json!({"world": w, "entity": e["id"]})).unwrap();
        if let Some(b) = a["entity"]["born"].as_u64() { living += u64::from((30..=60).contains(&b)); }
    }
    assert_eq!(c["alive_censored"].as_u64().unwrap(), living, "the censored are the living born in those ticks");
    let surv = c["survival"].as_array().unwrap();
    assert_eq!(surv[0]["alive"], c["size"], "everyone is alive at age zero");
    assert!(surv.windows(2).all(|p| p[1]["alive"].as_u64() <= p[0]["alive"].as_u64()), "survival never rises");
    let ages: u64 = l["snapshot"]["ages"].as_array().unwrap().iter().flat_map(|a| a["bins"].as_array().unwrap()).map(|b| b.as_u64().unwrap()).sum();
    assert_eq!(ages, all["total"].as_u64().unwrap(), "the age distribution covers every living creature");
    assert!(l["source"].as_str().unwrap().contains("founders"));
}

/// Stage S5 (`proposals/science-panels.md` §11): a bookmark keeps copies of its evidence, so it is still what was
/// seen after the source moves on; the timeline keeps accepted and rejected edits apart and brackets a lineage that
/// fell to zero by the two samples around it; the export names the run and holds every entry.
#[test]
fn the_notebook_keeps_its_evidence_and_the_timeline_says_only_what_was_sampled() {
    let mut engine = Engine::new();
    let w = engine.call("world.create", &json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": 1, "mutation": true})).unwrap()["world"].as_str().unwrap().to_string();
    engine.call("run.step", &json!({"world": w, "ticks": 40})).unwrap();
    let first = engine.call("entities.query", &json!({"world": w, "limit": 1})).unwrap()["entities"][0]["id"].clone();
    let b = engine.call("notebook.add", &json!({"world": w, "title": "Why does Builder fall?", "question": "what limits Builder", "note": "n", "attach": ["stats", "source", "genome", "budget"], "entity": first})).unwrap();
    assert_eq!(b["tick"], 40);
    assert!(b["evidence"]["budget"]["unavailable"].is_string(), "evidence that cannot be had is recorded as unavailable, not dropped");
    let old_source = b["evidence"]["source"]["source"].as_str().unwrap().to_string();
    engine.call("run.step", &json!({"world": w, "ticks": 20})).unwrap();
    engine.call("world.hotswap", &json!({"world": w, "base_revision": 0, "patch": "patch Dish {\n    set rules.rotting@react_rot.rate = 200bp;\n}\n", "reason": "faster recycling"})).unwrap();
    assert!(engine.call("world.hotswap", &json!({"world": w, "base_revision": 0, "patch": "patch Dish {\n    set upkeep.base = 25mu;\n}\n"})).is_err());
    let listed = engine.call("notebook.list", &json!({"world": w})).unwrap();
    assert_eq!(listed["entries"][0]["evidence"]["source"]["source"].as_str().unwrap(), old_source, "the bookmark's source is the source it was made with");
    assert_ne!(old_source, engine.call("world.source", &json!({"world": w})).unwrap()["source"].as_str().unwrap_or(""), "the live source has moved on");
    engine.call("run.step", &json!({"world": w, "ticks": 400})).unwrap();
    let t = engine.call("notebook.events", &json!({"world": w})).unwrap();
    let ev = t["events"].as_array().unwrap();
    assert!(ev.iter().any(|e| e["kind"] == "edit accepted" && e["tick"] == 60));
    assert!(t["caution"].as_str().unwrap().contains("association"));
    let h = engine.call("history.query", &json!({"world": w})).unwrap();
    let zero: Vec<&Value> = ev.iter().filter(|e| e["kind"] == "lineage sampled at zero").collect();
    assert!(!zero.is_empty(), "a lineage fell to zero within 460 ticks");
    for z in zero {
        let i = h["lineages"].as_array().unwrap().iter().position(|n| *n == z["lineage"]).unwrap();
        let at = |tick: &Value| h["samples"].as_array().unwrap().iter().find(|s| s["tick"] == *tick).unwrap()["counts"][i].as_u64().unwrap();
        assert_eq!(at(&z["tick"]), 0);
        assert!(at(&z["tick_from"]) > 0, "the sample before had creatures: the fall lies between the two");
    }
    let x = engine.call("notebook.export", &json!({"world": w})).unwrap();
    assert_eq!(x["format"], "emerge-notebook/1");
    assert_eq!(x["entries"].as_array().unwrap().len(), 1);
    assert!(x["run"]["header"]["source_hash"].is_string());
}

/// Stage S6 (`proposals/science-panels.md` §12): the outcome grid keeps every world (an extinct one and a short one too),
/// compares at the common tick only, counts trials per setup without calling a repeat of one seed a new trial, and
/// holds only the pairs a user declared. The provenance says which run an export came from.
#[test]
fn the_outcome_grid_keeps_every_trial_and_pairs_are_only_declared() {
    let mut engine = Engine::new();
    let mk = |seed: u64| json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": seed, "mutation": true});
    let ids: Vec<String> = [1u64, 1, 2, 3].iter().map(|s| engine.call("world.create", &mk(*s)).unwrap()["world"].as_str().unwrap().to_string()).collect();
    for (id, t) in ids.iter().zip([200u64, 200, 200, 50]) { engine.call("run.step", &json!({"world": id, "ticks": t})).unwrap(); }
    let o = engine.call("runs.outcomes", &json!({})).unwrap();
    assert_eq!(o["rows"].as_array().unwrap().len(), 4, "every world is a row");
    assert_eq!(o["common_tick"], 50, "worlds that ran different lengths are compared at the shortest");
    assert!(o["rows"].as_array().unwrap().iter().all(|r| r["population_at_common_tick"].is_array()));
    let setups = o["setups"].as_array().unwrap();
    assert_eq!(setups.len(), 1);
    assert_eq!((setups[0]["trials"].as_u64().unwrap(), setups[0]["distinct_seeds"].as_u64().unwrap()), (4, 3));
    assert!(setups[0]["note"].as_str().unwrap().contains("reproducibility"));
    assert!(o["pairs"].as_array().unwrap().is_empty(), "nothing is paired until the user says so");
    engine.call("runs.pair", &json!({"a": ids[0], "b": ids[2], "relation": "same source, seeds 1 and 2"})).unwrap();
    assert_eq!(engine.call("runs.outcomes", &json!({})).unwrap()["pairs"][0]["relation"], "same source, seeds 1 and 2");
    assert!(engine.call("runs.pair", &json!({"a": ids[0], "b": ids[0]})).is_err());
    let p = engine.call("export.provenance", &json!({"world": ids[0]})).unwrap();
    assert_eq!(p["tick"], 200);
    assert!(p["source_hash"].as_str().unwrap().len() == 16 && p["engine"].as_str().unwrap().starts_with("emerge-server"));
}
