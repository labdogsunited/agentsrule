//! Per-group accounting (`emerge run --accounts`): exact totals for one entity and one tick.

mod common;

use common::{dish, dish_cell};
use emerge_ref::world::{Accounts, ACCOUNT_COLUMNS};
use emerge_ref::{End, Gene, U};

#[test]
fn one_tick_of_upkeep_fees_uptake_and_a_reaction_is_counted_in_the_carrier_group() {
    // Upkeep 1mu per gene. One `take(carbon)` for 5mu, and respiration (sugar -> energy + carbon)
    // at 1u. With `flux` carbon the entity carries a carbon-making reaction: group 2 × 0 + 1.
    let (mut w, d) = dish(0, 1);
    let cell = dish_cell(&mut w, &d);
    w.endow(End::Cell(cell), d.carbon, 100 * U);
    let e = w.spawn(cell, vec![Gene::take(d.carbon, 2 * U, 5), Gene::react(d.respiration, U)]);
    w.endow(End::Entity(e), d.energy, 100);
    w.endow(End::Entity(e), d.sugar, U);
    w.accounts = Some(Accounts { flux: Some(d.carbon), ..Default::default() });
    w.step().unwrap();
    let a = w.accounts.as_ref().unwrap();
    let got = |k: &str| a.totals[1][ACCOUNT_COLUMNS.iter().position(|c| *c == k).unwrap()];
    assert_eq!((got("upkeep_mu"), got("fees_mu")), (2, 5));
    assert_eq!((got("pay_made_mu"), got("pay_used_mu")), (U as u64, 0));
    assert_eq!((got("flux_taken_mu"), got("flux_made_mu")), (2 * U as u64, U as u64));
    assert_eq!(a.totals[0], [0; ACCOUNT_COLUMNS.len()]);
}
