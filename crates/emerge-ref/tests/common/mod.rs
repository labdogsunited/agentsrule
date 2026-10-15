//! The chemistry of `examples/dish.em`, built by hand until there is a parser.

#![allow(dead_code)]

use emerge_ref::world::{SinkId, SourceId};
use emerge_ref::{Chemistry, Mu, Sink, Upkeep, World, U};

pub struct Dish {
    pub light: usize,
    pub energy: usize,
    pub heat: usize,
    pub sugar: usize,
    pub carbon: usize,
    pub nitrate: usize,
    pub body: usize,
    pub photosynthesis: usize,
    pub respiration: usize,
    pub build: usize,
    pub rot: usize,
    pub sun: SourceId,
    pub spark: SourceId,
    pub rock: SourceId,
    pub void: SinkId,
}

/// A world with Dish's chemistry and payment route, and no cells or laws.
/// Upkeep is `base` plus `per_gene` per gene, paid in energy and vented as heat.
pub fn dish(base: Mu, per_gene: Mu) -> (World, Dish) {
    let mut c = Chemistry::new(&["C", "N", "E"]);
    let light = c.substance("light", &[("E", 1)], 10);
    let energy = c.substance("energy", &[("E", 1)], 8);
    let heat = c.substance("heat", &[("E", 1)], 0);
    let sugar = c.substance("sugar", &[("C", 1), ("E", 1)], 9);
    let carbon = c.substance("carbon", &[("C", 1)], 0);
    let nitrate = c.substance("nitrate", &[("N", 1)], 0);
    let body = c.substance("body", &[("C", 1), ("N", 1)], 5);
    let photosynthesis = c.reaction("photosynthesis", &[(light, 1), (carbon, 1)], &[(sugar, 1)]).unwrap();
    let respiration = c.reaction("respiration", &[(sugar, 1)], &[(energy, 1), (carbon, 1)]).unwrap();
    let build = c
        .reaction("build", &[(energy, 1), (carbon, 1), (nitrate, 1)], &[(heat, 1), (body, 1)])
        .unwrap();
    let rot = c.reaction("rot", &[(body, 1)], &[(carbon, 1), (nitrate, 1)]).unwrap();
    let void = 0;
    let upkeep = Upkeep { pay: energy, becomes: heat, vent: void, base, per_gene };
    let mut w = World::new(c, upkeep, vec![Sink { name: "void".into(), accepts: None }], void).unwrap();
    w.waste.push((heat, void));
    let sun = w.add_source("sun");
    let spark = w.add_source("spark");
    let rock = w.add_source("rock");
    let d = Dish {
        light, energy, heat, sugar, carbon, nitrate, body,
        photosynthesis, respiration, build, rot,
        sun, spark, rock, void,
    };
    (w, d)
}

/// Cell caps as in Dish: 2_000u of light, sugar, carbon, nitrate and body; no energy or heat.
pub fn dish_cell(w: &mut World, d: &Dish) -> usize {
    let mut caps = vec![0; w.chem.substances.len()];
    for s in [d.light, d.sugar, d.carbon, d.nitrate, d.body] {
        caps[s] = 2_000 * U;
    }
    w.add_cell(caps)
}
