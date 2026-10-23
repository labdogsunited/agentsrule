//! Agent tracking, stages T1 to T5 (`proposals/agent-tracking.md` §17, §18): exact lookups by ID, run identity,
//! lossless IDs, trackers and their membership over time, recordings at the run's own cadence, the shared
//! transfer-recording slot, and that none of it changes the run. CPU only (the GPU's parity is in `track_gpu.rs`).

use emerge_server::api::Engine;
use serde_json::{json, Value};

const DISH: &str = include_str!("../../../examples/dish.em");

fn small(s: &str) -> String {
    s.replace("Grid(w: 256, h: 256)", "Grid(w: 64, h: 64)").replace("count: 2_000;", "count: 200;")
}

fn world(e: &mut Engine, seed: u64) -> String {
    let r = e.call("world.create", &json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": seed})).unwrap();
    assert!(r["run"].as_str().is_some_and(|s| s.ends_with(r["world"].as_str().unwrap())), "{r}");
    r["world"].as_str().unwrap().to_string()
}

fn ok(e: &mut Engine, method: &str, p: Value) -> Value {
    e.call(method, &p).unwrap_or_else(|f| panic!("{method}: {} {}", f.code, f.message))
}

fn err(e: &mut Engine, method: &str, p: Value) -> (i64, String) {
    match e.call(method, &p) { Ok(v) => panic!("{method} should be refused: {v}"), Err(f) => (f.code, f.message) }
}

/// Every living ID, from a complete find.
fn living(e: &mut Engine, w: &str) -> Vec<String> {
    let r = ok(e, "entities.find", json!({"world": w, "limit": 1000}));
    assert!(!r["truncated"].as_bool().unwrap());
    r["agents"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap().to_string()).collect()
}

#[test]
fn an_exact_lookup_finds_any_creature_and_says_what_is_known_of_the_rest() {
    let mut e = Engine::new();
    let w = world(&mut e, 3);
    ok(&mut e, "run.step", json!({"world": w, "ticks": 80}));
    // A creature outside the 5-row sample is still found by its ID.
    let sample: Vec<String> = ok(&mut e, "entities.query", json!({"world": w, "limit": 5}))["entities"].as_array().unwrap().iter().map(|a| a["id"].to_string()).collect();
    let all = living(&mut e, &w);
    let outside = all.iter().find(|id| !sample.contains(id)).unwrap().clone();
    let r = ok(&mut e, "entities.get", json!({"world": w, "ids": [outside]}));
    let a = &r["agents"][0];
    assert_eq!((a["id"].as_str(), a["status"].as_str()), (Some(outside.as_str()), Some("alive")), "{a}");
    assert!(a["x"].is_number() && a["carried_mu"].is_object() && a["lineage"].is_string());
    assert!(r["observation"]["run"].is_string() && r["observation"]["tick"] == 80);
    // A recorded death: its tick and cause; a never-given ID: unknown; a number is fine while exact.
    let run = e.run(&w).unwrap();
    let ev = run.events().unwrap().0.into_owned();
    let dead = ev.iter().find_map(|x| match x { emerge_ref::world::Event::Died { id, tick, .. } => Some((*id, *tick)), _ => None }).expect("someone died by tick 80");
    let next = run.next_id();
    let r = ok(&mut e, "entities.get", json!({"world": w, "ids": [dead.0, (next + 5).to_string()]}));
    assert_eq!(r["agents"][0]["status"], "dead");
    assert_eq!(r["agents"][0]["death"]["tick"], dead.1);
    assert!(["starved", "old age"].contains(&r["agents"][0]["death"]["cause"].as_str().unwrap()));
    assert_eq!(r["agents"][1]["status"], "unknown");
    // A founder has no parent and says so; a child names its parent as a string.
    let born = ev.iter().find_map(|x| match x { emerge_ref::world::Event::Born { child, parent, .. } => Some((*child, *parent)), _ => None }).unwrap();
    let alive_child = all.iter().find(|id| ev.iter().any(|x| matches!(x, emerge_ref::world::Event::Born { child, .. } if child.to_string() == **id))).cloned();
    if let Some(c) = alive_child {
        let r = ok(&mut e, "entities.get", json!({"world": w, "ids": [c]}));
        assert!(r["agents"][0]["born"]["parent"].is_string(), "{}", r["agents"][0]);
    }
    let _ = born;
    let r = ok(&mut e, "entities.get", json!({"world": w, "ids": ["1"]}));
    assert!(r["agents"][0]["born"]["founder"] == true || r["agents"][0]["status"] != "alive");
    // IDs that are not exact as numbers are refused, not rounded.
    assert_eq!(err(&mut e, "entities.get", json!({"world": w, "ids": [9007199254740993u64]})).0, -32602);
    assert_eq!(err(&mut e, "entities.get", json!({"world": w, "ids": ["12a"]})).0, -32602);
    assert_eq!(err(&mut e, "entities.get", json!({"world": w, "ids": [1.5]})).0, -32602);
}

#[test]
fn ids_above_two_to_the_53_stay_exact_through_lookup_tracking_and_export() {
    let mut e = Engine::new();
    let w = world(&mut e, 5);
    // The API cannot make IDs this large; the runner's counter is set directly (a test-only state).
    e.run_mut(&w).unwrap().live.world.next_id = (1u64 << 53) + 1;
    ok(&mut e, "run.step", json!({"world": w, "ticks": 60}));
    let big: Vec<String> = living(&mut e, &w).into_iter().filter(|id| id.parse::<u64>().unwrap() > 1u64 << 53).collect();
    assert!(big.len() >= 2, "children with large IDs were born");
    let (a, b) = (big[0].clone(), big[1].clone());
    assert_ne!(a, b);
    let r = ok(&mut e, "entities.get", json!({"world": w, "ids": [a, b]}));
    assert_eq!((r["agents"][0]["id"].as_str(), r["agents"][1]["id"].as_str()), (Some(a.as_str()), Some(b.as_str())));
    assert_eq!(r["agents"][0]["status"], "alive");
    let w1 = ok(&mut e, "tracker.create", json!({"world": w, "kind": "watch", "ids": [a]}))["tracker"].as_str().unwrap().to_string();
    let q = ok(&mut e, "tracker.query", json!({"world": w, "tracker": w1}));
    assert_eq!(q["members"][0]["id"].as_str(), Some(a.as_str()));
    assert_eq!(q["tracker"]["name"].as_str(), Some(format!("Agent {a}").as_str()));
    let some: Vec<&String> = big.iter().take(32).collect();
    let t = ok(&mut e, "tracker.create", json!({"world": w, "ids": some}))["tracker"].as_str().unwrap().to_string();
    ok(&mut e, "tracker.record_start", json!({"world": w, "tracker": t, "every": 2}));
    ok(&mut e, "run.step", json!({"world": w, "ticks": 6}));
    let csv = ok(&mut e, "tracker.export", json!({"world": w, "tracker": t, "format": "csv"}))["text"].as_str().unwrap().to_string();
    let rows: Vec<&str> = csv.lines().filter(|l| !l.starts_with('#')).skip(1).collect();
    assert!(!rows.is_empty() && rows.iter().all(|l| some.iter().any(|id| l.split(',').nth(3) == Some(id.as_str()))), "{csv}");
    let j = ok(&mut e, "tracker.export", json!({"world": w, "tracker": t, "format": "json"}));
    assert!(j["rows"].as_array().unwrap().iter().all(|r| some.iter().any(|id| r[3].as_str() == Some(id.as_str()))));
}

#[test]
fn a_run_is_its_own_subject_and_a_restarted_servers_w1_is_another() {
    let mut e = Engine::new();
    let w = world(&mut e, 1);
    let run = ok(&mut e, "world.list", json!({}))["worlds"][0]["run"].as_str().unwrap().to_string();
    ok(&mut e, "entities.get", json!({"world": w, "run": run, "ids": ["1"]}));
    // Another engine (a restarted server) hands out `w1` again, with the same seed: not the same run.
    std::thread::sleep(std::time::Duration::from_millis(2));
    let mut e2 = Engine::new();
    let w2 = world(&mut e2, 1);
    assert_eq!(w, w2);
    let (code, msg) = err(&mut e2, "entities.get", json!({"world": w2, "run": run, "ids": ["1"]}));
    assert_eq!(code, -32012, "{msg}");
    assert_eq!(err(&mut e2, "tracker.create", json!({"world": w2, "run": run, "ids": ["1"]})).0, -32012);
}

#[test]
fn tracking_and_recording_change_nothing_in_the_run() {
    let mut a = Engine::new();
    let mut b = Engine::new();
    let wa = world(&mut a, 7);
    let wb = world(&mut b, 7);
    let ids = living(&mut a, &wa);
    let t1 = ok(&mut a, "tracker.create", json!({"world": wa, "kind": "watch", "ids": [ids[0], ids[1]]}))["tracker"].clone();
    ok(&mut a, "tracker.create", json!({"world": wa, "mode": "live", "where": {"attr": "age", "op": "<", "value": 5}}));
    ok(&mut a, "tracker.create", json!({"world": wa, "mode": "latched", "where": {"region": {"x": [0, 20], "y": [0, 20]}}, "from": 10, "until": 40}));
    ok(&mut a, "tracker.create", json!({"world": wa, "mode": "descendants", "root": ids[2]}));
    ok(&mut a, "tracker.create", json!({"world": wa, "mode": "representative", "by": "age"}));
    ok(&mut a, "tracker.record_start", json!({"world": wa, "tracker": t1, "every": 1, "interactions": true}));
    for _ in 0..12 {
        ok(&mut a, "run.step", json!({"world": wa, "ticks": 10}));
        ok(&mut a, "entities.get", json!({"world": wa, "ids": ids[..10]}));
        ok(&mut a, "entities.find", json!({"world": wa, "where": {"attr": "carried.body", "op": ">", "value": 1000}, "sort": "age"}));
        ok(&mut a, "tracker.update", json!({"world": wa, "tracker": t1, "display": {"color": "#ff0000"}}));
        ok(&mut b, "run.step", json!({"world": wb, "ticks": 10}));
    }
    let (ra, rb) = (a.run(&wa).unwrap(), b.run(&wb).unwrap());
    assert_eq!((ra.tick(), ra.revision()), (rb.tick(), rb.revision()));
    assert_eq!(ra.revision(), 0, "no tracking change is a source revision");
    assert_eq!(ra.digest(), rb.digest(), "tracking changed the trajectory");
    assert!(ra.track.revision > 12, "every definition change is an observation revision");
}

#[test]
fn a_watch_stays_on_its_creature_through_division_and_death() {
    let mut e = Engine::new();
    let w = world(&mut e, 11);
    // A creature that has a child, then dies, within 400 ticks of a twin run.
    let mut twin = Engine::new();
    let tw = world(&mut twin, 11);
    ok(&mut twin, "run.step", json!({"world": tw, "ticks": 400}));
    let ev = twin.run(&tw).unwrap().events().unwrap().0.into_owned();
    let pick = ev.iter().find_map(|x| match x {
        emerge_ref::world::Event::Born { parent, tick, .. } if *parent <= 200 && ev.iter().any(|d| matches!(d, emerge_ref::world::Event::Died { id, tick: dt, .. } if id == parent && dt > tick)) => Some(*parent),
        _ => None,
    }).expect("a founder that had a child and then died");
    let id = pick.to_string();
    let watch = ok(&mut e, "tracker.create", json!({"world": w, "kind": "watch", "ids": [id]}))["tracker"].as_str().unwrap().to_string();
    let fam = ok(&mut e, "tracker.create", json!({"world": w, "mode": "descendants", "root": id}))["tracker"].as_str().unwrap().to_string();
    ok(&mut e, "tracker.record_start", json!({"world": w, "tracker": watch, "every": 5}));
    ok(&mut e, "run.step", json!({"world": w, "ticks": 400}));
    let q = ok(&mut e, "tracker.query", json!({"world": w, "tracker": watch}));
    let m = q["members"].as_array().unwrap();
    assert_eq!(m.len(), 1, "a division does not add the child to an individual's watch");
    assert_eq!(m[0]["id"].as_str(), Some(id.as_str()));
    assert_eq!(m[0]["status"], "dead", "{}", m[0]);
    assert_eq!(m[0]["member"], true, "a death does not end membership");
    assert!(m[0]["last_observed"]["tick"].is_number(), "the last observation is kept and labelled");
    let f = ok(&mut e, "tracker.query", json!({"world": w, "tracker": fam}));
    assert!(f["members"].as_array().unwrap().len() >= 2, "the family took in the child");
    assert!(f["members"].as_array().unwrap().iter().any(|x| x["stays"][0]["why"].as_str().unwrap().starts_with(&format!("born to #{id}"))));
    let s = ok(&mut e, "tracker.samples", json!({"world": w, "tracker": watch}));
    let evs = s["events"].as_array().unwrap();
    assert!(evs.iter().any(|x| x["event"] == "child"), "the recording links the child: {evs:?}");
    assert!(evs.iter().any(|x| x["event"] == "death" && x["id"] == id.as_str()), "and the death with its cause");
    // After death nothing jumps to a nearby creature: the overlay list has nobody else.
    assert_eq!(e.run(&w).unwrap().track.shown().keys().filter(|k| k.to_string() != id).count() > 0, f["tracker"]["count_now"].as_u64().unwrap() > 1);
}

#[test]
fn membership_over_time_is_what_each_mode_says() {
    let mut e = Engine::new();
    let w = world(&mut e, 2);
    ok(&mut e, "run.step", json!({"world": w, "ticks": 30}));
    let half = ok(&mut e, "region.query", json!({"world": w, "x": [0, 1], "y": [0, 1]}))["world"]["w"].as_f64().unwrap() / 2.0;
    let live = ok(&mut e, "tracker.create", json!({"world": w, "mode": "live", "where": {"attr": "age", "op": "<", "value": 10}}))["tracker"].clone();
    let latched = ok(&mut e, "tracker.create", json!({"world": w, "mode": "latched", "where": {"region": {"x": [0.0, half], "y": [0.0, half]}}, "from": 30, "until": 40}))["tracker"].clone();
    let rep = ok(&mut e, "tracker.create", json!({"world": w, "mode": "representative", "by": "age", "highest": true}))["tracker"].clone();
    ok(&mut e, "run.step", json!({"world": w, "ticks": 40}));
    let q = |e: &mut Engine, t: &Value| ok(e, "tracker.query", json!({"world": w, "tracker": t}));
    let l = q(&mut e, &live);
    // Live: every current member matches now; someone left (aged out) and their stay says when.
    for m in l["members"].as_array().unwrap().iter().filter(|m| m["member"] == true && m["status"] == "alive") { assert!(m["age"].as_u64().unwrap() < 10, "{m}"); }
    assert!(l["members"].as_array().unwrap().iter().any(|m| m["member"] == false && m["stays"][0]["until"].is_number()), "members leave a live query");
    // Latched: members joined only in [30, 40) and stay afterwards though they have left the region.
    let la = q(&mut e, &latched);
    let ms = la["members"].as_array().unwrap();
    assert!(!ms.is_empty());
    for m in ms { let from = m["stays"][0]["from"].as_u64().unwrap(); assert!((30..40).contains(&from), "{m}"); assert_eq!(m["member"], true); }
    assert!(ms.iter().any(|m| m["status"] == "alive" && (m["x"].as_f64().unwrap() >= half || m["y"].as_f64().unwrap() >= half)), "kept after it stopped matching");
    // Representative: one member now; it is the oldest living, ties by lowest ID.
    let r = q(&mut e, &rep);
    let now: Vec<&Value> = r["members"].as_array().unwrap().iter().filter(|m| m["member"] == true).collect();
    assert_eq!(now.len(), 1);
    let all = ok(&mut e, "entities.find", json!({"world": w, "sort": "age", "descending": true, "limit": 1}));
    assert_eq!(now[0]["id"], all["agents"][0]["id"]);
    // A changed definition starts a new segment; the old stays keep their past.
    let before = l["tracker"]["segments"].as_array().unwrap().len();
    ok(&mut e, "tracker.update", json!({"world": w, "tracker": live, "where": {"attr": "age", "op": "<", "value": 3}}));
    let l2 = q(&mut e, &live);
    assert_eq!(l2["tracker"]["segments"].as_array().unwrap().len(), before + 1);
    // A condition on a substance the world lacks is refused, never read as zero.
    assert!(err(&mut e, "tracker.create", json!({"world": w, "mode": "live", "where": {"attr": "carried.gold", "op": ">", "value": 0}})).1.contains("substance `gold`"));
}

#[test]
fn a_recording_keeps_its_cadence_its_caps_and_its_provenance() {
    let mut e = Engine::new();
    let w = world(&mut e, 4);
    let ids = living(&mut e, &w);
    let t = ok(&mut e, "tracker.create", json!({"world": w, "ids": ids[..3]}))["tracker"].clone();
    let r = ok(&mut e, "tracker.record_start", json!({"world": w, "tracker": t, "every": 3, "substances": ["body"], "cap": 5, "policy": "evict"}));
    assert_eq!(r["first_sample_tick"], 1);
    // One call runs 30 ticks: the run takes every sample that is due, whatever the browser sees.
    ok(&mut e, "run.step", json!({"world": w, "ticks": 30}));
    let s = ok(&mut e, "tracker.samples", json!({"world": w, "tracker": t, "limit": 4}));
    let cov = &s["coverage"];
    assert_eq!(cov["sampled_ticks"], 10, "{cov}");
    assert_eq!((cov["first"].as_u64(), cov["last"].as_u64()), (Some(1), Some(28)));
    assert_eq!(s["schema"]["columns"].as_array().unwrap().last().unwrap(), "carried.body (µ)");
    // Evicted to the cap, and the eviction says how many per individual.
    assert!(cov["evicted"].as_object().unwrap().values().all(|v| v == 5), "{cov}");
    // Paging by sequence number: a reconnecting client asks after the last one it has and gets none twice.
    let mut seen = vec![];
    let mut after = None;
    loop {
        let p = ok(&mut e, "tracker.samples", json!({"world": w, "tracker": t, "after": after, "limit": 4}));
        for row in p["rows"].as_array().unwrap() { seen.push(row[0].as_u64().unwrap()); }
        if !p["more"].as_bool().unwrap() { break; }
        after = p["last_seq"].as_u64();
    }
    let mut d = seen.clone();
    d.dedup();
    assert_eq!(seen, d);
    assert_eq!(seen.len(), 15, "three individuals, five samples each");
    // A stop-policy recording stops at the cap and says why.
    let t2 = ok(&mut e, "tracker.create", json!({"world": w, "ids": [ids[5]]}))["tracker"].clone();
    ok(&mut e, "tracker.record_start", json!({"world": w, "tracker": t2, "every": 1, "cap": 4}));
    ok(&mut e, "run.step", json!({"world": w, "ticks": 10}));
    let s2 = ok(&mut e, "tracker.samples", json!({"world": w, "tracker": t2}));
    assert!(s2["coverage"]["stopped"]["why"].as_str().unwrap().contains("cap of 4"));
    assert_eq!(s2["rows"].as_array().unwrap().len(), 4);
    // Export: provenance with source hash, seed, cadence and the selection rule.
    let x = ok(&mut e, "tracker.export", json!({"world": w, "tracker": t, "format": "json"}));
    let p = &x["provenance"];
    assert!(p["source_hash"].is_string() && p["seed"] == 4 && p["sampling"]["every"] == 3 && p["selection_rule"].as_str().unwrap().contains("these individuals"));
    // Stopping keeps the data; deleting it is a separate, explicit act.
    ok(&mut e, "tracker.stop", json!({"world": w, "tracker": t}));
    assert!(err(&mut e, "tracker.remove", json!({"world": w, "tracker": t})).1.contains("delete_data"));
    ok(&mut e, "tracker.remove", json!({"world": w, "tracker": t, "delete_data": true}));
}

#[test]
fn definitions_have_their_own_revision_undo_and_limits() {
    let mut e = Engine::new();
    let w = world(&mut e, 6);
    let ids = living(&mut e, &w);
    let r = ok(&mut e, "tracker.create", json!({"world": w, "kind": "watch", "ids": [ids[0]], "request": "click-1"}));
    let again = ok(&mut e, "tracker.create", json!({"world": w, "kind": "watch", "ids": [ids[0]], "request": "click-1"}));
    assert_eq!(r["tracker"], again["tracker"], "a create sent twice makes one tracker");
    let t = r["tracker"].clone();
    ok(&mut e, "tracker.update", json!({"world": w, "tracker": t, "name": "Interesting drifter", "base": 1}));
    let (code, _) = err(&mut e, "tracker.update", json!({"world": w, "tracker": t, "name": "late", "base": 1}));
    assert_eq!(code, -32001, "a change made against an old definition is stale");
    ok(&mut e, "tracker.undo", json!({"world": w}));
    assert_eq!(ok(&mut e, "tracker.list", json!({"world": w}))["trackers"][0]["name"].as_str().unwrap(), format!("Agent {}", ids[0]));
    ok(&mut e, "tracker.redo", json!({"world": w}));
    assert_eq!(ok(&mut e, "tracker.list", json!({"world": w}))["trackers"][0]["name"], "Interesting drifter");
    // Undo puts back definitions, never observations: a live query's history stays, and a recording blocks undoing its tracker.
    let live = ok(&mut e, "tracker.create", json!({"world": w, "mode": "live", "where": {"attr": "age", "op": "<", "value": 3}}))["tracker"].clone();
    ok(&mut e, "run.step", json!({"world": w, "ticks": 20}));
    let ever = |e: &mut Engine| ok(e, "tracker.query", json!({"world": w, "tracker": live}))["tracker"]["ever"].as_u64().unwrap();
    let before = ever(&mut e);
    ok(&mut e, "tracker.update", json!({"world": w, "tracker": live, "name": "young"}));
    ok(&mut e, "tracker.undo", json!({"world": w}));
    assert!(ever(&mut e) >= before, "undoing a rename forgot no member");
    let rec = ok(&mut e, "tracker.create", json!({"world": w, "ids": [ids[1], ids[2]]}))["tracker"].clone();
    ok(&mut e, "tracker.record_start", json!({"world": w, "tracker": rec, "every": 1}));
    ok(&mut e, "run.step", json!({"world": w, "ticks": 2}));
    assert!(err(&mut e, "tracker.undo", json!({"world": w})).1.contains("holds a recording"), "undoing the creation of a tracker with a recording is refused");
    // At most 32 individuals are watched at once.
    for id in &ids[1..32] { ok(&mut e, "tracker.create", json!({"world": w, "kind": "watch", "ids": [id]})); }
    assert!(err(&mut e, "tracker.create", json!({"world": w, "kind": "watch", "ids": [ids[40]]})).1.contains("at most 32"));
    // A cohort is not a quick watch: it can be larger.
    ok(&mut e, "tracker.create", json!({"world": w, "ids": ids[..100]}));
    // A fixed set from another run's definition needs a fresh choice of individuals.
    assert!(err(&mut e, "tracker.create", json!({"world": w, "definition": {"mode": "fixed"}})).1.contains("choose them again"));
}

#[test]
fn a_tracker_shares_the_transfer_recording_and_never_takes_it_silently() {
    let mut e = Engine::new();
    let w = world(&mut e, 8);
    let ids = living(&mut e, &w);
    // The Rule Lens records only one rule's statements: a tracker that needs creatures' actions is refused.
    ok(&mut e, "run.step", json!({"world": w, "ticks": 1}));
    let act = ok(&mut e, "rules.activity", json!({"world": w}));
    let path = act["rows"][0]["path"].as_str().unwrap().to_string();
    ok(&mut e, "trace.start", json!({"world": w, "ticks": 50, "rules": [path], "creatures": false}));
    let t = ok(&mut e, "tracker.create", json!({"world": w, "ids": ids[..5]}))["tracker"].clone();
    assert!(err(&mut e, "tracker.record_start", json!({"world": w, "tracker": t, "interactions": true})).1.contains("held by trace.start"));
    // Once it has ended, the tracker takes the slot; the Rule Lens is then refused unless it says replace.
    ok(&mut e, "run.step", json!({"world": w, "ticks": 50}));
    ok(&mut e, "tracker.record_start", json!({"world": w, "tracker": t, "interactions": true, "stop_at": 120}));
    assert!(err(&mut e, "trace.start", json!({"world": w, "ticks": 10})).1.contains(&format!("held by tracker {}", t.as_str().unwrap())));
    ok(&mut e, "run.step", json!({"world": w, "ticks": 30}));
    let i = ok(&mut e, "tracker.interactions", json!({"world": w, "tracker": t}));
    assert_eq!(i["owner"].as_str(), Some(format!("tracker {}", t.as_str().unwrap()).as_str()));
    for r in i["records"].as_array().unwrap() {
        let ends = [r["from"].as_str().unwrap(), r["to"].as_str().unwrap()];
        assert!(ends.iter().any(|x| ids[..5].iter().any(|id| *x == format!("creature #{id}"))), "{r}");
    }
    ok(&mut e, "trace.start", json!({"world": w, "ticks": 10, "replace": true}));
}

#[test]
fn a_find_is_one_observation_paged_without_skips_or_repeats() {
    let mut e = Engine::new();
    let w = world(&mut e, 9);
    ok(&mut e, "run.step", json!({"world": w, "ticks": 20}));
    let first = ok(&mut e, "entities.find", json!({"world": w, "where": {"attr": "lineage", "op": "==", "value": "Drifter"}, "sort": "age", "limit": 25}));
    let cursor = first["cursor"].as_str().unwrap().to_string();
    let matched = first["matched"].as_u64().unwrap() as usize;
    let mut got: Vec<String> = first["agents"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap().to_string()).collect();
    // The world moves on between pages; the pages are of the first observation.
    ok(&mut e, "run.step", json!({"world": w, "ticks": 5}));
    let mut off = 25;
    while off < matched {
        let p = ok(&mut e, "entities.find", json!({"world": w, "cursor": cursor, "offset": off, "limit": 25}));
        assert_eq!(p["tick"], 20);
        got.extend(p["agents"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap().to_string()));
        off += 25;
    }
    let mut d = got.clone();
    d.sort();
    d.dedup();
    assert_eq!((got.len(), d.len()), (matched, matched));
    assert!(first["agents"].as_array().unwrap().iter().all(|a| a["lineage"] == "Drifter"));
    let ages: Vec<u64> = first["agents"].as_array().unwrap().iter().map(|a| a["age"].as_u64().unwrap()).collect();
    assert!(ages.windows(2).all(|p| p[0] <= p[1]));
    // Old cursors expire.
    for _ in 0..5 { ok(&mut e, "entities.find", json!({"world": w})); }
    assert_eq!(err(&mut e, "entities.find", json!({"world": w, "cursor": cursor})).0, -32010);
}

/// The JSON header of the next frame of tick `tick` (a changed selection or tracker also sends a frame of the same tick).
fn header_at(ws: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>, tick: u64) -> Value {
    loop {
        let h = header(ws);
        if h["tick"] == tick { return h; }
    }
}

/// The next binary frame's JSON header, answering nothing else.
fn header(ws: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>) -> Value {
    loop {
        if let tungstenite::Message::Binary(b) = ws.read().unwrap() {
            let n = u32::from_le_bytes(b[..4].try_into().unwrap()) as usize;
            return serde_json::from_slice(&b[4..4 + n]).unwrap();
        }
    }
}

#[test]
fn a_click_picks_the_creature_the_displayed_frame_showed() {
    use tungstenite::Message;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || emerge_server::serve::serve_with(listener, None, None));
    let (mut ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{port}/")).unwrap();
    let mut n = 0;
    let mut send = |ws: &mut tungstenite::WebSocket<_>, method: &str, params: Value| -> Result<Value, Value> {
        n += 1;
        ws.send(Message::Text(json!({"jsonrpc": "2.0", "id": n, "method": method, "params": params}).to_string().into())).unwrap();
        loop {
            if let Message::Text(t) = ws.read().unwrap() {
                let v: Value = serde_json::from_str(&t).unwrap();
                return match v.get("error") { Some(e) => Err(e.clone()), None => Ok(v["result"].clone()) };
            }
        }
    };
    let views = include_str!("../../../examples/views/dish.em");
    let w = send(&mut ws, "world.create", json!({"source": small(DISH), "lineages": ["Drifter", "Builder"], "seed": 4})).unwrap()["world"].clone();
    let sub = send(&mut ws, "view.subscribe", json!({"world": w, "views": views, "view": "Lineages", "cell": 4, "pick": true})).unwrap();
    let s = sub["subscription"].clone();
    assert!(sub["run"].is_string());
    let h0 = header(&mut ws);
    assert!(h0["frame"].is_u64() && h0["run"] == sub["run"] && h0["pickable"] == true && h0["drawn_agents"].as_u64().unwrap() > 0, "{h0}");
    // A creature where frame 0 drew it.
    let all = send(&mut ws, "entities.find", json!({"world": w, "limit": 1})).unwrap();
    let a = &all["agents"][0];
    let (kx, ky) = (h0["map"]["w"].as_f64().unwrap() / h0["map"]["world_w"].as_f64().unwrap(), h0["map"]["h"].as_f64().unwrap() / h0["map"]["world_h"].as_f64().unwrap());
    let (px, py) = (a["x"].as_f64().unwrap() * kx, a["y"].as_f64().unwrap() * ky);
    let pick = |r: &Value| r["candidates"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let p0 = send(&mut ws, "view.pick_frame", json!({"subscription": s, "frame": h0["frame"], "x": px, "y": py, "tolerance": 2})).unwrap();
    assert!(pick(&p0).contains(&a["id"].as_str().unwrap().to_string()), "{p0}");
    assert_eq!(p0["tick"], 0);
    let ds: Vec<f64> = p0["candidates"].as_array().unwrap().iter().map(|c| c["distance_px"].as_f64().unwrap()).collect();
    assert!(ds.windows(2).all(|p| p[0] <= p[1]), "nearest first");
    // Selecting it: its place comes with the next frame, from that frame.
    send(&mut ws, "view.watch", json!({"subscription": s, "ids": [a["id"]]})).unwrap();
    // A changed selection sends a fresh frame of the same tick, so a paused world's outline is current.
    let hw = header_at(&mut ws, 0);
    assert_ne!(hw["frame"], h0["frame"]);
    assert!(hw["agents"].as_array().unwrap().iter().any(|x| x["id"] == a["id"] && x["selected"] == true));
    send(&mut ws, "run.step", json!({"world": w, "ticks": 8})).unwrap();
    let h1 = header_at(&mut ws, 8);
    let mine = h1["agents"].as_array().unwrap().iter().find(|x| x["id"] == a["id"]).cloned().expect("the selection travels with the frame");
    assert_eq!((mine["selected"].as_bool(), h1["tick"].as_u64()), (Some(true), Some(8)));
    let now = send(&mut ws, "entities.get", json!({"world": w, "ids": [a["id"]]})).unwrap();
    assert_eq!((mine["x"].as_f64(), mine["y"].as_f64()), (now["agents"][0]["x"].as_f64(), now["agents"][0]["y"].as_f64()), "the overlay is the frame's own position");
    // The creature has moved, but a click on the tick-0 picture still names what that picture showed.
    let again = send(&mut ws, "view.pick_frame", json!({"subscription": s, "frame": hw["frame"], "x": px, "y": py, "tolerance": 2})).unwrap();
    assert_eq!(pick(&again), pick(&p0));
    // Two frames later, frame 0 is no longer kept: the answer says so instead of picking against the present.
    send(&mut ws, "run.step", json!({"world": w, "ticks": 1})).unwrap();
    header_at(&mut ws, 9);
    let gone = send(&mut ws, "view.pick_frame", json!({"subscription": s, "frame": hw["frame"], "x": px, "y": py})).unwrap_err();
    assert_eq!((gone["code"].as_i64(), gone["data"]["kind"].as_str()), (Some(-32011), Some("frame_expired")), "{gone}");
    // A visible tracker's members come with the frame too, naming the tracker.
    let t = send(&mut ws, "tracker.create", json!({"world": w, "ids": [all["agents"][0]["id"]]})).unwrap()["tracker"].clone();
    send(&mut ws, "view.watch", json!({"subscription": s, "ids": []})).unwrap();
    send(&mut ws, "run.step", json!({"world": w, "ticks": 1})).unwrap();
    let h2 = header_at(&mut ws, 10);
    let m = h2["agents"].as_array().unwrap().iter().find(|x| x["id"] == a["id"]).cloned().unwrap();
    assert_eq!((m["selected"].as_bool(), m["trackers"][0].clone()), (Some(false), t));
    assert_eq!(h2["tracking_revision"], 1);
}
