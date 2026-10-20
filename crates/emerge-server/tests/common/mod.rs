//! A journalled run, shared by the CPU and GPU journal tests.

#![allow(dead_code)]

use emerge_server::journal::Journal;
use emerge_server::run::{Run, Spec};

const DISH: &str = include_str!("../../../../examples/dish.em");

pub fn spec(gpu: bool) -> Spec {
    let source = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;");
    Spec { source, lineages: vec!["Drifter".into(), "Builder".into()], seed: 9, mutation: true, contract: false, gpu }
}

/// A run with three committed and two rejected swaps over 120 ticks; its journal text.
pub fn journalled(gpu: bool) -> (Run, String) {
    let mut run = Run::create(spec(gpu), Journal::memory()).unwrap();
    run.step(10);
    run.hotswap("patch Dish {\n    set rules.rotting@react_rot.rate = 90bp;\n}\n", 0, "faster rot").unwrap();
    run.step(15);
    assert!(run.hotswap("patch Dish {\n    set upkeep.base = 30mu;\n}\n", 0, "stale on purpose").is_err());
    run.step(5);
    run.hotswap("patch Drifter {\n    set mutation.rate = 0.1;\n}\n", 1, "more variation").unwrap();
    assert!(run.hotswap("patch Dish {\n    set space.max_interaction_radius = 2.0;\n}\n", 2, "not swappable").is_err());
    run.step(40);
    run.hotswap("patch Dish {\n    add after rules.spread: rule wind on cells { diffuse_conserved([carbon], rate: 100bp) @wind; }\n}\n", 2, "wind").unwrap();
    run.step(50);
    run.close();
    assert!(run.population() > 0 && run.tick() == 120, "the run should be alive at tick 120");
    let text = run.journal.text();
    (run, text)
}
