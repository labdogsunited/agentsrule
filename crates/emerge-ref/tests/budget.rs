//! The static budget check: `steal` of the `pay` substance as net income.

mod common;

use common::dish;
use emerge_ref::budget::steal_pays;
use emerge_ref::mutate::ActionMenu;

#[test]
fn steal_of_pay_is_net_income_only_when_it_gains_more_than_fee_plus_upkeep() {
    let (mut w, d) = dish(0, 5);
    let steal = |max: f32, fee, substances| Some(ActionMenu { range: (0.0, max), fee, substances });
    assert_eq!(steal_pays(&w), None);
    // 1u for 200mu + 5mu: net income.
    w.menus.steal = steal(1_000.0, 200, vec![d.energy, d.body]);
    assert_eq!(steal_pays(&w), Some((1_000, 205)));
    // 200mu for 200mu + 5mu: a loss.
    w.menus.steal = steal(200.0, 200, vec![d.energy]);
    assert_eq!(steal_pays(&w), None);
    // `steal` cannot name energy.
    w.menus.steal = steal(10_000.0, 30, vec![d.light, d.sugar]);
    assert_eq!(steal_pays(&w), None);
}
