//! `--accounts` closes the budget: what a lineage's entities hold of `pay` and `flux` changes
//! by exactly the columns of `ACCOUNT_COLUMNS` (see the note above them in `world.rs`).

mod common;

use common::dish;
use emerge_ref::compile::{compile, Options};
use emerge_ref::world::{Accounts, Space, ACCOUNT_COLUMNS};
use emerge_ref::{End, Gene, Genome, World, ONE, U};

/// What the entities of lineage `l` hold of `s`.
fn held(w: &World, l: usize, s: usize) -> i64 {
    w.entities.iter().filter(|e| e.lineage == l).map(|e| e.store.amounts[s] as i64).sum()
}

/// The sum of a column over the groups of lineage `l` (all lineages if `None`). With `flux`
/// set, group `2 l` is the lineage's non-carriers and `2 l + 1` its carriers.
fn total(w: &World, column: &str, l: Option<usize>) -> i64 {
    let k = ACCOUNT_COLUMNS.iter().position(|c| *c == column).unwrap();
    let totals = &w.accounts.as_ref().unwrap().totals;
    totals.iter().enumerate().filter(|(g, _)| l.is_none_or(|l| g / 2 == l)).map(|(_, t)| t[k] as i64).sum()
}

/// Expected change in what the entities of `l` hold of `kind` (`pay` or `flux`).
fn change(w: &World, kind: &str, l: Option<usize>) -> i64 {
    let t = |c: &str| total(w, &format!("{kind}_{c}_mu"), l);
    let mut x = t("made") - t("used") + t("taken") + t("stolen_in") - t("stolen_out") + t("gift_in") - t("gift_out") - t("dropped")
        - t("wasted") - t("died_with") + t("from_parent") - t("to_child");
    if kind == "pay" {
        x -= total(w, "upkeep_mu", l) + total(w, "fees_mu", l);
    }
    x
}

#[test]
fn gifts_drops_deaths_and_divisions_are_counted_and_close_the_budget() {
    let (mut w, d) = dish(5, 0);
    w.grid(Space { w: 8.0, h: 8.0, wrap: true, grid_w: 4, grid_h: 4, child_offset: 0.5, radius: 2.0 }, vec![2_000 * U; w.chem.substances.len()]);
    let give = |s| Gene::Give { s, max: 3 * U, fee: 5, src: ONE, threshold: 0.0 };
    let giver = w.spawn_at([4.0, 4.0], [1.0, 0.0], Genome::simple(vec![give(d.body), give(d.energy)]));
    let near = w.spawn_at([5.0, 4.0], [1.0, 0.0], Genome::simple(vec![]));
    let dropper = w.spawn_at([1.0, 1.0], [1.0, 0.0], Genome::simple(vec![Gene::drop(d.body, 2 * U, 5)]));
    let divider = w.spawn_at([1.0, 6.0], [1.0, 0.0], Genome::simple(vec![Gene::divide(5_000, (d.energy, U), 0)]));
    // No energy: it cannot pay its 5mu upkeep, so it dies holding body.
    let dying = w.spawn_at([6.0, 1.0], [1.0, 0.0], Genome::simple(vec![]));
    for (e, energy, body) in [(giver, 100 * U, 10 * U), (near, 50 * U, 0), (dropper, 100 * U, 5 * U), (divider, 10 * U, 4 * U), (dying, 0, 3 * U)] {
        w.endow(End::Entity(e), d.energy, energy);
        w.endow(End::Entity(e), d.body, body);
    }
    w.accounts = Some(Accounts { flux: Some(d.body), ..Default::default() });
    let before = [held(&w, 0, d.energy), held(&w, 0, d.body)];
    w.step().unwrap();
    let col = |c: &str| total(&w, c, None);
    assert_eq!((col("pay_gift_out_mu"), col("pay_gift_in_mu")), (3 * U as i64, 3 * U as i64));
    assert_eq!((col("flux_gift_out_mu"), col("flux_gift_in_mu")), (3 * U as i64, 3 * U as i64));
    assert_eq!(col("flux_dropped_mu"), 2 * U as i64);
    assert_eq!(col("flux_died_with_mu"), 3 * U as i64);
    // The divider holds 10u less its 5mu upkeep, and hands over half.
    assert_eq!((col("pay_to_child_mu"), col("pay_from_parent_mu")), (4_997, 4_997));
    assert_eq!(col("flux_to_child_mu"), 2 * U as i64);
    assert_eq!(col("births"), 1);
    assert_eq!(held(&w, 0, d.energy) - before[0], change(&w, "pay", None));
    assert_eq!(held(&w, 0, d.body) - before[1], change(&w, "flux", None));
}

#[test]
fn the_budget_closes_for_each_lineage_of_the_dish_over_many_ticks() {
    let src = include_str!("../../../examples/dish.em");
    let mut w = compile(src, &["Drifter", "Builder"], Options { seed: 7, allow_partial: true, mutation: true }).unwrap_or_else(|e| panic!("{e:?}")).world;
    let (pay, flux) = (w.upkeep.pay, w.chem.substances.iter().position(|s| s.name == "body").unwrap());
    w.accounts = Some(Accounts { flux: Some(flux), ..Default::default() });
    let before: Vec<[i64; 2]> = (0..2).map(|l| [held(&w, l, pay), held(&w, l, flux)]).collect();
    for _ in 0..120 {
        w.run_tick();
    }
    assert!(w.deaths > 0 && w.births > 0, "the run should have births and deaths");
    for l in 0..2 {
        assert_eq!(held(&w, l, pay) - before[l][0], change(&w, "pay", Some(l)), "pay, lineage {l}");
        assert_eq!(held(&w, l, flux) - before[l][1], change(&w, "flux", Some(l)), "flux, lineage {l}");
    }
}
