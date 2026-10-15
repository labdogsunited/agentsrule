//! Contact within the interaction radius: `give`, `steal`, and the `neighbour` and `crowd`
//! senses; also the `probe` and `clock` senses (spec §5.5, §6.2). Exact expected states.

mod common;

use common::dish;
use emerge_ref::compile::{compile, Options};
use emerge_ref::mutate::{mutate, Capacity, Draws, Rates};
use emerge_ref::philox::{self, Stream};
use emerge_ref::world::Space;
use emerge_ref::{End, Gene, Genome, Sense, World, ONE, U};

/// An 8 × 8 wrapping space with a 4 × 4 grid and an interaction radius of 2.
fn small(w: &mut World) {
    let caps = vec![2_000 * U; w.chem.substances.len()];
    w.grid(Space { w: 8.0, h: 8.0, wrap: true, grid_w: 4, grid_h: 4, child_offset: 0.5, radius: 2.0 }, caps);
}

fn steal(s: usize, max: u32) -> Gene {
    Gene::Steal { s, max, fee: 30, src: ONE, threshold: 0.0 }
}

fn sense(sense: Sense) -> Gene {
    Gene::Sense { sense, out: 0, out2: None }
}

#[test]
fn steal_takes_from_the_nearest_with_wrapping_and_ties_to_the_lower_id() {
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    let thief = w.spawn_at([1.0, 1.0], [1.0, 0.0], Genome::simple(vec![steal(d.energy, U)]));
    // Both are 1.5 away: one across the wrapped edge (ID 2), one directly (ID 3).
    let across = w.spawn_at([7.5, 1.0], [1.0, 0.0], Genome::simple(vec![]));
    let direct = w.spawn_at([2.5, 1.0], [1.0, 0.0], Genome::simple(vec![]));
    w.endow(End::Entity(thief), d.energy, 100);
    w.endow(End::Entity(across), d.energy, 5 * U);
    w.endow(End::Entity(direct), d.energy, 5 * U);
    w.step().unwrap();
    let energy: Vec<_> = w.entities.iter().map(|e| e.store.amounts[d.energy]).collect();
    // The thief paid its 30mu fee and took 1u from ID 2.
    assert_eq!(energy, [70 + U, 4 * U, 5 * U]);
    assert_eq!(w.stolen, U as u64);
}

#[test]
fn with_no_one_in_range_steal_and_give_are_dropped_and_pay_nothing() {
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    let give = Gene::Give { s: d.energy, max: U, fee: 5, src: ONE, threshold: 0.0 };
    let me = w.spawn_at([1.0, 1.0], [1.0, 0.0], Genome::simple(vec![steal(d.energy, U), give]));
    // 2.5 away on both axes, so about 3.5 away: out of range.
    let far = w.spawn_at([3.5, 3.5], [1.0, 0.0], Genome::simple(vec![]));
    w.endow(End::Entity(me), d.energy, 100);
    w.endow(End::Entity(far), d.energy, 5 * U);
    w.step().unwrap();
    assert_eq!(w.entities[0].store.amounts[d.energy], 100);
    assert_eq!(w.entities[1].store.amounts[d.energy], 5 * U);
}

#[test]
fn give_moves_to_the_nearest_and_respects_its_headroom() {
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    let give = Gene::Give { s: d.body, max: 3 * U, fee: 5, src: ONE, threshold: 0.0 };
    let giver = w.spawn_at([4.0, 4.0], [1.0, 0.0], Genome::simple(vec![give]));
    let near = w.spawn_at([5.0, 4.0], [1.0, 0.0], Genome::simple(vec![]));
    w.spawn_at([5.5, 4.0], [1.0, 0.0], Genome::simple(vec![]));
    w.endow(End::Entity(giver), d.energy, 100);
    w.endow(End::Entity(giver), d.body, 10 * U);
    // The receiver has room for only 2u more body.
    let cap = w.entities[near].store.caps[d.body];
    w.endow(End::Entity(near), d.body, cap - 2 * U);
    w.step().unwrap();
    let body: Vec<_> = w.entities.iter().map(|e| e.store.amounts[d.body]).collect();
    assert_eq!(body, [8 * U, cap, 0]);
    assert_eq!(w.entities[0].store.amounts[d.energy], 95);
}

#[test]
fn two_thieves_share_what_their_victim_has() {
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    let a = w.spawn_at([4.0, 3.0], [1.0, 0.0], Genome::simple(vec![steal(d.body, U)]));
    let victim = w.spawn_at([4.0, 4.0], [1.0, 0.0], Genome::simple(vec![]));
    let b = w.spawn_at([4.0, 5.0], [1.0, 0.0], Genome::simple(vec![steal(d.body, U)]));
    for e in [a, b] {
        w.endow(End::Entity(e), d.energy, 100);
    }
    w.endow(End::Entity(victim), d.body, U);
    w.step().unwrap();
    let body: Vec<_> = w.entities.iter().map(|e| e.store.amounts[d.body]).collect();
    assert_eq!(body, [U / 2, 0, U / 2]);
}

#[test]
fn a_dying_entity_cannot_be_stolen_from() {
    // Upkeep is 1mu per gene. The victim has a gene and no energy, so it is dying this tick
    // and takes part in no transfer. The thief still pays its attempt fee.
    let (mut w, d) = dish(0, 1);
    small(&mut w);
    let thief = w.spawn_at([4.0, 4.0], [1.0, 0.0], Genome::simple(vec![steal(d.body, U)]));
    let victim = w.spawn_at([5.0, 4.0], [1.0, 0.0], Genome::simple(vec![Gene::take(d.carbon, 0, 0)]));
    w.endow(End::Entity(thief), d.energy, 100);
    w.endow(End::Entity(victim), d.body, 5 * U);
    w.step().unwrap();
    assert_eq!(w.entities.len(), 1);
    assert_eq!(w.entities[0].store.amounts[d.body], 0);
    assert_eq!(w.entities[0].store.amounts[d.energy], 100 - 1 - 30);
    // Its body went to its cell.
    assert_eq!(w.cells[2 * 4 + 2].amounts[d.body], 5 * U);
}

#[test]
fn neighbour_reports_the_ranked_others_inventory_and_crowd_counts_them() {
    let reading = |s: Sense| {
        let (mut w, d) = dish(0, 0);
        small(&mut w);
        w.spawn_at([4.0, 4.0], [1.0, 0.0], Genome::simple(vec![sense(s)]));
        for (x, body) in [(5.5, 3), (4.5, 1), (2.5, 2), (7.0, 9)] {
            let e = w.spawn_at([x, 4.0], [1.0, 0.0], Genome::simple(vec![]));
            w.endow(End::Entity(e), d.body, body * U);
        }
        w.step().unwrap();
        w.entities[0].signals[0]
    };
    let body = dish(0, 0).1.body;
    // Readings are f32(amount) × 0.001, as a GPU computes them.
    let units = |mu: f32| mu * 0.001;
    // Nearest first: 0.5 away (1u), then two at 1.5 (2u at ID 4 loses to 3u at ID 2).
    assert_eq!(reading(Sense::Neighbour { s: body, rank: 0 }), units(1_000.0));
    assert_eq!(reading(Sense::Neighbour { s: body, rank: 1 }), units(3_000.0));
    assert_eq!(reading(Sense::Neighbour { s: body, rank: 2 }), units(2_000.0));
    // The fourth is 3.0 away, out of range.
    assert_eq!(reading(Sense::Neighbour { s: body, rank: 3 }), 0.0);
    assert_eq!(reading(Sense::Crowd { radius: 1.0 }), 1.0);
    // A radius beyond the interaction radius counts only as far as it.
    assert_eq!(reading(Sense::Crowd { radius: 6.0 }), 3.0);
}

#[test]
fn probe_reads_the_cell_ahead_and_clock_follows_the_tick() {
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    // Facing +y from cell (1, 1): 2.0 ahead, turned 90° left, is (3 - 2, 3), cell (0, 1).
    w.endow(End::Cell(4), d.body, 7 * U);
    let probe = sense(Sense::Probe { s: d.body, angle_deg: 90.0, dist: 2.0 });
    w.spawn_at([3.0, 3.0], [0.0, 1.0], Genome::simple(vec![probe]));
    w.step().unwrap();
    assert_eq!(w.entities[0].signals[0], 7_000.0 * 0.001);

    let (mut w, _) = dish(0, 0);
    small(&mut w);
    w.spawn_at([3.0, 3.0], [1.0, 0.0], Genome::simple(vec![sense(Sense::Clock { period: 4 })]));
    let mut seen = Vec::new();
    for _ in 0..4 {
        w.step().unwrap();
        seen.push(w.entities[0].signals[0].round());
    }
    assert_eq!(seen, [0.0, 1.0, 0.0, -1.0]);
}

const DISH: &str = include_str!("../../../examples/dish.em");

#[test]
fn contact_genes_compile_and_need_an_interaction_radius() {
    let src = DISH.replacen("action take(s: carbon, max: 1u) @eat_carbon;", "action steal(s: energy, max: 1u, from: neighbour) @eat_carbon;", 1);
    let opts = Options { seed: 1, allow_partial: true, mutation: false };
    let w = compile(&src, &["Drifter"], opts).unwrap_or_else(|e| panic!("{e}")).world;
    assert!(w.entities[0].genome.slots.iter().any(|s| matches!(s.gene, Gene::Steal { max: 1_000, fee: 30, .. })));
    let src = src.replace(", max_interaction_radius: 4.0", "");
    let e = compile(&src, &["Drifter"], opts).err().expect("steal needs a radius");
    assert_eq!(e.code, "E0116");
}

#[test]
fn mutation_can_insert_every_contact_gene() {
    let w = compile(DISH, &["Drifter"], Options { seed: 1, allow_partial: true, mutation: true }).unwrap().world;
    let only = Rates { insert_gene: 1.0, ..Rates::default() };
    let cap = Capacity { genes: 256, signals: 64, modules: 8 };
    let mut kinds = std::collections::BTreeSet::new();
    for n in 0..200 {
        let d = &mut Draws::new(Stream { seed: 3, tick: n, stream: n, domain: philox::DOMAIN_ENTITY, aux: 0, pass: philox::MUTATE });
        let child = mutate(&w.entities[0].genome, &only, &cap, &w.menus, d);
        for s in &child.slots {
            kinds.insert(match s.gene {
                Gene::Sense { sense: Sense::Probe { .. }, .. } => "probe",
                Gene::Sense { sense: Sense::Crowd { .. }, .. } => "crowd",
                Gene::Sense { sense: Sense::Neighbour { .. }, .. } => "neighbour",
                Gene::Sense { sense: Sense::Clock { .. }, .. } => "clock",
                Gene::Give { .. } => "give",
                Gene::Steal { .. } => "steal",
                _ => continue,
            });
        }
    }
    assert_eq!(kinds.into_iter().collect::<Vec<_>>(), ["clock", "crowd", "give", "neighbour", "probe", "steal"]);
}
