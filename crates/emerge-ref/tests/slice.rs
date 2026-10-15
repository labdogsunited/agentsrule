//! Exact expected states for the first executable slice of the four-phase schedule.

mod common;

use common::{dish, dish_cell};
use emerge_ref::chem::ChemError;
use emerge_ref::philox::{Stream, ARBITRATE_ORIGIN, DOMAIN_CELL};
use emerge_ref::{Cause, End, Event, Gene, Law, U};

// ---------- chemistry (spec §3.2, §4.4) ----------

#[test]
fn compiler_rejects_bad_reactions() {
    let (mut w, d) = dish(20, 0);
    let c = &mut w.chem;
    // heat -> energy is uphill: ΔG = +8.
    assert!(matches!(c.reaction("pump", &[(d.heat, 1)], &[(d.energy, 1)]), Err(ChemError::Uphill { delta_g: 8, .. })));
    // carbon + nitrate -> body without energy: ΔG = +5.
    assert!(matches!(
        c.reaction("free_body", &[(d.carbon, 1), (d.nitrate, 1)], &[(d.body, 1)]),
        Err(ChemError::Uphill { delta_g: 5, .. })
    ));
    // light -> body does not balance.
    assert!(matches!(c.reaction("bad", &[(d.light, 1)], &[(d.body, 1)]), Err(ChemError::Unbalanced { .. })));
    // 2 light -> 2 energy is not reduced.
    assert!(matches!(c.reaction("big", &[(d.light, 2)], &[(d.energy, 2)]), Err(ChemError::NotCanonical { .. })));
    // light -> energy is downhill and canonical: allowed.
    assert!(c.reaction("shortcut", &[(d.light, 1)], &[(d.energy, 1)]).is_ok());
}

// ---------- upkeep (spec §3.3, exit criterion 6) ----------

#[test]
fn starving_entity_dies_on_the_first_tick_it_cannot_pay() {
    // S = 1_010mu, upkeep u = 20mu: it pays on ticks 0..=49 and fails on tick 50,
    // which is floor(S/u) and within the spec's bound of ceil(S/u) + 1.
    let (mut w, d) = dish(20, 0);
    let cell = dish_cell(&mut w, &d);
    let e = w.spawn(cell, vec![]);
    w.endow(End::Entity(e), d.energy, 1_010);
    for _ in 0..50 {
        w.step().unwrap();
        assert!(w.events.is_empty());
    }
    assert_eq!(w.entities[0].store.amounts[d.energy], 10);
    w.step().unwrap();
    assert_eq!(w.events, vec![Event::Died { tick: 50, id: 1, cause: Cause::Starved }]);
    assert!(w.entities.is_empty());
    // The unpaid 10mu of energy cannot go to the cell (cap 0), so it goes to remains.
    assert_eq!(w.cells[cell].amounts[d.energy], 0);
}

#[test]
fn dying_entity_does_nothing_else() {
    // Not enough for upkeep: no take, no reaction, no division, even with stock to spare.
    let (mut w, d) = dish(20, 0);
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.carbon, 5 * U);
    let e = w.spawn(
        cell,
        vec![
            Gene::Take { s: d.carbon, max: U, fee: 0 },
            Gene::Divide { split_bp: 5_000, requires: (d.body, 0), fee: 0 },
        ],
    );
    w.endow(End::Entity(e), d.energy, 19);
    w.endow(End::Entity(e), d.body, 10 * U);
    w.step().unwrap();
    assert!(w.last_transfers.is_empty());
    assert_eq!(w.events, vec![Event::Died { tick: 0, id: 1, cause: Cause::Starved }]);
    // Its body goes back to its cell.
    assert_eq!(w.cells[cell].amounts[d.body], 10 * U);
    assert_eq!(w.cells[cell].amounts[d.carbon], 5 * U);
}

#[test]
fn fees_are_paid_in_genome_order_and_unaffordable_intents_are_dropped() {
    let (mut w, d) = dish(20, 0);
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.carbon, 10 * U);
    w.endow(End::Cell(cell), d.nitrate, 10 * U);
    let e = w.spawn(
        cell,
        vec![
            Gene::Take { s: d.carbon, max: U, fee: 5 },
            Gene::Take { s: d.nitrate, max: U, fee: 10 },
        ],
    );
    w.endow(End::Entity(e), d.energy, 30); // 20 upkeep, 5 for the first fee, 5 left
    w.step().unwrap();
    let inv = &w.entities[0].store.amounts;
    assert_eq!(inv[d.carbon], U);
    assert_eq!(inv[d.nitrate], 0);
    assert_eq!(inv[d.energy], 5);
}

// ---------- contention (spec §7.2) ----------

#[test]
fn takes_from_one_cell_are_shared_pro_rata_with_exact_remainders() {
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.carbon, 1_000);
    for _ in 0..3 {
        w.spawn(cell, vec![Gene::Take { s: d.carbon, max: 700, fee: 0 }]);
    }
    w.step().unwrap();
    let got: Vec<_> = w.entities.iter().map(|e| e.store.amounts[d.carbon]).collect();
    // 1_000 × 700/2_100 = 333.33 each. The fractions are equal, so the leftover µ goes to
    // the request with the smallest draw from the cell's stage-1 stream (spec §7.2).
    let draws = Stream { seed: 0, tick: 0, stream: cell as u32, domain: DOMAIN_CELL, aux: d.carbon as u32, pass: ARBITRATE_ORIGIN };
    let winner = (0..3).min_by_key(|&i| draws.draw(i)).unwrap() as usize;
    let mut expected = vec![333, 333, 333];
    expected[winner] = 334;
    assert_eq!(got, expected);
    assert_eq!(w.cells[cell].amounts[d.carbon], 0);
}

#[test]
fn remainder_ties_do_not_favour_low_ids() {
    let mut wins = [0; 3];
    for seed in 0..60 {
        let (mut w, d) = dish(0, 0);
        w.seed = seed;
        let cell = dish_cell(&mut w, &d);
        w.endow(End::Cell(cell), d.carbon, 1_000);
        for _ in 0..3 {
            w.spawn(cell, vec![Gene::Take { s: d.carbon, max: 700, fee: 0 }]);
        }
        w.step().unwrap();
        let i = w.entities.iter().position(|e| e.store.amounts[d.carbon] == 334).unwrap();
        wins[i] += 1;
    }
    assert!(wins.iter().all(|&n| n >= 10), "wins by position: {wins:?}");
}

#[test]
fn two_origins_into_a_nearly_full_cell_overflow_to_the_sink() {
    let (mut w, d) = dish(0, 0);
    let cell = w.add_cell({
        let mut caps = vec![0; w.chem.substances.len()];
        caps[d.carbon] = 1_000;
        caps
    });
    w.endow(End::Cell(cell), d.carbon, 900);
    let rock2 = w.add_source("rock2");
    w.laws.push(Law::Inflow { s: d.carbon, source: d.rock, amount: 100, overflow: Some(d.void) });
    w.laws.push(Law::Inflow { s: d.carbon, source: rock2, amount: 300, overflow: Some(d.void) });
    let emitted_before = w.emitted[w.chem.element("C")];
    w.step().unwrap();
    let t = &w.last_transfers;
    assert_eq!((t[0].delivered, t[0].overflowed), (25, 75));
    assert_eq!((t[1].delivered, t[1].overflowed), (75, 225));
    assert_eq!(w.cells[cell].amounts[d.carbon], 1_000);
    assert_eq!(w.emitted[w.chem.element("C")] - emitted_before, 400);
}

#[test]
fn without_an_overflow_sink_the_excess_stays_at_its_origin() {
    // An entity near its cap takes more than fits: the rest stays in the cell.
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.carbon, 5 * U);
    let e = w.spawn(cell, vec![Gene::Take { s: d.carbon, max: 2 * U, fee: 0 }]);
    w.endow(End::Entity(e), d.carbon, 99 * U);
    w.step().unwrap();
    assert_eq!(w.entities[0].store.amounts[d.carbon], 100 * U);
    assert_eq!(w.cells[cell].amounts[d.carbon], 4 * U);
}

#[test]
fn world_and_entity_transfers_share_one_arbitration() {
    // Spec §15.1: sunlight into a full cell overflows, and next tick refills exactly
    // what a seed took, because outflows do not create headroom in the same tick.
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.light, 2_000 * U);
    w.laws.push(Law::Inflow { s: d.light, source: d.sun, amount: 2 * U, overflow: Some(d.void) });
    w.spawn(cell, vec![Gene::Take { s: d.light, max: U, fee: 0 }]);
    w.step().unwrap();
    assert_eq!(w.cells[cell].amounts[d.light], 1_999 * U);
    let sun = &w.last_transfers[1];
    assert_eq!((sun.delivered, sun.overflowed), (0, 2 * U));
    w.step().unwrap();
    assert_eq!(w.cells[cell].amounts[d.light], 1_999 * U);
    let sun = &w.last_transfers[1];
    assert_eq!((sun.delivered, sun.overflowed), (U, U));
}

#[test]
fn a_seed_shares_a_cell_with_diffusion() {
    // Spec §15.1: a 200mu cell, diffusion at 300bp to 4 neighbours, a seed taking 1u.
    // Diffusion offers floor(200 × 3%) = 6mu, 1mu to each neighbour (2mu stay).
    // Requests total 1_004mu > 200mu, so the seed's share is 200 × 1_000/1_004 = 199.20mu
    // and each neighbour's 0.199mu. Floors give 199mu and 0mu; the leftover µ goes to the
    // largest fraction, the seed's. So here the seed still takes the whole cell.
    let (mut w, d) = dish(0, 0);
    let centre = dish_cell(&mut w, &d);
    for _ in 0..4 {
        let nb = dish_cell(&mut w, &d);
        w.link(centre, nb);
    }
    w.endow(End::Cell(centre), d.body, 200);
    w.laws.push(Law::Diffuse { substances: vec![d.body], rate_bp: 300 });
    w.spawn(centre, vec![Gene::Take { s: d.body, max: U, fee: 0 }]);
    w.step().unwrap();
    assert_eq!(w.entities[0].store.amounts[d.body], 200);
    let to_neighbours: u32 = (1..5).map(|c| w.cells[c].amounts[d.body]).sum();
    assert_eq!(to_neighbours, 0);
    assert_eq!(w.cells[centre].amounts[d.body], 0);
}

// ---------- reaction order (spec §7.3) ----------

#[test]
fn reactions_chain_in_genome_order_within_a_tick() {
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    let e = w.spawn(
        cell,
        vec![
            Gene::React { reaction: d.photosynthesis, rate: U },
            Gene::React { reaction: d.respiration, rate: U },
        ],
    );
    w.endow(End::Entity(e), d.light, U);
    w.endow(End::Entity(e), d.carbon, U);
    w.step().unwrap();
    let inv = &w.entities[0].store.amounts;
    assert_eq!((inv[d.energy], inv[d.sugar], inv[d.carbon], inv[d.light]), (U, 0, U, 0));
}

#[test]
fn reversed_order_delays_the_chain_by_a_tick() {
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    let e = w.spawn(
        cell,
        vec![
            Gene::React { reaction: d.respiration, rate: U },
            Gene::React { reaction: d.photosynthesis, rate: U },
        ],
    );
    w.endow(End::Entity(e), d.light, U);
    w.endow(End::Entity(e), d.carbon, U);
    w.step().unwrap();
    let inv = &w.entities[0].store.amounts;
    assert_eq!((inv[d.energy], inv[d.sugar]), (0, U));
    w.step().unwrap();
    let inv = &w.entities[0].store.amounts;
    assert_eq!((inv[d.energy], inv[d.sugar]), (U, 0));
}

#[test]
fn world_reactions_run_after_transfers() {
    // Spec §15.1: detritus rain of 1mu per tick and rot at 50bp. Rot runs on the balance
    // after the rain, floored to whole µ: it removes 1mu once the cell reaches 200mu.
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    w.laws.push(Law::Inflow { s: d.body, source: d.rock, amount: 1, overflow: Some(d.void) });
    w.laws.push(Law::React { reaction: d.rot, rate_bp: 50 });
    let mut ends = Vec::new();
    for _ in 0..400 {
        w.step().unwrap();
        ends.push(w.cells[cell].amounts[d.body]);
    }
    assert_eq!(ends[198], 199);
    // From then on the cell ends every tick at exactly 199mu (it holds 200mu only
    // between Move and Transform).
    assert!(ends[198..].iter().all(|&b| b == 199));
}

// ---------- division and disposal (spec §7.5) ----------

#[test]
fn division_checks_the_inventory_after_transform() {
    // The seed starts with 9.5u body, below the 10u requirement. It takes 1u in Move,
    // so after Transform it has 10.5u and divides in the same tick.
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.body, 100 * U);
    let genes = vec![
        Gene::Take { s: d.body, max: U, fee: 0 },
        Gene::Divide { split_bp: 5_000, requires: (d.body, 10 * U), fee: 0 },
    ];
    let e = w.spawn(cell, genes);
    w.endow(End::Entity(e), d.body, 9_500);
    w.endow(End::Entity(e), d.energy, 3);
    w.step().unwrap();
    assert_eq!(w.events, vec![Event::Born { tick: 0, parent: 1, child: 2 }]);
    let amounts: Vec<_> = w.entities.iter().map(|e| (e.id, e.store.amounts[d.body], e.store.amounts[d.energy])).collect();
    // floor(10_500 × 50%) = 5_250 each; floor(3 × 50%) = 1 to the child, the remainder stays.
    assert_eq!(amounts, vec![(1, 5_250, 2), (2, 5_250, 1)]);
    assert_eq!(w.cells[cell].amounts[d.body], 99 * U);
}

#[test]
fn newborns_first_act_in_the_next_tick() {
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.body, 100 * U);
    let e = w.spawn(
        cell,
        vec![
            Gene::Take { s: d.body, max: U, fee: 0 },
            Gene::Divide { split_bp: 5_000, requires: (d.body, 20 * U), fee: 0 },
        ],
    );
    w.endow(End::Entity(e), d.body, 19 * U);
    w.step().unwrap();
    // The child got 10u and took nothing in its birth tick; the parent kept 10u.
    assert_eq!(w.entities[1].store.amounts[d.body], 10 * U);
    assert_eq!(w.entities[1].age, 0);
    w.step().unwrap();
    // Next tick both take 1u, and neither reaches 20u.
    let bodies: Vec<_> = w.entities.iter().map(|e| e.store.amounts[d.body]).collect();
    assert_eq!(bodies, vec![11 * U, 11 * U]);
    assert_eq!(w.cells[cell].amounts[d.body], 97 * U);
}

#[test]
fn division_is_blocked_at_max_entities_and_by_an_unpaid_fee() {
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    let divide = |fee| vec![Gene::Divide { split_bp: 5_000, requires: (d.body, U), fee }];
    let a = w.spawn(cell, divide(0));
    let b = w.spawn(cell, divide(10));
    w.endow(End::Entity(a), d.body, 2 * U);
    w.endow(End::Entity(b), d.body, 2 * U);
    w.endow(End::Entity(b), d.energy, 9); // cannot afford the 10mu divide fee
    w.max_entities = 2;
    w.step().unwrap();
    assert!(w.events.is_empty());
    w.max_entities = 3;
    w.step().unwrap();
    // Only `a` divides: `b` still cannot pay its fee.
    assert_eq!(w.events, vec![Event::Born { tick: 1, parent: 1, child: 3 }]);
}

#[test]
fn disposal_comes_before_division_and_fills_the_cell_first() {
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.body, 1_999 * U);
    let e = w.spawn(cell, vec![Gene::Divide { split_bp: 5_000, requires: (d.body, U), fee: 0 }]);
    w.endow(End::Entity(e), d.body, 3 * U);
    w.endow(End::Entity(e), d.energy, 2 * U);
    w.max_age = 1;
    w.step().unwrap();
    // It reached max_age: it is disposed of and does not divide.
    assert_eq!(w.events, vec![Event::Died { tick: 0, id: 1, cause: Cause::Age }]);
    // 1u of body fits in the cell; 2u of body and all the energy go to remains.
    assert_eq!(w.cells[cell].amounts[d.body], 2_000 * U);
    w.audit().unwrap();
}
