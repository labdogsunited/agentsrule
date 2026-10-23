//! The checked tick schedule of one world (`proposals/checked-schedule-ir.md` §2).
//!
//! `lower` reads a prepared file once and says what a tick of the world does, in what phase and
//! step, and with what reading and settlement rule. `emerge order` formats this and `emerge-ref`
//! takes its rule placements, its creature capability and its disposal descriptor from it, so
//! neither works out phases from the syntax tree on its own.
//!
//! The steps are fixed engine behaviour, written down once in `STEPS`. A test compares their names
//! and order with the numbered steps of spec §7.1 to §7.5, so the report and the spec cannot drift
//! apart. World rules are placed by what their statements do: the one classifier is `leaf_slot`.

use crate::ast::*;
use crate::fmt::stmt_text;

/// The four phases of a tick (`core.md` §6.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Set,
    Move,
    Transform,
    Restructure,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Set => "Set",
            Phase::Move => "Move",
            Phase::Transform => "Transform",
            Phase::Restructure => "Restructure",
        }
    }
}

/// Where world rules of one kind run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    None,
    Signal,
    Transfer,
    React,
}

/// What a step reads when it forms its request (`core.md` §6.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reads {
    /// The state at the start of the tick.
    Snapshot,
    /// Signals as the previous mind step left them.
    Signals,
    /// The balance as earlier steps of the phase left it.
    Live,
    /// Conserved values after Transform, free values after Set and Move.
    PostTransform,
    /// Nothing about the world: the step moves memory around.
    Nothing,
}

impl Reads {
    pub fn text(self) -> &'static str {
        match self {
            Reads::Snapshot => "the snapshot",
            Reads::Signals => "signals after the previous mind step",
            Reads::Live => "live balances",
            Reads::PostTransform => "values after Transform",
            Reads::Nothing => "nothing",
        }
    }
}

/// How a step applies its effect (`core.md` §6.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Settles {
    /// Writes combine by the declared rule and commit together at the end of the step.
    Combined,
    /// One after another against live balances, in genome or declaration order.
    Sequential,
    /// Two stages: against the origins' balances after fees, then the destinations' headroom.
    Arbitrated,
    /// Reactions one after another against live stores, in genome or declaration order.
    Ordered,
    /// One after another in entity order against live stores.
    Restructured,
    /// Bookkeeping for the next tick; no effect on the world.
    Maintenance,
}

impl Settles {
    pub fn text(self) -> &'static str {
        match self {
            Settles::Combined => "combined writes, committed together",
            Settles::Sequential => "in order, against live balances",
            Settles::Arbitrated => "two-stage arbitration (origins, then destinations)",
            Settles::Ordered => "in order, against live stores",
            Settles::Restructured => "in entity order, against live stores",
            Settles::Maintenance => "maintenance",
        }
    }
}

/// One step of the tick.
pub struct Step {
    pub phase: Phase,
    pub name: &'static str,
    pub slot: Slot,
    pub what: &'static str,
    /// Only a world with the creature capability has this step.
    pub creatures: bool,
    pub reads: Reads,
    pub settles: Settles,
}

impl Step {
    /// A stable name for the step, such as `Move.Transfers`.
    pub fn id(&self) -> String {
        format!("{}.{}", self.phase.name(), self.name.replace(' ', "_"))
    }
}

const fn step(phase: Phase, name: &'static str, slot: Slot, creatures: bool, reads: Reads, settles: Settles, what: &'static str) -> Step {
    Step { phase, name, slot, what, creatures, reads, settles }
}

/// Spec §7.1 to §7.5, in order. `name` is the bold title the spec gives the step.
pub const STEPS: &[Step] = &[
    step(Phase::Set, "Senses", Slot::None, true, Reads::Snapshot, Settles::Combined, "each living entity reads the snapshot"),
    step(Phase::Set, "Minds", Slot::None, true, Reads::Signals, Settles::Combined, "regulation genes run on the entity's own signals only"),
    step(Phase::Set, "Intents", Slot::None, true, Reads::Signals, Settles::Combined, "action genes become intents; reaction genes set their speed"),
    step(Phase::Set, "Field signals", Slot::Signal, false, Reads::Snapshot, Settles::Combined, "reads the snapshot; all writes commit together"),
    step(Phase::Move, "Upkeep", Slot::None, true, Reads::Live, Settles::Sequential, "pays from the balance; an entity that cannot pay is dying"),
    step(Phase::Move, "Action fees", Slot::None, true, Reads::Live, Settles::Sequential, "charged in genome order; an unaffordable intent is dropped"),
    step(Phase::Move, "Transfers", Slot::Transfer, false, Reads::Snapshot, Settles::Arbitrated, "entity transfers and these rules share one arbitration"),
    step(Phase::Move, "Movement", Slot::None, true, Reads::Snapshot, Settles::Combined, "entities turn and move; cells change at the end of the tick"),
    step(Phase::Transform, "Entity reactions", Slot::None, true, Reads::Live, Settles::Ordered, "each entity's reaction genes, in genome order"),
    step(Phase::Transform, "World reactions", Slot::React, false, Reads::Live, Settles::Ordered, "in each cell, in declaration order, with the balances Move left"),
    step(Phase::Transform, "Venting", Slot::None, false, Reads::Live, Settles::Ordered, "waste substances go to their sinks"),
    step(Phase::Restructure, "Disposal", Slot::None, true, Reads::PostTransform, Settles::Restructured, "dying and aged entities die; their inventory goes to the cell or `remains`"),
    step(Phase::Restructure, "Division", Slot::None, true, Reads::PostTransform, Settles::Restructured, "checks inventories after Transform; children join next tick"),
    step(Phase::Restructure, "Newborns", Slot::None, true, Reads::Nothing, Settles::Restructured, "join at the end of the tick"),
    step(Phase::Restructure, "Compaction and binning", Slot::None, true, Reads::Nothing, Settles::Maintenance, "prepare the next tick"),
];

/// The step names, in order. The test against the spec uses this.
pub fn step_names() -> Vec<&'static str> {
    STEPS.iter().map(|s| s.name).collect()
}

/// What `Disposal` does with the settings of the world (spec §7.5, item 1). The rest is fixed:
/// an entity is disposed of when it is dying or `age + 1 >= max_age`, `Starved` takes precedence
/// over `Age`, `--accounts` counts the inventory before `decompose` runs, `decompose` runs to its
/// input-limited extent with no cap on its outputs, and each substance goes to the entity's cell
/// as far as the cell has headroom (entities in index order, substances in declaration order)
/// and the rest to `remains`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dispose {
    /// The reaction run on the inventory first, if any (`decompose: rot;`).
    pub decompose: Option<String>,
    /// The sink that takes what the cell cannot (`remains: void;`).
    pub remains: Option<String>,
}

/// What one statement of a rule does, and in which step it runs.
#[derive(Clone, Debug)]
pub struct StmtLowering {
    pub pos: Pos,
    pub label: Option<String>,
    pub text: String,
    pub slot: Result<Slot, String>,
}

/// One world rule: its statements, and where the rule as a whole runs.
#[derive(Clone, Debug)]
pub struct RuleLowering {
    pub name: String,
    pub on: String,
    pub pos: Pos,
    /// `Err` for a rule that mixes kinds of effect, has none, or uses a call with no phase.
    pub slot: Result<Slot, String>,
    pub stmts: Vec<StmtLowering>,
}

impl RuleLowering {
    /// Where the rule's statements run: one step for a rule of one kind, one entry per step for a
    /// rule that mixes kinds (the runner runs each statement in its own step). `Err` when some
    /// statement has no step either, or the rule has no effect at all. Statements without an
    /// effect (`let`) are left out of a mixed rule.
    pub fn placements(&self) -> Result<Vec<(Slot, Vec<&StmtLowering>)>, String> {
        let whole = match &self.slot {
            Ok(slot) => return Ok(vec![(*slot, self.stmts.iter().collect())]),
            Err(why) => why,
        };
        let mut out: Vec<(Slot, Vec<&StmtLowering>)> = Vec::new();
        for st in &self.stmts {
            match &st.slot {
                Ok(slot) => match out.iter_mut().find(|(s, _)| s == slot) {
                    Some((_, v)) => v.push(st),
                    None => out.push((*slot, vec![st])),
                },
                Err(why) if why == NO_EFFECT => {}
                Err(_) => return Err(whole.clone()),
            }
        }
        if out.is_empty() {
            return Err(whole.clone());
        }
        Ok(out)
    }

    /// The path of a statement: `Dish.rules.weather@transfer_nitrate`, or the rule's alone.
    pub fn path(&self, world: &str, st: &StmtLowering) -> String {
        match &st.label {
            Some(l) => format!("{world}.rules.{}@{l}", self.name),
            None => format!("{world}.rules.{}", self.name),
        }
    }
}

/// The checked schedule of one world.
pub struct Schedule {
    pub world: String,
    /// Whether the world has the creature capability (`checked-schedule-ir.md` §3).
    pub creatures: bool,
    /// The steps this world has, in order.
    pub steps: Vec<&'static Step>,
    /// World rules in declaration order. The runner builds its laws in this order, which fixes
    /// each request's position and so its random draws.
    pub rules: Vec<RuleLowering>,
    pub waste: Vec<(String, String)>,
    /// `Some` exactly when the world has creatures.
    pub disposal: Option<Dispose>,
}

/// A world has the creature capability when any lineage of the project is for it, or it declares
/// a menu, `upkeep`, `remains` or `decompose`. `--lineages` does not matter, so whether a file is
/// valid does not depend on run options.
pub fn needs_creatures(file: &File, w: &World) -> bool {
    file.items.iter().any(|i| matches!(&i.node, Item::Lineage(l) if l.world == w.name))
        || w.decls.iter().any(|d| match &d.node {
            WDecl::Menu { .. } => true,
            WDecl::Config { name, .. } => name == "upkeep",
            WDecl::Setting(e) => e.key == "remains" || e.key == "decompose",
            _ => false,
        })
}

/// Lowers world `world` of a prepared file (`prepare`, so that extends and constants are
/// worked out).
pub fn lower(file: &File, world: &str) -> Result<Schedule, String> {
    let w = file
        .items
        .iter()
        .find_map(|i| match &i.node {
            Item::World(w) if w.name == world => Some(w),
            _ => None,
        })
        .ok_or_else(|| format!("no world `{world}`"))?;
    let creatures = needs_creatures(file, w);

    let mut rules = Vec::new();
    let mut waste = Vec::new();
    for d in &w.decls {
        match &d.node {
            WDecl::Rule { name, on, body } => rules.push(RuleLowering {
                name: name.clone(),
                on: on.clone(),
                pos: d.pos,
                slot: block_slot(body),
                stmts: body
                    .iter()
                    .map(|s| StmtLowering {
                        pos: s.pos,
                        label: match &s.node {
                            Stmt::Call { label, .. } => label.clone(),
                            _ => None,
                        },
                        text: stmt_text(&s.node),
                        slot: block_slot(std::slice::from_ref(s)),
                    })
                    .collect(),
            }),
            WDecl::Waste { substance, sink } => waste.push((substance.clone(), sink.clone())),
            _ => {}
        }
    }
    let setting = |key: &str| {
        w.decls.iter().find_map(|d| match &d.node {
            WDecl::Setting(e) if e.key == key => Some(crate::fmt::value_text(&e.value)),
            _ => None,
        })
    };
    Ok(Schedule {
        world: world.to_string(),
        creatures,
        steps: STEPS.iter().filter(|s| creatures || !s.creatures).collect(),
        rules,
        waste,
        disposal: creatures.then(|| Dispose { decompose: setting("decompose"), remains: setting("remains") }),
    })
}

pub(crate) const NO_EFFECT: &str = "it has no effect";
pub(crate) const MIXED: &str = "it mixes kinds of effect";

/// The slot a rule's statements put it in, or why it has none.
pub fn block_slot(body: &[Node<Stmt>]) -> Result<Slot, String> {
    let mut kinds = Vec::new();
    leaves(body, &mut kinds);
    let mut slot = Slot::None;
    for k in kinds {
        let this = leaf_slot(&k)?;
        if slot != Slot::None && slot != this {
            return Err(MIXED.to_string());
        }
        slot = this;
    }
    if slot == Slot::None {
        return Err(NO_EFFECT.to_string());
    }
    Ok(slot)
}

/// The one classifier: which step an effect runs in. A state rule (`life_like`) is a signal rule
/// (spec §7.4).
fn leaf_slot(k: &Leaf) -> Result<Slot, String> {
    Ok(match k {
        Leaf::Assign => Slot::Signal,
        Leaf::Call(n) if *n == "transfer" || *n == "diffuse_conserved" => Slot::Transfer,
        Leaf::Call("react") => Slot::React,
        Leaf::Call("life_like") => Slot::Signal,
        Leaf::Call(n) => return Err(format!("`{n}` has no phase")),
    })
}

enum Leaf<'a> {
    Assign,
    Call(&'a str),
}

fn leaves<'a>(body: &'a [Node<Stmt>], out: &mut Vec<Leaf<'a>>) {
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
            Stmt::For { body, .. } | Stmt::Each { body, .. } => leaves(body, out),
            Stmt::Let { .. } | Stmt::Setting { .. } => {}
        }
    }
}
