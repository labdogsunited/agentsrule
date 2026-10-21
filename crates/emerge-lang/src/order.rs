//! `emerge order`: the steps of one tick for a world, with each world rule placed in the
//! phase its effect gives it (spec §7, §7.4).
//!
//! The report is formatted from the checked schedule (`schedule::lower`), the same one the
//! runner takes its rule placements and disposal settings from. The steps are written down once,
//! in `schedule::STEPS`; a test compares their names and order with the numbered steps of
//! spec §7.1 to §7.5, so the report and the spec cannot drift apart.

use crate::ast::*;
use crate::fmt::formula_text;
use crate::schedule::{lower, Slot};

/// The step names, in order. The test against the spec uses this.
pub fn step_names() -> Vec<&'static str> {
    crate::schedule::step_names()
}

/// The report for one world. `file` should be prepared (`prepare`), so that extends and
/// constants are worked out.
pub fn order(file: &File, world: &str) -> Result<String, String> {
    let schedule = lower(file, world)?;
    let w = file
        .items
        .iter()
        .find_map(|i| match &i.node {
            Item::World(w) if w.name == world => Some(w),
            _ => None,
        })
        .expect("lower found the world");

    let mut out = format!("world {world}: one tick, in order (spec §7)\n");
    let mut unplaced = Vec::new();
    // (step, rule, the statements of the rule that run in that step); a rule of several kinds
    // appears under each of its steps.
    let mut placed = Vec::new();
    for r in &schedule.rules {
        match r.placements() {
            Ok(parts) => placed.extend(parts.into_iter().map(|(slot, stmts)| (slot, r, stmts))),
            Err(why) => unplaced.push(format!("rule {} on {}: not placed, {why}", r.name, r.on)),
        }
    }
    let reaction = |name: &str| {
        w.decls.iter().find_map(|d| match &d.node {
            WDecl::Reaction { name: n, formula, .. } if n == name => Some(formula_text(formula)),
            _ => None,
        })
    };

    let mut phase = None;
    let mut n = 0;
    for s in &schedule.steps {
        if phase != Some(s.phase) {
            phase = Some(s.phase);
            n = 0;
            out.push_str(&format!("\n{}\n", s.phase.name()));
        }
        n += 1;
        out.push_str(&format!("  {n}. {:<24} {}\n", s.name, s.what));
        out.push_str(&format!("       reads {}; settles: {}\n", s.reads.text(), s.settles.text()));
        match s.name {
            "Venting" => {
                for (substance, sink) in &schedule.waste {
                    out.push_str(&format!("       waste {substance} -> {sink}\n"));
                }
            }
            "Disposal" => {
                if let Some(d) = &schedule.disposal {
                    if let Some(r) = &d.decompose {
                        let f = reaction(r).map(|f| format!(": {f}")).unwrap_or_default();
                        out.push_str(&format!("       decompose: {r}{f}, to its full extent, before the inventory goes out\n"));
                    }
                    if let Some(r) = &d.remains {
                        out.push_str(&format!("       remains: {r}\n"));
                    }
                }
            }
            _ => {}
        }
        if s.slot != Slot::None {
            for (slot, r, stmts) in &placed {
                if *slot != s.slot {
                    continue;
                }
                out.push_str(&format!("       rule {} on {}\n", r.name, r.on));
                for st in stmts {
                    out.push_str(&format!("         {}\n", st.text));
                }
            }
        }
    }
    for u in unplaced {
        out.push_str(&format!("\n{u}\n"));
    }
    Ok(out)
}
