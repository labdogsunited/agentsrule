//! `emerge order`: the steps of one tick for a world, with each world rule placed in the
//! phase its effect gives it (spec §7, §7.4).
//!
//! The built-in steps are fixed engine behaviour, written down once in `STEPS`. A test
//! compares their names and order with the numbered steps of spec §7.1 to §7.5, so the
//! report and the spec cannot drift apart.

use crate::ast::*;
use crate::fmt::{formula_text, stmt_text, value_text};

/// Where world rules of one kind run.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    None,
    Signal,
    Transfer,
    React,
}

struct Step {
    phase: &'static str,
    name: &'static str,
    slot: Slot,
    what: &'static str,
}

const fn step(phase: &'static str, name: &'static str, slot: Slot, what: &'static str) -> Step {
    Step { phase, name, slot, what }
}

/// Spec §7.1 to §7.5, in order. `name` is the bold title the spec gives the step.
const STEPS: &[Step] = &[
    step("Set", "Senses", Slot::None, "each living entity reads the snapshot"),
    step("Set", "Minds", Slot::None, "regulation genes run on the entity's own signals only"),
    step("Set", "Intents", Slot::None, "action genes become intents; reaction genes set their speed"),
    step("Set", "Field signals", Slot::Signal, "reads the snapshot; all writes commit together"),
    step("Move", "Upkeep", Slot::None, "pays from the balance; an entity that cannot pay is dying"),
    step("Move", "Action fees", Slot::None, "charged in genome order; an unaffordable intent is dropped"),
    step("Move", "Transfers", Slot::Transfer, "entity transfers and these rules share one arbitration"),
    step("Move", "Movement", Slot::None, "entities turn and move; cells change at the end of the tick"),
    step("Transform", "Entity reactions", Slot::None, "each entity's reaction genes, in genome order"),
    step("Transform", "World reactions", Slot::React, "in each cell, in declaration order, with the balances Move left"),
    step("Transform", "Venting", Slot::None, "waste substances go to their sinks"),
    step("Restructure", "Disposal", Slot::None, "dying and aged entities die; their inventory goes to the cell or `remains`"),
    step("Restructure", "Division", Slot::None, "checks inventories after Transform; children join next tick"),
    step("Restructure", "Newborns", Slot::None, "join at the end of the tick"),
    step("Restructure", "Compaction and binning", Slot::None, "prepare the next tick"),
];

/// The step names, in order. The test against the spec uses this.
pub fn step_names() -> Vec<&'static str> {
    STEPS.iter().map(|s| s.name).collect()
}

/// The slot a rule's statements put it in, or why it has none.
fn slot_of(body: &Block) -> Result<Slot, String> {
    let mut kinds = Vec::new();
    leaves(body, &mut kinds);
    let mut slot = Slot::None;
    for k in kinds {
        let this = match k {
            Leaf::Assign => Slot::Signal,
            Leaf::Call(n) if n == "transfer" || n == "diffuse_conserved" => Slot::Transfer,
            Leaf::Call(n) if n == "react" => Slot::React,
            Leaf::Call(n) => return Err(format!("`{n}` has no phase")),
        };
        if slot != Slot::None && slot != this {
            return Err("it mixes kinds of effect".to_string());
        }
        slot = this;
    }
    if slot == Slot::None {
        return Err("it has no effect".to_string());
    }
    Ok(slot)
}

enum Leaf<'a> {
    Assign,
    Call(&'a str),
}

fn leaves<'a>(body: &'a Block, out: &mut Vec<Leaf<'a>>) {
    for s in body {
        match &s.node {
            Stmt::Assign { .. } => out.push(Leaf::Assign),
            Stmt::Call { call, .. } => out.push(Leaf::Call(&call.name)),
            Stmt::When { then, otherwise, .. } => {
                leaves(then, out);
                if let Some(o) = otherwise {
                    leaves(o, out);
                }
            }
            Stmt::For { body, .. } => leaves(body, out),
            Stmt::Let { .. } | Stmt::Setting { .. } => {}
        }
    }
}

/// The report for one world. `file` should be prepared (`prepare`), so that extends and
/// constants are worked out.
pub fn order(file: &File, world: &str) -> Result<String, String> {
    let w = file
        .items
        .iter()
        .find_map(|i| match &i.node {
            Item::World(w) if w.name == world => Some(w),
            _ => None,
        })
        .ok_or_else(|| format!("no world `{world}`"))?;

    let mut out = format!("world {world}: one tick, in order (spec §7)\n");
    let mut unplaced = Vec::new();
    let mut placed: Vec<(Slot, &str, &str, &Block)> = Vec::new();
    for d in &w.decls {
        if let WDecl::Rule { name, on, body } = &d.node {
            match slot_of(body) {
                Ok(slot) => placed.push((slot, name, on, body)),
                Err(why) => unplaced.push(format!("rule {name} on {on}: not placed, {why}")),
            }
        }
    }
    let setting = |key: &str| {
        w.decls.iter().find_map(|d| match &d.node {
            WDecl::Setting(e) if e.key == key => Some(value_text(&e.value)),
            _ => None,
        })
    };
    let reaction = |name: &str| {
        w.decls.iter().find_map(|d| match &d.node {
            WDecl::Reaction { name: n, formula, .. } if n == name => Some(formula_text(formula)),
            _ => None,
        })
    };

    let mut phase = "";
    let mut n = 0;
    for s in STEPS {
        if s.phase != phase {
            phase = s.phase;
            n = 0;
            out.push_str(&format!("\n{phase}\n"));
        }
        n += 1;
        out.push_str(&format!("  {n}. {:<24} {}\n", s.name, s.what));
        match s.name {
            "Venting" => {
                for d in &w.decls {
                    if let WDecl::Waste { substance, sink } = &d.node {
                        out.push_str(&format!("       waste {substance} -> {sink}\n"));
                    }
                }
            }
            "Disposal" => {
                if let Some(r) = setting("decompose") {
                    let f = reaction(&r).map(|f| format!(": {f}")).unwrap_or_default();
                    out.push_str(&format!("       decompose: {r}{f}, to its full extent, before the inventory goes out\n"));
                }
                if let Some(r) = setting("remains") {
                    out.push_str(&format!("       remains: {r}\n"));
                }
            }
            _ => {}
        }
        if s.slot != Slot::None {
            for (slot, name, on, body) in &placed {
                if *slot != s.slot {
                    continue;
                }
                out.push_str(&format!("       rule {name} on {on}\n"));
                for st in body.iter() {
                    out.push_str(&format!("         {}\n", stmt_text(&st.node)));
                }
            }
        }
    }
    for u in unplaced {
        out.push_str(&format!("\n{u}\n"));
    }
    Ok(out)
}
