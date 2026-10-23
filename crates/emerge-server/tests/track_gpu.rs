//! Agent tracking on the GPU (`proposals/agent-tracking.md` §15, §18 scenario 17): in contract mode, exact lookups,
//! query membership, families and recordings equal the CPU's; the GPU's coarser recording cadence is refused, not
//! silently changed. Needs the GPU.

use emerge_server::api::Engine;
use serde_json::{json, Value};

const DISH: &str = include_str!("../../../examples/dish.em");

fn ok(e: &mut Engine, method: &str, p: Value) -> Value {
    e.call(method, &p).unwrap_or_else(|f| panic!("{method}: {} {}", f.code, f.message))
}

#[test]
fn tracking_on_the_gpu_equals_the_cpu_in_contract_mode() {
    let source = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;");
    let mut e = Engine::new();
    let mk = |e: &mut Engine, backend: &str| ok(e, "world.create", json!({"source": source, "lineages": ["Drifter", "Builder"], "seed": 3, "contract": true, "backend": backend}))["world"].as_str().unwrap().to_string();
    let (c, g) = (mk(&mut e, "cpu"), mk(&mut e, "gpu"));
    for w in [&c, &g] { ok(&mut e, "run.step", json!({"world": w, "ticks": 20})); }
    let ids: Vec<String> = ok(&mut e, "entities.find", json!({"world": c, "limit": 6}))["agents"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap().to_string()).collect();
    let strip = |v: &Value| { let mut v = v.clone(); v.as_object_mut().unwrap().remove("observation"); for k in ["backend", "run"] { v.as_object_mut().unwrap().remove(k); } v };
    assert_eq!(strip(&ok(&mut e, "entities.get", json!({"world": c, "ids": ids}))), strip(&ok(&mut e, "entities.get", json!({"world": g, "ids": ids}))), "exact lookups");
    // The same trackers on both.
    for w in [&c, &g] {
        ok(&mut e, "tracker.create", json!({"world": w, "ids": ids}));
        ok(&mut e, "tracker.create", json!({"world": w, "mode": "live", "where": {"attr": "age", "op": "<", "value": 8}, "every": 10}));
        ok(&mut e, "tracker.create", json!({"world": w, "mode": "descendants", "root": ids[0], "every": 10}));
        ok(&mut e, "tracker.record_start", json!({"world": w, "tracker": "t1", "every": 10, "interactions": true}));
    }
    // A recording every tick is refused on the GPU, with the reason.
    let refused = e.call("tracker.record_start", &json!({"world": g, "tracker": "t2", "every": 1})).unwrap_err();
    assert!(refused.message.contains("every 10 ticks"), "{}", refused.message);
    for w in [&c, &g] { ok(&mut e, "run.step", json!({"world": w, "ticks": 80})); }
    let members = |e: &mut Engine, w: &str, t: &str| { let q = ok(e, "tracker.query", json!({"world": w, "tracker": t})); q["members"].as_array().unwrap().iter().map(|m| (m["id"].clone(), m["status"].clone(), m["stays"].clone())).collect::<Vec<_>>() };
    for t in ["t1", "t2", "t3"] { assert_eq!(members(&mut e, &c, t), members(&mut e, &g, t), "tracker {t}"); }
    let rows = |e: &mut Engine, w: &str| ok(e, "tracker.samples", json!({"world": w, "tracker": "t1", "limit": 10000}))["rows"].clone();
    let (rc, rg) = (rows(&mut e, &c), rows(&mut e, &g));
    assert!(!rc.as_array().unwrap().is_empty());
    assert_eq!(rc, rg, "recorded samples");
    // Interactions come from the run's one transfer recording, which keeps every creature's requests and filters afterwards:
    // when the GPU's (smaller) recording is full, which requests got a record is not defined, and it says it is incomplete.
    let inter = |e: &mut Engine, w: &str| { let r = ok(e, "tracker.interactions", json!({"world": w, "tracker": "t1"})); let mut v = r["records"].as_array().unwrap().clone(); v.sort_by_key(|x| x.to_string()); (v, r["complete"].as_bool().unwrap(), r["dropped"].as_u64().unwrap()) };
    let ((ic, cc, _), (ig, cg, dg)) = (inter(&mut e, &c), inter(&mut e, &g));
    assert!(cc, "the CPU's recording kept everything here");
    if cg { assert_eq!(ic, ig, "recorded interactions, as sets"); } else { assert!(dg > 0, "an incomplete recording says what it dropped"); eprintln!("the GPU recording dropped {dg} requests: interactions compared only for completeness"); }
    // And tracking changed nothing in either run.
    let mut twin = Engine::new();
    let tc = ok(&mut twin, "world.create", json!({"source": source, "lineages": ["Drifter", "Builder"], "seed": 3, "contract": true, "backend": "gpu"}))["world"].as_str().unwrap().to_string();
    ok(&mut twin, "run.step", json!({"world": tc, "ticks": 100}));
    assert_eq!(e.run(&g).unwrap().digest(), twin.run(&tc).unwrap().digest());
}

/// What tracking costs a running world (`proposals/agent-tracking.md` §15): ms per tick of the reference dish with no
/// tracking, 32 quick watches, 32 watches recorded, and a live query, on each backend. Run by hand:
/// `cargo test --release -p emerge-server --test track_gpu -- --ignored --nocapture --test-threads=1`.
#[test]
#[ignore]
fn what_tracking_costs() {
    for backend in ["cpu", "gpu"] {
        let every = if backend == "gpu" { 10 } else { 1 };
        for case in ["none", "32 watches", "32 watches recorded", "live query"] {
            let mut e = Engine::new();
            let w = ok(&mut e, "world.create", json!({"source": DISH, "lineages": ["Drifter", "Builder"], "seed": 1, "contract": true, "backend": backend}))["world"].as_str().unwrap().to_string();
            ok(&mut e, "run.step", json!({"world": w, "ticks": 200}));
            let ids: Vec<String> = ok(&mut e, "entities.find", json!({"world": w, "sort": "age", "limit": 32}))["agents"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap().to_string()).collect();
            if case != "none" && case != "live query" { for id in &ids { ok(&mut e, "tracker.create", json!({"world": w, "kind": "watch", "ids": [id]})); } }
            if case == "32 watches recorded" { ok(&mut e, "tracker.create", json!({"world": w, "ids": ids})); let t = format!("t{}", ids.len() + 1); ok(&mut e, "tracker.record_start", json!({"world": w, "tracker": t, "every": every})); }
            if case == "live query" { ok(&mut e, "tracker.create", json!({"world": w, "mode": "live", "where": {"attr": "age", "op": "<", "value": 5}, "every": every})); }
            let pop = e.run(&w).unwrap().population();
            let t = std::time::Instant::now();
            ok(&mut e, "run.step", json!({"world": w, "ticks": 300}));
            println!("{backend} {case:>22}: {:.2} ms/tick ({pop} creatures at the start)", t.elapsed().as_secs_f64() * 1000.0 / 300.0);
        }
    }
}
