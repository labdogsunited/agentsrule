//! The reference world, `examples/dish.em`, translated by hand.
//!
//! This exists only until the parser does: then `examples/dish.em` is compiled instead,
//! and this file is deleted. Every number here must match the `.em` file. Left out,
//! because nothing in the seed genomes uses them: the `scent` field and its `smell` rule,
//! the generated reaction space (no mutation yet), and senses and actions the seeds do
//! not use.

use crate::chem::{Chemistry, U};
use crate::mind::{Curve, Gene, Genome, Input, Sense, Slot, ONE};
use crate::philox::{self, Stream};
use crate::world::{End, Law, Sink, Space, Upkeep, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lineage {
    /// Must discover body synthesis.
    Drifter,
    /// Positive control: body synthesis already works.
    Builder,
}

/// Substance and reaction indices in Dish's declaration order.
pub mod ids {
    pub const LIGHT: usize = 0;
    pub const ENERGY: usize = 1;
    pub const HEAT: usize = 2;
    pub const SUGAR: usize = 3;
    pub const CARBON: usize = 4;
    pub const NITRATE: usize = 5;
    pub const BODY: usize = 6;

    pub const PHOTOSYNTHESIS: usize = 0;
    pub const RESPIRATION: usize = 1;
    pub const BUILD: usize = 2;
    pub const ROT: usize = 3;
}

use ids::*;

pub fn chemistry() -> Chemistry {
    let mut c = Chemistry::new(&["C", "N", "E"]);
    c.substance("light", &[("E", 1)], 10);
    c.substance("energy", &[("E", 1)], 8);
    c.substance("heat", &[("E", 1)], 0);
    c.substance("sugar", &[("C", 1), ("E", 1)], 9);
    c.substance("carbon", &[("C", 1)], 0);
    c.substance("nitrate", &[("N", 1)], 0);
    c.substance("body", &[("C", 1), ("N", 1)], 5);
    c.reaction("photosynthesis", &[(LIGHT, 1), (CARBON, 1)], &[(SUGAR, 1)]).unwrap();
    c.reaction("respiration", &[(SUGAR, 1)], &[(ENERGY, 1), (CARBON, 1)]).unwrap();
    c.reaction("build", &[(ENERGY, 1), (CARBON, 1), (NITRATE, 1)], &[(HEAT, 1), (BODY, 1)]).unwrap();
    c.reaction("rot", &[(BODY, 1)], &[(CARBON, 1), (NITRATE, 1)]).unwrap();
    c
}

/// The seed genome of a lineage (spec §15).
pub fn seed_genome(lineage: Lineage) -> Genome {
    // Signals: food_x, food_y, steer, fullness, ready.
    let (food_x, food_y, steer, fullness, ready) = (0, 1, 2, 3, 4);
    let forage = vec![
        Gene::Sense { sense: Sense::Gradient(BODY), out: food_x, out2: Some(food_y) },
        Gene::Weigh { bias: 0.0, curve: Curve::Tanh, inputs: vec![(10.0, food_y)], out: steer },
        Gene::Move { speed: 1.0, fee_per_distance: 20, src: ONE, threshold: 0.0 },
        Gene::Turn { angle_deg: 30.0, fee: 10, src: Input::Signal(steer), threshold: 0.0 },
    ];
    let mut metabolism = vec![Gene::react(PHOTOSYNTHESIS, U), Gene::react(RESPIRATION, U)];
    if lineage == Lineage::Builder {
        metabolism.push(Gene::react(BUILD, 250));
    }
    metabolism.push(Gene::take(LIGHT, U, 5));
    metabolism.push(Gene::take(CARBON, U, 5));
    if lineage == Lineage::Builder {
        metabolism.push(Gene::take(NITRATE, 250, 5));
    }
    let reproduce = vec![
        Gene::sense(Sense::Inventory(BODY), fullness),
        Gene::Weigh { bias: -10.0, curve: Curve::Step, inputs: vec![(1.0, fullness)], out: ready },
        Gene::take(BODY, U, 5),
        Gene::Divide { split_bp: 5_000, requires: (BODY, 10 * U), fee: 0, src: Input::Signal(ready), threshold: 0.5 },
    ];
    let slots = [forage, metabolism, reproduce]
        .into_iter()
        .enumerate()
        .flat_map(|(m, genes)| genes.into_iter().map(move |gene| Slot { gene, expressed: true, module: m }))
        .collect();
    Genome { slots, modules: vec![true; 3], keep: vec![0.0; 5] }
}

/// Builds `Dish` with 2_000 seeds of one lineage, as `examples/dish.em` describes.
pub fn world(lineage: Lineage, seed: u64) -> World {
    let void = 0;
    let upkeep = Upkeep {
        pay: ENERGY, becomes: HEAT, vent: void,
        base: 20, per_gene: 5, per_signal: 1, per_signal_keep: 0, per_carried_u: 1,
    };
    let mut w = World::new(chemistry(), upkeep, vec![Sink { name: "void".into(), accepts: None }], void).unwrap();
    w.seed = seed;
    w.waste.push((HEAT, void));
    let (sun, _spark, rock) = (w.add_source("sun"), w.add_source("spark"), w.add_source("rock"));
    w.max_entities = 200_000;
    w.inventory_cap = 100 * U;
    w.max_age = 20_000;
    w.mind_steps = 1;

    let mut caps = vec![0; 7];
    for s in [LIGHT, SUGAR, CARBON, NITRATE, BODY] {
        caps[s] = 2_000 * U;
    }
    let space = Space { w: 512.0, h: 512.0, wrap: true, grid_w: 256, grid_h: 256, child_offset: 0.5 };
    w.grid(space, caps);

    // World rules, in declaration order.
    w.laws.push(Law::Inflow { s: LIGHT, source: sun, amount: 2 * U, overflow: Some(void) });
    w.laws.push(Law::Inflow { s: CARBON, source: rock, amount: 20, overflow: Some(void) });
    w.laws.push(Law::Inflow { s: NITRATE, source: rock, amount: 10, overflow: Some(void) });
    w.laws.push(Law::Inflow { s: BODY, source: rock, amount: 1, overflow: Some(void) });
    w.laws.push(Law::React { reaction: ROT, rate_bp: 50 });
    w.laws.push(Law::Diffuse { substances: vec![SUGAR, CARBON, NITRATE, BODY], rate_bp: 300 });

    // World genesis.
    for c in 0..w.cells.len() {
        w.endow(End::Cell(c), CARBON, 50 * U);
        w.endow(End::Cell(c), NITRATE, 20 * U);
        w.endow(End::Cell(c), BODY, U);
    }

    // Lineage genesis: 2_000 seeds, uniform positions and headings, 5u of energy each.
    let genome = seed_genome(lineage);
    for _ in 0..2_000 {
        let id = w.next_id;
        let draws = Stream { seed, tick: 0, stream: id as u32, domain: philox::DOMAIN_ENTITY, aux: 0, pass: philox::GENESIS };
        let unit = |i: u64| (draws.draw(i) >> 8) as f32 * (1.0 / 16_777_216.0);
        let pos = [unit(0) * 512.0, unit(1) * 512.0];
        let (s, c) = (unit(2) * std::f32::consts::TAU).sin_cos();
        let e = w.spawn_at(pos, [c, s], genome.clone());
        w.endow(End::Entity(e), ENERGY, 5 * U);
    }
    w
}

/// µ of body produced by entity reactions so far.
pub fn body_synthesis(w: &World) -> u64 {
    w.chem
        .reactions
        .iter()
        .enumerate()
        .map(|(r, rx)| rx.outputs.iter().filter(|o| o.0 == BODY).map(|o| o.1 as u64).sum::<u64>() * w.entity_extent[r])
        .sum()
}
