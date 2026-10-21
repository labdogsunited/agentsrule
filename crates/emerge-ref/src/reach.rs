//! Which substances can ever appear in a compiled world, by an optimistic closure
//! (`proposals/mathematical-tools-for-ai.md` §3).
//!
//! Start with every substance some store holds and every substance an inflow rule adds.
//! Then, until nothing changes, add the outputs of every reaction whose inputs are all
//! present: the reaction space (anything mutation may give a gene), reaction rules, the
//! world's `decompose` reaction, and the upkeep route (`pay` becomes `becomes`). Amounts,
//! order, costs, caps and competition are ignored, so the closure only over-estimates.
//! A substance it never adds can never appear, and every reaction that makes one needs
//! another such substance: the set of them is an empty siphon.
//!
//! Not covered: lineages that are not in the world (for example `--inject`), and hot-swaps.

use crate::chem::{ReactionId, SubstanceId};
use crate::world::{Law, World};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reach {
    /// By substance: whether it can ever appear.
    pub possible: Vec<bool>,
    /// Reactions in scope, in reaction-ID order, that can never run.
    pub dead: Vec<ReactionId>,
    /// For each reaction in scope that makes an impossible substance: an impossible input
    /// it needs. Checkable against the reaction list without rerunning the closure.
    pub witness: Vec<(ReactionId, SubstanceId)>,
}

/// The reactions the closure uses: the reaction space, reaction rules and `decompose`.
fn in_scope(w: &World) -> Vec<ReactionId> {
    let mut r: Vec<ReactionId> = w.menus.reactions.iter().map(|&(r, _)| r).collect();
    r.extend(w.laws.iter().filter_map(|l| match l {
        Law::React { reaction, .. } => Some(*reaction),
        _ => None,
    }));
    r.extend(w.decompose);
    r.sort_unstable();
    r.dedup();
    r
}

pub fn reach(w: &World) -> Reach {
    let n = w.chem.substances.len();
    let mut possible = vec![false; n];
    for store in w.cells.iter().chain(w.entities.iter().map(|e| &e.store)) {
        for (s, p) in possible.iter_mut().enumerate() {
            *p |= store.amounts[s] > 0;
        }
    }
    for l in &w.laws {
        if let Law::Inflow { s, .. } = l {
            possible[*s] = true;
        }
    }
    let scope = in_scope(w);
    let fires = |possible: &[bool], r: ReactionId| w.chem.reactions[r].inputs.iter().all(|&(s, _)| possible[s]);
    loop {
        let mut changed = false;
        if w.creatures && possible[w.upkeep.pay] && !possible[w.upkeep.becomes] {
            possible[w.upkeep.becomes] = true;
            changed = true;
        }
        for &r in &scope {
            if fires(&possible, r) {
                for &(s, _) in &w.chem.reactions[r].outputs {
                    changed |= !possible[s];
                    possible[s] = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let dead = scope.iter().copied().filter(|&r| !fires(&possible, r)).collect();
    let witness = scope
        .iter()
        .filter(|&&r| w.chem.reactions[r].outputs.iter().any(|&(s, _)| !possible[s]))
        .map(|&r| (r, w.chem.reactions[r].inputs.iter().find(|&&(s, _)| !possible[s]).expect("closure is complete").0))
        .collect();
    Reach { possible, dead, witness }
}
