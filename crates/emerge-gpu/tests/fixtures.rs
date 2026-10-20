//! Contract fixtures (spec §10.2, exit test 3): the cases of `emerge-ref/tests/slice.rs`, rebuilt on a
//! laid grid with fixed positions and axis-aligned headings, each compared with the CPU in contract mode
//! every tick (full state, births, deaths, reaction extent). Needs the GPU.

#[path = "../../emerge-ref/tests/common/mod.rs"]
mod common;

use common::{dish, Dish};
use emerge_gpu::gpu::Gpu;
use emerge_gpu::m3::compare_full;
use emerge_ref::world::Space;
use emerge_ref::{End, Gene, Genome, Law, Mu, World, U};

/// A world with Dish's chemistry on an 8 × 8 wrapping space with a 4 × 4 grid; `caps` per substance
/// for every cell (Dish's by default: 2_000u of light, sugar, carbon, nitrate and body).
fn world(base: Mu, caps: Option<Vec<Mu>>) -> (World, Dish) {
    let (mut w, d) = dish(base, 0);
    let caps = caps.unwrap_or_else(|| {
        let mut c = vec![0; w.chem.substances.len()];
        for s in [d.light, d.sugar, d.carbon, d.nitrate, d.body] {
            c[s] = 2_000 * U;
        }
        c
    });
    w.grid(Space { w: 8.0, h: 8.0, wrap: true, grid_w: 4, grid_h: 4, child_offset: 0.5, radius: 2.0 }, caps);
    w.contract = true;
    (w, d)
}

/// An entity in cell 0, heading east.
fn at(w: &mut World, genes: Vec<Gene>) -> usize {
    w.spawn_at([1.0, 1.0], [1.0, 0.0], Genome::simple(genes))
}

/// Compares `ticks` ticks with the CPU and returns the CPU world, so a case can check that it still
/// does what its `slice.rs` original checks.
fn same(name: &str, mut w: World, ticks: u64) -> World {
    let gpu = Gpu::new();
    let (states, be, bc) = compare_full(&mut w, &gpu, ticks, 1, true);
    assert!(states == ticks && be == 0 && bc == 0, "{name}: {states} states, {be} entities, {bc} cells differ");
    w
}

#[test]
fn upkeep_dying_and_fees() {
    let (mut w, d) = world(20, None);
    let e = at(&mut w, vec![]);
    w.endow(End::Entity(e), d.energy, 1_010);
    let w = same("starving entity", w, 60);
    assert_eq!((w.deaths, w.entities.len()), (1, 0));

    let (mut w, d) = world(20, None);
    w.endow(End::Cell(0), d.carbon, 5 * U);
    let e = at(&mut w, vec![Gene::take(d.carbon, U, 0), Gene::divide(5_000, (d.body, 0), 0)]);
    w.endow(End::Entity(e), d.energy, 19);
    w.endow(End::Entity(e), d.body, 10 * U);
    same("dying entity", w, 3);

    let (mut w, d) = world(20, None);
    w.endow(End::Cell(0), d.carbon, 10 * U);
    w.endow(End::Cell(0), d.nitrate, 10 * U);
    let e = at(&mut w, vec![Gene::take(d.carbon, U, 5), Gene::take(d.nitrate, U, 10)]);
    w.endow(End::Entity(e), d.energy, 30);
    same("fees in genome order", w, 3);
}

#[test]
fn contention_remainders_and_overflow() {
    for seed in 0..20 {
        let (mut w, d) = world(0, None);
        w.seed = seed;
        w.endow(End::Cell(0), d.carbon, 1_000);
        for _ in 0..3 {
            at(&mut w, vec![Gene::take(d.carbon, 700, 0)]);
        }
        let w = same(&format!("pro-rata takes, seed {seed}"), w, 2);
        assert_eq!(w.cells[0].amounts[d.carbon], 0);
    }

    // Cells that hold 1_000µ of carbon and nothing else.
    let (plain, d) = dish(0, 0);
    let mut caps = vec![0; plain.chem.substances.len()];
    caps[d.carbon] = 1_000;
    let (mut w, d) = world(0, Some(caps));
    w.endow(End::Cell(0), d.carbon, 900);
    let rock2 = w.add_source("rock2");
    w.laws.push(Law::Inflow { s: d.carbon, source: d.rock, amount: 100, overflow: Some(d.void) });
    w.laws.push(Law::Inflow { s: d.carbon, source: rock2, amount: 300, overflow: Some(d.void) });
    let w = same("two origins into a nearly full cell", w, 3);
    assert_eq!(w.cells[0].amounts[d.carbon], 1_000);

    let (mut w, d) = world(0, None);
    w.endow(End::Cell(0), d.carbon, 5 * U);
    let e = at(&mut w, vec![Gene::take(d.carbon, 2 * U, 0)]);
    w.endow(End::Entity(e), d.carbon, 99 * U);
    same("no overflow sink: the excess stays", w, 3);

    let (mut w, d) = world(0, None);
    w.endow(End::Cell(0), d.light, 2_000 * U);
    w.laws.push(Law::Inflow { s: d.light, source: d.sun, amount: 2 * U, overflow: Some(d.void) });
    at(&mut w, vec![Gene::take(d.light, U, 0)]);
    same("world and entity transfers share one arbitration", w, 4);

    let (mut w, d) = world(0, None);
    w.endow(End::Cell(0), d.body, 200);
    w.laws.push(Law::Diffuse { substances: vec![d.body], rate_bp: 300 });
    at(&mut w, vec![Gene::take(d.body, U, 0)]);
    same("a seed shares a cell with diffusion", w, 4);
}

#[test]
fn reaction_order() {
    for order in [[0, 1], [1, 0]] {
        let (mut w, d) = world(0, None);
        let genes = [Gene::react(d.photosynthesis, U), Gene::react(d.respiration, U)];
        let e = at(&mut w, order.iter().map(|&k| genes[k].clone()).collect());
        w.endow(End::Entity(e), d.light, U);
        w.endow(End::Entity(e), d.carbon, U);
        same(&format!("reactions in order {order:?}"), w, 3);
    }

    let (mut w, d) = world(0, None);
    w.laws.push(Law::Inflow { s: d.body, source: d.rock, amount: 1, overflow: Some(d.void) });
    w.laws.push(Law::React { reaction: d.rot, rate_bp: 50 });
    same("world reactions after transfers", w, 400);
}

#[test]
fn division_and_disposal() {
    let (mut w, d) = world(0, None);
    w.endow(End::Cell(0), d.body, 100 * U);
    let e = at(&mut w, vec![Gene::take(d.body, U, 0), Gene::divide(5_000, (d.body, 10 * U), 0)]);
    w.endow(End::Entity(e), d.body, 9_500);
    w.endow(End::Entity(e), d.energy, 3);
    let w = same("division after transform", w, 3);
    assert!(w.births >= 1);

    let (mut w, d) = world(0, None);
    w.endow(End::Cell(0), d.body, 100 * U);
    let e = at(&mut w, vec![Gene::take(d.body, U, 0), Gene::divide(5_000, (d.body, 20 * U), 0)]);
    w.endow(End::Entity(e), d.body, 19 * U);
    same("newborns act next tick", w, 4);

    for most in [2, 3] {
        let (mut w, d) = world(0, None);
        let a = at(&mut w, vec![Gene::divide(5_000, (d.body, U), 0)]);
        let b = at(&mut w, vec![Gene::divide(5_000, (d.body, U), 10)]);
        w.endow(End::Entity(a), d.body, 2 * U);
        w.endow(End::Entity(b), d.body, 2 * U);
        w.endow(End::Entity(b), d.energy, 9);
        w.max_entities = most;
        let w = same(&format!("division at max_entities {most} and an unpaid fee"), w, 3);
        assert_eq!(w.births, most as u64 - 2, "only `a` divides, and only below max_entities");
    }

    for decompose in [false, true] {
        let (mut w, d) = world(0, None);
        w.endow(End::Cell(0), if decompose { d.carbon } else { d.body }, 1_999 * U);
        let genes = if decompose { vec![] } else { vec![Gene::divide(5_000, (d.body, U), 0)] };
        let e = at(&mut w, genes);
        w.endow(End::Entity(e), d.body, 3 * U);
        w.endow(End::Entity(e), d.energy, 2 * U);
        w.max_age = 1;
        if decompose {
            w.decompose = Some(d.rot);
        }
        let w = same(&format!("disposal (decompose {decompose})"), w, 3);
        assert_eq!((w.deaths, w.births), (1, 0));
    }
}
