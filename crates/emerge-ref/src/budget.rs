//! Static budget check (`proposals/budget-check.md`). Only what holds without dynamics:
//! per-unit costs of substances collapse once light can be turned into the `pay` substance,
//! because supply, which the check cannot see, is what limits them.

use crate::chem::Mu;
use crate::world::World;

/// If `steal` can take the `pay` substance and one `steal` gene at its menu maximum gains
/// more of it than its fee plus its gene upkeep, returns `(gain, spend)` in µ per tick. Such
/// a gene is net income whenever a neighbour holds `pay`.
pub fn steal_pays(w: &World) -> Option<(Mu, Mu)> {
    let a = w.menus.steal.as_ref()?;
    if !a.substances.contains(&w.upkeep.pay) {
        return None;
    }
    let (gain, spend) = (a.range.1 as Mu, a.fee + w.upkeep.per_gene);
    (gain > spend).then_some((gain, spend))
}
