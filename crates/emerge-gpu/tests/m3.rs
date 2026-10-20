//! M3 as a regression test: Transform, Restructure and whole ticks kept on the GPU equal the
//! CPU's in contract mode (mutation off). Needs the GPU (`cargo test -p emerge-gpu`).

use emerge_gpu::gpu::Gpu;
use emerge_gpu::m3::{compare_full, compare_restructure, compare_transform};
use emerge_ref::compile::{compile, Options};

const DISH: &str = include_str!("../../../examples/dish.em");

fn world(lineage: &str, seed: u64, mutation: bool) -> emerge_ref::World {
    let mut w = compile(DISH, &[lineage], Options { seed, allow_partial: true, mutation }).unwrap().world;
    w.contract = true;
    w
}

#[test]
fn transform_matches_the_cpu() {
    let gpu = Gpu::new();
    for lineage in ["Drifter", "Builder"] {
        let r = compare_transform(&mut world(lineage, 2, true), &gpu, 120, 20, true);
        assert!(r.clean(), "{lineage}: {} {}", r.bad_amounts, r.bad_cells);
    }
}

#[test]
fn restructure_matches_the_cpu() {
    let gpu = Gpu::new();
    for lineage in ["Drifter", "Builder"] {
        let r = compare_restructure(&mut world(lineage, 3, false), &gpu, 120, 20, true);
        assert!(r.clean(), "{lineage}: {} {} {}", r.bad_counts, r.bad_entities, r.bad_cells);
        assert!(r.children > 0 && r.removed > 0, "{lineage}: no births or deaths were compared");
    }
}

#[test]
fn what_each_entity_made_matches_the_cpu() {
    // `Entity::made` for the tracked substance (exit test 9's producers); compare_state checks it.
    let gpu = Gpu::new();
    let mut w = world("Builder", 6, true);
    w.track = w.chem.substances.iter().position(|s| s.name == "body");
    let (states, bad_entities, bad_cells) = compare_full(&mut w, &gpu, 60, 10, true);
    assert!(states > 5 && bad_entities == 0 && bad_cells == 0, "{states} states, {bad_entities} entities, {bad_cells} cells differ");
    assert!(w.entities.iter().any(|e| e.made > 0), "no entity made body, so nothing was compared");
}

#[test]
fn whole_ticks_kept_on_the_gpu_match_the_cpu() {
    let gpu = Gpu::new();
    for (lineage, seed) in [("Drifter", 4), ("Builder", 5)] {
        let (states, bad_entities, bad_cells) = compare_full(&mut world(lineage, seed, false), &gpu, 100, 10, true);
        assert!(states > 5 && bad_entities == 0 && bad_cells == 0, "{lineage}: {states} states, {bad_entities} entities, {bad_cells} cells differ");
    }
}
