//! `lineage B extends A`: the same world as the same lineage written out in full.

use emerge_ref::compile::{compile, Options};

const DISH: &str = include_str!("../../../examples/dish.em");

/// `examples/dish.em` with `Builder` written as `Drifter` plus its two genes.
fn with_extends() -> String {
    let start = DISH.find("lineage Builder in Dish {").expect("Builder");
    let end = start + DISH[start..].find("\n}\n").expect("end of Builder") + 3;
    let short = "lineage Builder extends Drifter in Dish {\n    seed genome {\n        module metabolism {\n            reaction build rate 50mu @build after @respiration;\n            action take(s: nitrate, max: 250mu) @eat_nitrate after @eat_carbon;\n        }\n    }\n}\n";
    format!("{}{short}{}", &DISH[..start], &DISH[end..])
}

fn fingerprint(src: &str, ticks: u64) -> Vec<String> {
    let mut w = compile(src, &["Builder"], Options { seed: 3, allow_partial: true, mutation: true }).unwrap_or_else(|e| panic!("{e:?}")).world;
    for _ in 0..ticks {
        w.run_tick();
    }
    let mut out = vec![format!("tick {} births {} deaths {}", w.tick, w.births, w.deaths)];
    for e in &w.entities {
        out.push(format!("{} {:?} {:?} {:?}", e.id, e.pos, e.store.amounts, e.genome));
    }
    out
}

#[test]
fn an_extended_lineage_runs_exactly_as_the_one_written_out() {
    let short = with_extends();
    assert!(short.len() < DISH.len());
    let (a, b) = (fingerprint(DISH, 150), fingerprint(&short, 150));
    assert!(a.len() > 100, "the run should keep a population");
    assert_eq!(a, b);
}
