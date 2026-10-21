//! Worlds at the limits of the GPU kernels: many requests for one store, distinct stores sharing a
//! bucket of the grouping sort, and more world laws than the fixed tables of the first versions held.
//! Each compares whole ticks with the CPU in contract mode. Needs the GPU (`cargo test -p emerge-gpu`).

use emerge_gpu::gpu::Gpu;
use emerge_gpu::m3::compare_full_with;
use emerge_ref::compile::{compile, Options};

const DISH: &str = include_str!("../../../examples/dish.em");

fn world(src: &str, lineage: &str) -> emerge_ref::World {
    let mut w = compile(src, &[lineage], Options { seed: 7, allow_partial: true, mutation: false }).unwrap().world;
    w.contract = true;
    w
}

fn edit(src: &str, from: &str, to: &str) -> String {
    assert!(src.contains(from), "the dish no longer has `{from}`");
    src.replacen(from, to, 1)
}

fn clean(name: &str, (states, bad_entities, bad_cells): (u64, u64, u64)) {
    assert!(states > 2 && bad_entities == 0 && bad_cells == 0, "{name}: {states} states, {bad_entities} entities, {bad_cells} cells differ");
}

/// A few cells and `count` Builders: every one asks the same few stores, far more than a tile of 256.
fn crowded(cells: &str, count: &str) -> String {
    let src = edit(DISH, "field cells : Grid(w: 256, h: 256)", &format!("field cells : Grid({cells})"));
    edit(&src, "genesis {\n        count: 2_000;\n        position = uniform(x: 0.0..512.0, y: 0.0..512.0);\n        transfer(energy, from: spark, to: self, amount: 5u) @transfer_energy;\n    }\n}\n\nview", &format!("genesis {{\n        count: {count};\n        position = uniform(x: 0.0..512.0, y: 0.0..512.0);\n        transfer(energy, from: spark, to: self, amount: 5u) @transfer_energy;\n    }}\n}}\n\nview"))
}

#[test]
fn one_store_asked_by_many() {
    let gpu = Gpu::new();
    clean("one cell, 600 Builders", compare_full_with(&mut world(&crowded("w: 1, h: 1", "600"), "Builder"), &gpu, 12, 1, true, |_| {}));
}

#[test]
fn one_store_asked_by_thousands() {
    // 5_000 Builders on one cell: a group of thousands, whose leftover µ `share` gives out by bisection
    // rather than one at a time (a draw per member per µ was some 10^7 draws in one thread).
    let gpu = Gpu::new();
    let t = std::time::Instant::now();
    clean("one cell, 5_000 Builders", compare_full_with(&mut world(&crowded("w: 1, h: 1", "5_000"), "Builder"), &gpu, 6, 1, true, |_| {}));
    println!("5_000 on one cell, 6 ticks on both runners: {:.2} s", t.elapsed().as_secs_f64());
}

#[test]
fn distinct_stores_in_one_bucket() {
    let gpu = Gpu::new();
    // 64 cells, so each is asked by about 30 Builders; with one or two buckets the keys share them.
    let src = crowded("w: 8, h: 8", "2_000");
    for most in [1, 2] {
        clean(&format!("{most} buckets"), compare_full_with(&mut world(&src, "Builder"), &gpu, 12, 1, true, |g| g.limit_buckets(most)));
    }
    clean("default dish, 1 bucket", compare_full_with(&mut world(DISH, "Builder"), &gpu, 12, 3, true, |g| g.limit_buckets(1)));
}

#[test]
fn many_world_laws() {
    let gpu = Gpu::new();
    // 70 reaction laws, and 300 inflows of carbon and 300 of nitrate into one cell that cannot hold them.
    let reacts: String = (0..70).map(|_| "        react(rot, rate: 1bp);\n").collect();
    let src = edit(&crowded("w: 4, h: 4", "200"), "rule rotting on cells {\n", &format!("rule rotting on cells {{\n{reacts}"));
    clean("70 reaction laws", compare_full_with(&mut world(&src, "Builder"), &gpu, 8, 1, true, |_| {}));
    let inflows: String = (0..300).map(|_| "        transfer(carbon, from: rock, to: self, amount: 20u, overflow: void);\n        transfer(nitrate, from: rock, to: self, amount: 20u, overflow: void);\n").collect();
    let src = edit(&crowded("w: 1, h: 1", "50"), "rule weather on cells {\n", &format!("rule weather on cells {{\n{inflows}"));
    clean("600 inflows", compare_full_with(&mut world(&src, "Builder"), &gpu, 8, 1, true, |_| {}));
}
