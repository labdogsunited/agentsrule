//! Catalysts (spec §4.5) on the GPU: an entity reaction (respiration, catalysed by nitrate) and a world
//! reaction (rot, catalysed by carbon) equal the CPU's in contract mode. Needs the GPU.

use emerge_gpu::gpu::Gpu;
use emerge_gpu::m3::{compare_full, compare_transform};
use emerge_ref::compile::{compile, Options};

const DISH: &str = include_str!("../../../examples/dish.em");

fn catalysed() -> String {
    let src = DISH.replacen("reaction respiration : sugar -> energy + carbon;", "reaction respiration : sugar -> energy + carbon catalyze nitrate(half: 1u, boost: 4.0);", 1);
    let src = src.replacen("reaction rot : body -> carbon + nitrate;", "reaction rot : body -> carbon + nitrate catalyze carbon(half: 10u, boost: 30.0);", 1);
    assert_eq!(src.matches("catalyze").count(), 2, "the dish no longer has the reactions this test catalyses");
    src
}

fn world(src: &str, lineage: &str, seed: u64, mutation: bool) -> emerge_ref::World {
    let mut w = compile(src, &[lineage], Options { seed, allow_partial: true, mutation }).unwrap().world;
    w.contract = true;
    w
}

#[test]
fn catalysts_change_the_run() {
    // Without this, the comparisons below could pass with the catalysts ignored on both sides.
    let (mut plain, mut fast) = (world(DISH, "Builder", 3, false), world(&catalysed(), "Builder", 3, false));
    let total = |w: &emerge_ref::World, s: usize| w.cells.iter().map(|c| c.amounts[s] as u64).sum::<u64>();
    for _ in 0..20 {
        plain.run_tick();
        fast.run_tick();
    }
    assert_ne!(plain.entity_extent, fast.entity_extent, "the entity catalyst had no effect");
    let body = fast.chem.substances.iter().position(|s| s.name == "body").unwrap();
    assert!(total(&fast, body) < total(&plain, body), "the world catalyst did not speed up rot");
}

#[test]
fn catalysed_reactions_match_the_cpu() {
    let gpu = Gpu::new();
    let src = catalysed();
    for lineage in ["Drifter", "Builder"] {
        let r = compare_transform(&mut world(&src, lineage, 2, true), &gpu, 60, 10, true);
        assert!(r.clean(), "{lineage}: {} {}", r.bad_amounts, r.bad_cells);
        let (states, be, bc) = compare_full(&mut world(&src, lineage, 4, true), &gpu, 100, 10, true);
        assert!(states > 5 && be == 0 && bc == 0, "{lineage}: {states} states, {be} entities, {bc} cells differ");
    }
}
