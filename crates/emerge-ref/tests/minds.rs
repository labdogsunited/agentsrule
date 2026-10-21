//! Minds: senses, regulation genes, mind steps, and signals driving actions and reactions
//! (spec §5, §7.1). Exact expected states.

mod common;

use common::{dish, dish_cell};
use emerge_ref::{Curve, End, Event, Gene, Genome, Input, Sense, Slot, U};

fn expressed(gene: Gene) -> Slot {
    Slot { gene, expressed: true, module: 0 }
}

/// Dish's `reproduce` module: sense body -> fullness; weigh(bias -10, step) -> ready;
/// take body; divide when ready > 0.5.
fn reproduce(body: usize) -> Vec<Gene> {
    vec![
        Gene::sense(Sense::Inventory(body), 0),
        Gene::Weigh { bias: -10.0, curve: Curve::Step, inputs: vec![(1.0, 0)], out: 1 },
        Gene::take(body, U, 0),
        Gene::Divide { split_bp: 5_000, requires: (body, 10 * U), fee: 0, src: Input::Signal(1), threshold: 0.5 },
    ]
}

#[test]
fn each_regulation_gene_adds_one_mind_step_of_delay() {
    // Body in the snapshot: 9.5u at tick 0, 10.5u at tick 1, 11.5u at tick 2.
    // With one mind step, `ready` at tick t reads `fullness` from tick t − 1, so the gate
    // opens at tick 2, although `requires body >= 10u` already holds after tick 0.
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.body, 100 * U);
    let e = w.spawn(cell, reproduce(d.body));
    w.endow(End::Entity(e), d.body, 9_500);
    w.step().unwrap();
    w.step().unwrap();
    assert!(w.events.is_empty());
    // Readings are amount × 0.001 in f32, so 10.5u reads as 10.500001 (spec §3.5).
    assert_eq!(w.entities[0].signals, vec![10_500.0 * 0.001, 0.0]);
    w.step().unwrap();
    assert_eq!(w.events, vec![Event::Born { tick: 2, parent: 1, child: 2 }]);
}

#[test]
fn more_mind_steps_shorten_the_delay() {
    // With two mind steps, `ready` sees this tick's `fullness` in the second step.
    let (mut w, d) = dish(0, 0);
    w.mind_steps = 2;
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.body, 100 * U);
    let e = w.spawn(cell, reproduce(d.body));
    w.endow(End::Entity(e), d.body, 9_500);
    w.step().unwrap();
    assert!(w.events.is_empty());
    w.step().unwrap();
    assert_eq!(w.events, vec![Event::Born { tick: 1, parent: 1, child: 2 }]);
}

#[test]
fn keep_sets_how_much_of_a_signal_survives_each_step() {
    // s ← 0.5 · s + 2.0 each tick: 2.0, 3.0, 3.5, 3.75.
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    let genome = Genome {
        slots: vec![expressed(Gene::sense(Sense::Inventory(d.carbon), 0))],
        modules: vec![true],
        keep: vec![0.5],
    };
    let e = w.spawn_genome(cell, genome);
    w.endow(End::Entity(e), d.carbon, 2 * U);
    let mut seen = Vec::new();
    for _ in 0..4 {
        w.step().unwrap();
        seen.push(w.entities[0].signals[0]);
    }
    assert_eq!(seen, vec![2.0, 3.0, 3.5, 3.75]);
}

#[test]
fn a_signal_sets_reaction_speed() {
    // weigh(bias 0.25, linear) with no inputs writes 0.25: photosynthesis runs at
    // floor(1u × 0.25) = 250mu of extent.
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    let e = w.spawn(
        cell,
        vec![
            Gene::Weigh { bias: 0.25, curve: Curve::Linear, inputs: vec![], out: 0 },
            Gene::React { reaction: d.photosynthesis, rate: U, speed: Input::Signal(0) },
        ],
    );
    w.endow(End::Entity(e), d.light, U);
    w.endow(End::Entity(e), d.carbon, U);
    w.step().unwrap();
    assert_eq!(w.entities[0].store.amounts[d.sugar], 250);
}

#[test]
fn a_signal_sizes_an_action_and_must_exceed_its_threshold() {
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.carbon, 10 * U);
    w.endow(End::Cell(cell), d.nitrate, 10 * U);
    w.spawn(
        cell,
        vec![
            Gene::Weigh { bias: 0.5, curve: Curve::Linear, inputs: vec![], out: 0 },
            // Fires at 0.5 > 0.0 and takes floor(0.5 × 1u) = 500mu.
            Gene::Take { s: d.carbon, max: U, fee: 0, src: Input::Signal(0), threshold: 0.0 },
            // 0.5 is not above 0.5: does not fire.
            Gene::Take { s: d.nitrate, max: U, fee: 0, src: Input::Signal(0), threshold: 0.5 },
        ],
    );
    w.step().unwrap();
    let inv = &w.entities[0].store.amounts;
    assert_eq!((inv[d.carbon], inv[d.nitrate]), (500, 0));
}

#[test]
fn only_the_first_firing_divide_gene_counts_and_later_ones_pay_nothing() {
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    let off = Input::Const(0.0);
    let e = w.spawn(
        cell,
        vec![
            // Does not fire, so it is not the first firing gene.
            Gene::Divide { split_bp: 5_000, requires: (d.body, U), fee: 3, src: off, threshold: 0.0 },
            Gene::divide(2_500, (d.body, U), 7),
            Gene::divide(5_000, (d.body, U), 11),
        ],
    );
    w.endow(End::Entity(e), d.body, 4 * U);
    w.endow(End::Entity(e), d.energy, 100);
    w.step().unwrap();
    // Only the second gene acts: a 25% split and a 7mu fee.
    let bodies: Vec<_> = w.entities.iter().map(|e| e.store.amounts[d.body]).collect();
    assert_eq!(bodies, vec![3 * U, U]);
    assert_eq!(w.entities[0].store.amounts[d.energy] + w.entities[1].store.amounts[d.energy], 93);
}

#[test]
fn an_action_without_a_target_is_dropped_and_pays_no_fee() {
    // Dish cells cannot hold energy, so take(energy) has no target.
    let (mut w, d) = dish(0, 0);
    let cell = dish_cell(&mut w, &d);
    let e = w.spawn(cell, vec![Gene::take(d.energy, U, 5)]);
    w.endow(End::Entity(e), d.energy, 100);
    w.step().unwrap();
    assert!(w.last_transfers.is_empty());
    assert_eq!(w.entities[0].store.amounts[d.energy], 100);
}

#[test]
fn upkeep_counts_active_genes_signals_in_use_retention_and_carried_units() {
    let (mut w, d) = dish(20, 5);
    w.upkeep.per_signal = 1;
    w.upkeep.per_signal_keep = 10;
    w.upkeep.per_carried_u = 1;
    let cell = dish_cell(&mut w, &d);
    let genome = Genome {
        slots: vec![
            // Writes s0, which nothing reads: s0 is not in use.
            expressed(Gene::sense(Sense::Cell(d.light), 0)),
            expressed(Gene::sense(Sense::Inventory(d.body), 1)),
            expressed(Gene::Weigh { bias: 0.0, curve: Curve::Tanh, inputs: vec![(1.0, 1)], out: 2 }),
            expressed(Gene::Take { s: d.body, max: U, fee: 0, src: Input::Signal(2), threshold: 0.0 }),
            // Dormant gene: free.
            Slot { gene: Gene::react(d.respiration, U), expressed: false, module: 0 },
            // Expressed gene in a dormant module: free.
            Slot { gene: Gene::react(d.photosynthesis, U), expressed: true, module: 1 },
        ],
        modules: vec![true, false],
        keep: vec![0.0, 0.25, 0.0],
    };
    let e = w.spawn_genome(cell, genome);
    w.endow(End::Entity(e), d.energy, 5 * U);
    w.endow(End::Entity(e), d.body, 500);
    w.step().unwrap();
    // base 20 + 4 active genes × 5 + 2 signals in use (s1, s2) × 1
    // + ceil(10 × 0.25) = 3 for s1's retention + floor(5.5u carried) = 5. Total 50mu.
    assert_eq!(w.entities[0].store.amounts[d.energy], 5 * U - 50);
}

#[test]
fn delta_reads_the_signed_change_since_the_last_reading_in_both_modes() {
    // Amounts at each tick's snapshot: 5u, 7u, 3u, 3u, then the full `u32` range down and up.
    // The first reading is 0; a decrease reads negative (it used to wrap to a huge positive).
    let amounts: [u32; 6] = [5_000, 7_000, 3_000, 3_000, 0, u32::MAX];
    let read = |contract: bool| {
        let (mut w, d) = dish(0, 0);
        w.contract = contract;
        let cell = dish_cell(&mut w, &d);
        w.spawn(cell, vec![Gene::sense(Sense::Delta(d.body), 0)]);
        amounts
            .iter()
            .map(|&a| {
                w.entities[0].store.amounts[d.body] = a;
                w.run_tick(); // the amounts are set by hand, so the ledger would not close
                w.entities[0].signals[0]
            })
            .collect::<Vec<f32>>()
    };
    let default = read(false);
    assert_eq!(default[..4], [0.0, 2_000.0 * 0.001, -(4_000.0 * 0.001), 0.0]);
    assert_eq!(default[4], -(3_000.0 * 0.001));
    // The largest rise is cut at the signal limit (1e4).
    assert_eq!(default[5], 1e4);
    assert_eq!(default, read(true));
}
