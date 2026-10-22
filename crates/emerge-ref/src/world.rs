//! A world of cells and entities, run on the four-phase schedule
//! (`core.md` §6.1, spec §7).


use crate::arbitrate::pro_rata;
use crate::chem::{ChemError, Chemistry, Mu, ReactionId, SubstanceId};
use crate::mind::{sanitize, think, think_contract, value, Gene, Genome, Sense, READING_LIMIT};
use crate::philox::{self, Stream};

pub type SourceId = usize;
pub type SinkId = usize;

/// The most substances a world may declare. Stores are fixed-size arrays, so a world's
/// cells sit in one contiguous block.
pub const MAX_SUBSTANCES: usize = 16;

/// Conserved amounts and caps, one slot per substance. A cap of `0` means the store
/// cannot hold that substance; slots past the world's substances stay at `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Store {
    pub amounts: [Mu; MAX_SUBSTANCES],
    pub caps: [Mu; MAX_SUBSTANCES],
}

impl Store {
    pub fn new(caps: Vec<Mu>) -> Self {
        assert!(caps.len() <= MAX_SUBSTANCES, "at most {MAX_SUBSTANCES} substances");
        let mut c = [0; MAX_SUBSTANCES];
        c[..caps.len()].copy_from_slice(&caps);
        Store { amounts: [0; MAX_SUBSTANCES], caps: c }
    }

    pub fn headroom(&self, s: SubstanceId) -> Mu {
        self.caps[s] - self.amounts[s]
    }
}

/// `Continuous2D` with one grid of cells laid over it (spec §6.1). Cell `(x, y)` has index
/// `y × grid_w + x`; its neighbours are N `(x, y − 1)`, E, S, W, in that order.
#[derive(Debug, Clone, PartialEq)]
pub struct Space {
    pub w: f32,
    pub h: f32,
    pub wrap: bool,
    pub grid_w: usize,
    pub grid_h: usize,
    pub child_offset: f32,
    /// `max_interaction_radius`: how far contact senses, `give` and `steal` reach.
    pub radius: f32,
}

impl Space {
    pub fn cell_at(&self, p: [f32; 2]) -> usize {
        let x = ((p[0] * (self.grid_w as f32 / self.w)).floor() as isize).clamp(0, self.grid_w as isize - 1);
        let y = ((p[1] * (self.grid_h as f32 / self.h)).floor() as isize).clamp(0, self.grid_h as isize - 1);
        y as usize * self.grid_w + x as usize
    }

    /// `place` in contract mode (`contract.rs`): one wrap, no `rem_euclid`.
    pub fn place_contract(&self, p: [f32; 2]) -> [f32; 2] {
        [crate::contract::fit(p[0], self.w, self.wrap), crate::contract::fit(p[1], self.h, self.wrap)]
    }

    pub fn place(&self, p: [f32; 2]) -> [f32; 2] {
        let fit = |v: f32, size: f32| {
            if self.wrap {
                let r = v.rem_euclid(size);
                if r >= size { 0.0 } else { r }
            } else {
                v.clamp(0.0, size.next_down())
            }
        };
        [fit(p[0], self.w), fit(p[1], self.h)]
    }

    /// The neighbour of cell `c` at offset `(dx, dy)`, wrapping or staying at the edge.
    /// The cell under the point `dist` from `p` along `heading` turned `angle_deg` left.
    fn cell_ahead(&self, p: [f32; 2], heading: [f32; 2], angle_deg: f32, dist: f32, contract: bool) -> usize {
        if contract {
            let d = crate::contract::turned(heading, angle_deg);
            return self.cell_at(self.place_contract(crate::contract::advance(p, d, dist)));
        }
        let (s, c) = angle_deg.to_radians().sin_cos();
        let [hx, hy] = heading;
        let d = [hx * c - hy * s, hx * s + hy * c];
        self.cell_at(self.place([p[0] + d[0] * dist, p[1] + d[1] * dist]))
    }

    fn offset(&self, c: usize, dx: isize, dy: isize) -> usize {
        let (gw, gh) = (self.grid_w as isize, self.grid_h as isize);
        let (x, y) = ((c % self.grid_w) as isize + dx, (c / self.grid_w) as isize + dy);
        let (x, y) = if self.wrap {
            (x.rem_euclid(gw), y.rem_euclid(gh))
        } else {
            (x.clamp(0, gw - 1), y.clamp(0, gh - 1))
        };
        (y * gw + x) as usize
    }
}

/// An action gene's intent, formed in Set (spec §5.5, §7.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Act {
    Move { distance: f32 },
    Turn { angle_deg: f32 },
    Take { s: SubstanceId, amount: Mu },
    Drop { s: SubstanceId, amount: Mu },
    /// To or from the entity at this index (the nearest in range, in the snapshot).
    Give { s: SubstanceId, amount: Mu, other: usize },
    Steal { s: SubstanceId, amount: Mu, other: usize },
    /// Drops every `steal` of `s` against this entity this tick.
    Guard { s: SubstanceId },
    Divide,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Intent {
    pub gene: usize,
    pub act: Act,
    pub fee: Mu,
    /// Cleared when the entity is dying or cannot pay the fee.
    pub live: bool,
}

/// What an entity's mind decided in Set: intents in genome order, and each reaction
/// gene's extent cap for Transform.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub intents: Vec<Intent>,
    pub react_caps: Vec<Mu>,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: u64,
    /// The field cell the entity is in, as of the snapshot.
    pub cell: usize,
    pub pos: [f32; 2],
    /// A unit vector.
    pub heading: [f32; 2],
    pub store: Store,
    pub genome: Genome,
    pub signals: Vec<f32>,
    /// Ticks completed.
    pub age: u64,
    /// Index into `World::variation`; children inherit it.
    pub lineage: usize,
    /// µ of `World::track` its reaction genes have produced since this was last reset.
    /// Children start at `0`.
    pub made: u64,
    /// What upkeep needs from the genome. A genome never changes after birth, so this is
    /// worked out once, not every tick.
    pub shape: GenomeShape,
    /// Own amounts at the entity's previous reading, for `delta`; empty until a gene reads it.
    pub prev: Vec<Mu>,
}

/// The parts of a genome that upkeep reads (spec §3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenomeShape {
    pub active_genes: usize,
    pub in_use: Vec<usize>,
    /// Whether some gene senses `delta`.
    pub delta: bool,
}

impl GenomeShape {
    pub fn of(g: &Genome) -> GenomeShape {
        GenomeShape {
            active_genes: g.active_genes(),
            in_use: g.signals_in_use(),
            delta: g.slots.iter().any(|s| matches!(s.gene, Gene::Sense { sense: Sense::Delta(_), .. })),
        }
    }
}

/// Gene-ticks of one reaction, by outcome. A gene that makes something counts as `full`
/// (extent = its cap) or `limited`. One that makes nothing counts once, in this order: a cap
/// of zero (rate, signal or rounding), its first missing input, a full output.
#[derive(Debug, Clone, Default)]
pub struct Blocked {
    pub full: u64,
    pub limited: u64,
    pub cap_zero: u64,
    pub no_input: Vec<u64>,
    pub no_room: u64,
}

/// Transfers of one kind and substance, by which limit, if any, cut them.
#[derive(Debug, Clone, Default)]
pub struct Cuts {
    pub requests: u64,
    /// Cut in stage 1: the origin held less than was asked of it.
    pub by_origin: u64,
    /// Cut in stage 2: the destination had less headroom than arrived.
    pub by_dest: u64,
    pub requested: u64,
    pub delivered: u64,
}

/// Which limits bound transfers and division, over a run.
#[derive(Debug, Clone, Default)]
pub struct Binding {
    /// By (kind, substance); the kind is `TransferKind as usize`.
    pub transfers: std::collections::BTreeMap<(usize, SubstanceId), Cuts>,
    /// First firing `divide` intents: divided, fee unpaid, `requires` unmet, at `max_entities`.
    pub divide: [u64; 4],
}

/// What a transfer is, from its ends.
pub const TRANSFER_KINDS: [&str; 7] = ["take", "drop", "give", "steal", "inflow", "diffusion", "other"];

fn transfer_kind(r: &Req) -> usize {
    match (unpack(r.origin), unpack(r.dest)) {
        (End::Cell(_), End::Entity(_)) => 0,
        (End::Entity(_), End::Cell(_)) => 1,
        (End::Entity(_), End::Entity(_)) => if r.steal { 3 } else { 2 },
        (End::Source(_), _) => 4,
        (End::Cell(_), End::Cell(_)) => 5,
        _ => 6,
    }
}

/// World rules. Each runs on every cell; its kind sets its phase (spec §7.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Law {
    /// Transfer rule: `transfer(s, from: source, to: self, amount, overflow)`. Move.
    Inflow { s: SubstanceId, source: SourceId, amount: Mu, overflow: Option<SinkId> },
    /// Transfer rule: `transfer(s, from: self, to: above|right|below|left, amount)`. Move. `dir` is an
    /// index of `cond::OFFSETS` (0 to 3); a cell with no neighbour there (the edge of a grid that
    /// does not wrap) makes no request.
    Shift { s: SubstanceId, dir: usize, amount: Mu },
    /// Transfer rule: `diffuse_conserved(substances, rate)`. Move.
    Diffuse { substances: Vec<SubstanceId>, rate_bp: u32 },
    /// Reaction rule: `react(reaction, rate)`. Transform.
    React { reaction: ReactionId, rate_bp: u32 },
    /// A `when` around one of the others (`proposals/rule-conditions.md`): it acts in the cells where
    /// the condition holds on the start-of-tick snapshot, and in no others. Never nested: the
    /// conditions of nested `when`s and of `else` are joined into one.
    When { cond: crate::cond::Ex, law: Box<Law> },
}

impl World {
    /// Why the GPU runtime cannot run this world, if it cannot: a world rule's condition that needs
    /// more than the GPU's stack (`cond::GPU_STACK`; `proposals/rule-conditions.md`).
    pub fn gpu_unsupported(&self) -> Option<String> {
        let deepest = self.laws.iter().filter_map(|l| l.parts().1).map(crate::cond::Ex::depth).max()?;
        (deepest > crate::cond::GPU_STACK).then(|| format!("a world rule's `when` condition is nested {deepest} values deep; the GPU runtime evaluates at most {} (the CPU runner has no limit; proposals/rule-conditions.md)", crate::cond::GPU_STACK))
    }
}

impl Law {
    /// The law that acts, and the condition it acts under, if any.
    pub fn parts(&self) -> (&Law, Option<&crate::cond::Ex>) {
        match self {
            Law::When { cond, law } => (law, Some(cond)),
            other => (other, None),
        }
    }

    /// Whether a condition guards it.
    pub fn is_conditional(&self) -> bool {
        matches!(self, Law::When { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upkeep {
    pub pay: SubstanceId,
    pub becomes: SubstanceId,
    pub vent: SinkId,
    pub base: Mu,
    pub per_gene: Mu,
    pub per_signal: Mu,
    pub per_signal_keep: Mu,
    pub per_carried_u: Mu,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sink {
    pub name: String,
    /// `None` accepts any substance.
    pub accepts: Option<Vec<SubstanceId>>,
}

impl Sink {
    pub fn accepts(&self, s: SubstanceId) -> bool {
        self.accepts.as_ref().is_none_or(|list| list.contains(&s))
    }
}

/// Per-group energy and flux accounting (`emerge run --accounts`), read only: turning it on
/// changes nothing in the run. A group is a lineage, split into carriers (an active reaction
/// gene with rate > 0 that makes `flux`) and the rest when `flux` is set. With `clade`, the
/// clade (the listed IDs and all their descendants) and the rest take the place of lineages.
/// Entities are grouped at the start of each tick; a birth counts in the parent's group.
#[derive(Debug, Clone, Default)]
pub struct Accounts {
    pub flux: Option<SubstanceId>,
    /// IDs in the clade, if one is followed; children of members join it at birth.
    pub clade: Option<std::collections::HashSet<u64>>,
    /// Each entity's group this tick.
    pub group: Vec<usize>,
    /// Totals by group since the last `take`, indexed by `ACCOUNT_COLUMNS`.
    pub totals: Vec<[u64; ACCOUNT_COLUMNS.len()]>,
}

/// What `Accounts` counts, in µ unless named otherwise. `pay` is the upkeep substance.
pub const ACCOUNT_COLUMNS: [&str; 29] = [
    "births", "starved", "aged", "upkeep_mu", "fees_mu", "pay_made_mu", "pay_used_mu", "pay_taken_mu",
    "pay_stolen_in_mu", "pay_stolen_out_mu", "flux_made_mu", "flux_taken_mu", "flux_stolen_in_mu",
    "flux_stolen_out_mu", "flux_used_mu",
    "pay_gift_in_mu", "pay_gift_out_mu", "pay_dropped_mu", "pay_wasted_mu", "pay_died_with_mu",
    "pay_to_child_mu", "pay_from_parent_mu",
    "flux_gift_in_mu", "flux_gift_out_mu", "flux_dropped_mu", "flux_wasted_mu", "flux_died_with_mu",
    "flux_to_child_mu", "flux_from_parent_mu",
];
// Per substance (pay, then flux), the columns that close the budget of a group: its stock
// changes by made - used + taken + stolen_in - stolen_out + gift_in - gift_out - dropped
// - wasted - died_with + from_parent - to_child (and, for pay, - upkeep - fees). `died_with` is
// what the entity held when it died, before `decompose` ran. Entities added by `--inject`
// or by genesis bring their stock with them and are not counted.
const A_PAY_GIFT_IN: usize = 15;
const A_PAY_GIFT_OUT: usize = 16;
const A_PAY_DROPPED: usize = 17;
const A_PAY_WASTED: usize = 18;
const A_PAY_DIED: usize = 19;
const A_PAY_TO_CHILD: usize = 20;
const A_PAY_FROM_PARENT: usize = 21;
const A_FLUX_GIFT_IN: usize = 22;
const A_FLUX_GIFT_OUT: usize = 23;
const A_FLUX_DROPPED: usize = 24;
const A_FLUX_WASTED: usize = 25;
const A_FLUX_DIED: usize = 26;
const A_FLUX_TO_CHILD: usize = 27;
const A_FLUX_FROM_PARENT: usize = 28;
const A_BIRTHS: usize = 0;
const A_STARVED: usize = 1;
const A_AGED: usize = 2;
const A_UPKEEP: usize = 3;
const A_FEES: usize = 4;
const A_PAY_MADE: usize = 5;
const A_PAY_USED: usize = 6;
const A_PAY_TAKEN: usize = 7;
const A_PAY_IN: usize = 8;
const A_PAY_OUT: usize = 9;
const A_FLUX_MADE: usize = 10;
const A_FLUX_TAKEN: usize = 11;
const A_FLUX_IN: usize = 12;
const A_FLUX_OUT: usize = 13;
const A_FLUX_USED: usize = 14;

impl Accounts {
    fn add(&mut self, g: usize, k: usize, x: u64) {
        if self.totals.len() <= g {
            self.totals.resize(g + 1, [0; ACCOUNT_COLUMNS.len()]);
        }
        self.totals[g][k] += x;
    }

    /// The group of an entity: `base`, or `2 × base + carrier` when `flux` is set, where `base`
    /// is the lineage, or 1 in the clade and 0 outside it.
    pub fn group_of(&self, chem: &Chemistry, e: &Entity) -> usize {
        let base = match &self.clade {
            Some(c) => c.contains(&e.id) as usize,
            None => e.lineage,
        };
        let Some(f) = self.flux else { return base };
        let carrier = e.genome.slots.iter().enumerate().any(|(g, slot)| {
            e.genome.active(g)
                && matches!(slot.gene, Gene::React { reaction, rate, .. } if rate > 0 && chem.reactions[reaction].outputs.iter().any(|o| o.0 == f))
        });
        2 * base + carrier as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Starved,
    Age,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Died { tick: u64, id: u64, cause: Cause },
    Born { tick: u64, parent: u64, child: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum End {
    Cell(usize),
    /// Index into `World::entities` during the tick.
    Entity(usize),
    Source(SourceId),
    Sink(SinkId),
}

/// One transfer request and how arbitration settled it (spec §7.2 step 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    pub origin: End,
    pub dest: End,
    pub s: SubstanceId,
    pub requested: Mu,
    pub overflow: Option<SinkId>,
    pub stage1: Mu,
    pub stage2: Mu,
    pub delivered: Mu,
    pub overflowed: Mu,
}

/// Who and what behind `last_transfers[k]` (same index): the creatures at the two ends, by ID as the tick
/// ran (their positions in `entities` move afterwards), and the world-rule statement that asked, or `None` for
/// a creature's own action. Observation only: not in the state digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferMeta {
    pub origin_id: Option<u64>,
    pub dest_id: Option<u64>,
    pub law: Option<u32>,
    /// From a `steal` intent.
    pub steal: bool,
}

/// A transfer request packed for arbitration: endpoints are tagged `u32`s (`pack`).
#[derive(Debug, Clone, Copy)]
struct Req {
    origin: u32,
    dest: u32,
    s: u16,
    /// `u16::MAX` for none.
    overflow: u16,
    requested: Mu,
    /// From a `steal` intent, for `World::stolen`.
    steal: bool,
}

const TAG_SHIFT: u32 = 30;
const INDEX: u32 = (1 << TAG_SHIFT) - 1;

fn pack(end: End) -> u32 {
    let (tag, i) = match end {
        End::Cell(i) => (0, i),
        End::Entity(i) => (1, i),
        End::Source(i) => (2, i),
        End::Sink(i) => (3, i),
    };
    debug_assert!(i as u32 <= INDEX);
    (tag << TAG_SHIFT) | i as u32
}

fn unpack(p: u32) -> End {
    let i = (p & INDEX) as usize;
    match p >> TAG_SHIFT {
        0 => End::Cell(i),
        1 => End::Entity(i),
        2 => End::Source(i),
        _ => End::Sink(i),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerError {
    pub tick: u64,
    pub element: String,
    /// Σ stores + absorbed − emitted, which must be zero.
    pub drift: i128,
}

/// What one world rule's statement did, counted over every tick since the world began or its rules last
/// changed (`LawStat`s are observation only: they are not in the state digest). Cells: how many
/// were looked at, and how many a `when` let through. A transfer: its requests, the µ asked for and the
/// µ delivered, and how many requests an origin's balance or a destination's room cut. A reaction: the
/// cells where it ran with a positive extent, and the extent in total.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LawStat {
    pub cells: u64,
    pub passed: u64,
    pub requests: u64,
    pub requested: u64,
    pub delivered: u64,
    pub cut_by_origin: u64,
    pub cut_by_dest: u64,
    pub ran: u64,
    pub extent: u64,
}

/// Where the inventory of the dead went (observation only, not in the state digest): after `decompose` ran, what each
/// lineage's dead put into the cell they died in and what did not fit and went to the `remains` sink, by substance (µ).
#[derive(Clone, Debug, Default)]
pub struct Disposal {
    pub to_cell: Vec<Vec<u64>>,
    pub to_remains: Vec<Vec<u64>>,
}

#[derive(Clone)]
pub struct World {
    /// Where the dead's inventory went since the world began (see `Disposal`).
    pub disposal: Disposal,
    /// Per law, in `laws` order: what it did (see `LawStat`); reset when the laws change.
    pub law_stats: Vec<LawStat>,
    /// The tick `law_stats` began counting at.
    pub law_since: u64,
    /// Which law made each request of the tick (`u32::MAX`: a creature's own action).
    pub req_law: Vec<u32>,
    /// With `record_transfers`: who and what behind each of `last_transfers`.
    pub last_transfer_meta: Vec<TransferMeta>,
    pub chem: Chemistry,
    pub cells: Vec<Store>,
    pub neighbours: Vec<Vec<usize>>,
    /// Set for worlds whose cells are a grid over continuous space.
    pub space: Option<Space>,
    pub entities: Vec<Entity>,
    pub sources: Vec<String>,
    pub sinks: Vec<Sink>,
    pub laws: Vec<Law>,
    /// Per law, in the cells of a conditional reaction rule, whether its condition held when the tick's
    /// requests were made (the start-of-tick state); used in Transform, after the transfers.
    pub react_masks: Vec<Option<Vec<bool>>>,
    /// State fields (`field NAME : Grid(…) states […]`): one discrete state per cell, outside the
    /// ledger, updated at the start of Set from the previous states (`proposals/field-rules.md`).
    pub states: Vec<StateField>,
    /// Whether the world has the creature capability: entities, with `upkeep` and `remains`
    /// (`proposals/checked-schedule-ir.md` §3). Without it `upkeep` and `remains` are placeholders
    /// that nothing reads, and no entity can be added.
    pub creatures: bool,
    pub upkeep: Upkeep,
    pub waste: Vec<(SubstanceId, SinkId)>,
    pub remains: SinkId,
    /// A reaction run to completion on a dead entity's inventory before disposal places it.
    pub decompose: Option<ReactionId>,
    pub max_age: u64,
    pub max_entities: usize,
    pub mind_steps: u32,
    /// Contract mode (`contract.rs`): float operations a GPU can reproduce bit for bit.
    pub contract: bool,
    pub inventory_cap: Mu,
    pub seed: u64,
    pub tick: u64,
    pub next_id: u64,
    /// Cumulative µ per element emitted by sources and absorbed by sinks.
    pub emitted: Vec<u64>,
    pub absorbed: Vec<u64>,
    /// Births and deaths are always counted; `events` lists them only if `record_events`.
    pub record_events: bool,
    pub events: Vec<Event>,
    pub births: u64,
    pub deaths: u64,
    /// Cumulative extent of each reaction run by entity genes.
    pub entity_extent: Vec<u64>,
    /// Cumulative µ delivered by `steal`, over all substances.
    pub stolen: u64,
    /// If set, each birth is listed in `genome_changes` as `(tick, parent ID, child ID,
    /// changes)`, with no changes when the child's genome equals its parent's.
    pub record_genome_changes: bool,
    pub genome_changes: Vec<(u64, u64, u64, Vec<crate::diff::Change>)>,
    /// With `record_genome_changes`, each death as `(tick, ID, cause, age)`.
    pub deaths_log: Vec<(u64, u64, Cause, u32)>,
    /// Per-group accounting, if set (`--accounts`).
    pub accounts: Option<Accounts>,
    /// Cumulative µ of `steal` requests dropped by `guard`.
    pub guarded: u64,
    /// If set, why reaction genes ran or not, by reaction (`--blocked`).
    pub blocked: Option<Vec<Blocked>>,
    /// Experimental (`--one-per-substance`, not in the spec): only the first firing gene of
    /// each substance action and substance counts per tick, as for `move`; later copies are
    /// dropped and pay nothing.
    pub one_per_substance: bool,
    /// If set, which limits cut transfers and division (`--blocked`).
    pub binding: Option<Binding>,
    /// The substance whose production each entity counts in `Entity::made`.
    pub track: Option<SubstanceId>,
    /// The transfers of the last tick, in request order, if `record_transfers`.
    pub record_transfers: bool,
    pub last_transfers: Vec<Transfer>,
    /// Variation at division (spec §8.1): off for control runs (`--mutation off`).
    pub mutation: bool,
    /// Each lineage's rates and capacity, by `Entity::lineage`; `None` never mutates.
    pub variation: Vec<Option<(crate::mutate::Rates, crate::mutate::Capacity)>>,
    /// What mutation may produce.
    pub menus: crate::mutate::Menus,
    /// Buffers reused from tick to tick, so long runs do not allocate megabytes per tick.
    requests: Vec<Req>,
    sums: Vec<u64>,
    stage1: Vec<Mu>,
    stage2: Vec<Mu>,
    over: Vec<(usize, u32)>,
    /// The snapshot's positions and IDs, binned for contact (spec §5.5).
    near: crate::near::Index,
    snap_pos: Vec<[f32; 2]>,
    snap_ids: Vec<u64>,
}

// The state digest of a run is a hash of this output (`emerge-server`'s `Run::digest`), so it must not
// change when a field that is not state is added: `react_masks` is scratch space between two phases of
// a tick, and is left out, which keeps every recorded digest valid.
impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("chem", &self.chem)
            .field("cells", &self.cells)
            .field("neighbours", &self.neighbours)
            .field("space", &self.space)
            .field("entities", &self.entities)
            .field("sources", &self.sources)
            .field("sinks", &self.sinks)
            .field("laws", &self.laws)
            .field("states", &self.states)
            .field("creatures", &self.creatures)
            .field("upkeep", &self.upkeep)
            .field("waste", &self.waste)
            .field("remains", &self.remains)
            .field("decompose", &self.decompose)
            .field("max_age", &self.max_age)
            .field("max_entities", &self.max_entities)
            .field("mind_steps", &self.mind_steps)
            .field("contract", &self.contract)
            .field("inventory_cap", &self.inventory_cap)
            .field("seed", &self.seed)
            .field("tick", &self.tick)
            .field("next_id", &self.next_id)
            .field("emitted", &self.emitted)
            .field("absorbed", &self.absorbed)
            .field("record_events", &self.record_events)
            .field("events", &self.events)
            .field("births", &self.births)
            .field("deaths", &self.deaths)
            .field("entity_extent", &self.entity_extent)
            .field("stolen", &self.stolen)
            .field("record_genome_changes", &self.record_genome_changes)
            .field("genome_changes", &self.genome_changes)
            .field("deaths_log", &self.deaths_log)
            .field("accounts", &self.accounts)
            .field("guarded", &self.guarded)
            .field("blocked", &self.blocked)
            .field("one_per_substance", &self.one_per_substance)
            .field("binding", &self.binding)
            .field("track", &self.track)
            .field("record_transfers", &self.record_transfers)
            .field("last_transfers", &self.last_transfers)
            .field("mutation", &self.mutation)
            .field("variation", &self.variation)
            .field("menus", &self.menus)
            .field("requests", &self.requests)
            .field("sums", &self.sums)
            .field("stage1", &self.stage1)
            .field("stage2", &self.stage2)
            .field("over", &self.over)
            .field("near", &self.near)
            .field("snap_pos", &self.snap_pos)
            .field("snap_ids", &self.snap_ids)
            .finish()
    }
}

impl World {
    /// A world with creatures: `upkeep` and `remains` are checked together, so a payment route
    /// that does not close is an error.
    pub fn new(chem: Chemistry, upkeep: Upkeep, sinks: Vec<Sink>, remains: SinkId) -> Result<Self, ChemError> {
        let pay = &chem.substances[upkeep.pay];
        let becomes = &chem.substances[upkeep.becomes];
        if pay.comp != becomes.comp {
            return Err(ChemError::PaymentRoute { reason: "pay and becomes differ in composition".into() });
        }
        if becomes.potential - pay.potential > -chem.min_drop {
            return Err(ChemError::PaymentRoute { reason: "pay -> becomes is not downhill".into() });
        }
        if !sinks[upkeep.vent].accepts(upkeep.becomes) {
            return Err(ChemError::PaymentRoute { reason: "vent does not accept becomes".into() });
        }
        Ok(Self::build(chem, true, upkeep, sinks, remains))
    }

    /// A world without creatures (a state automaton, a chemistry on a field): no upkeep, no
    /// `remains`, and `spawn*` and `inject` are refused.
    pub fn without_creatures(chem: Chemistry, sinks: Vec<Sink>) -> Self {
        let upkeep = Upkeep { pay: 0, becomes: 0, vent: 0, base: 0, per_gene: 0, per_signal: 0, per_signal_keep: 0, per_carried_u: 0 };
        Self::build(chem, false, upkeep, sinks, 0)
    }

    fn build(chem: Chemistry, creatures: bool, upkeep: Upkeep, sinks: Vec<Sink>, remains: SinkId) -> Self {
        let n = chem.elements.len();
        let reactions = chem.reactions.len();
        World {
            chem,
            cells: Vec::new(),
            neighbours: Vec::new(),
            space: None,
            entities: Vec::new(),
            sources: Vec::new(),
            sinks,
            laws: Vec::new(),
            react_masks: Vec::new(),
            law_stats: Vec::new(),
            disposal: Disposal::default(),
            law_since: 0,
            req_law: Vec::new(),
            last_transfer_meta: Vec::new(),
            states: Vec::new(),
            creatures,
            upkeep,
            waste: Vec::new(),
            remains,
            decompose: None,
            max_age: u64::MAX,
            max_entities: usize::MAX,
            mind_steps: 1,
            contract: false,
            inventory_cap: 100 * crate::chem::U,
            seed: 0,
            tick: 0,
            next_id: 1,
            emitted: vec![0; n],
            absorbed: vec![0; n],
            record_events: true,
            events: Vec::new(),
            births: 0,
            deaths: 0,
            entity_extent: vec![0; reactions],
            stolen: 0,
            accounts: None,
            record_genome_changes: false,
            genome_changes: Vec::new(),
            deaths_log: Vec::new(),
            guarded: 0,
            blocked: None,
            one_per_substance: false,
            binding: None,
            track: None,
            record_transfers: true,
            last_transfers: Vec::new(),
            mutation: false,
            variation: Vec::new(),
            menus: crate::mutate::Menus::default(),
            requests: Vec::new(),
            sums: Vec::new(),
            stage1: Vec::new(),
            stage2: Vec::new(),
            over: Vec::new(),
            near: crate::near::Index::default(),
            snap_pos: Vec::new(),
            snap_ids: Vec::new(),
        }
    }

    pub fn add_source(&mut self, name: &str) -> SourceId {
        self.sources.push(name.to_string());
        self.sources.len() - 1
    }

    /// Adds a cell with per-substance caps (`0` = cannot hold).
    pub fn add_cell(&mut self, caps: Vec<Mu>) -> usize {
        assert_eq!(caps.len(), self.chem.substances.len());
        self.cells.push(Store::new(caps));
        self.neighbours.push(Vec::new());
        self.cells.len() - 1
    }

    pub fn link(&mut self, a: usize, b: usize) {
        self.neighbours[a].push(b);
        self.neighbours[b].push(a);
    }

    /// Lays a grid of cells with the given caps over `space`, linked N, E, S, W.
    /// Must be called before any other cell is added.
    pub fn grid(&mut self, space: Space, caps: Vec<Mu>) {
        assert!(self.cells.is_empty());
        for _ in 0..space.grid_w * space.grid_h {
            self.add_cell(caps.clone());
        }
        for c in 0..self.cells.len() {
            let mut nbs = Vec::with_capacity(4);
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let nb = space.offset(c, dx, dy);
                if nb != c {
                    nbs.push(nb);
                }
            }
            self.neighbours[c] = nbs;
        }
        self.space = Some(space);
    }

    /// Adds an entity with an empty inventory and one module of expressed genes.
    pub fn spawn(&mut self, cell: usize, genes: Vec<Gene>) -> usize {
        self.spawn_genome(cell, Genome::simple(genes))
    }

    /// Adds an entity with an empty inventory. Its signals start at `0.0`.
    pub fn spawn_genome(&mut self, cell: usize, genome: Genome) -> usize {
        assert!(self.creatures, "this world has no creature capability (`upkeep` and `remains`); it cannot hold entities");
        let caps = vec![self.inventory_cap; self.chem.substances.len()];
        let signals = vec![0.0; genome.keep.len()];
        let shape = GenomeShape::of(&genome);
        self.entities.push(Entity {
            id: self.next_id,
            cell,
            pos: [0.0, 0.0],
            heading: [1.0, 0.0],
            store: Store::new(caps),
            genome,
            signals,
            age: 0,
            lineage: 0,
            made: 0,
            shape,
            prev: Vec::new(),
        });
        self.next_id += 1;
        self.entities.len() - 1
    }

    /// Adds an entity at a position in space, in the cell under it.
    pub fn spawn_at(&mut self, pos: [f32; 2], heading: [f32; 2], genome: Genome) -> usize {
        let space = self.space.as_ref().expect("spawn_at needs a space");
        let pos = space.place(pos);
        let cell = space.cell_at(pos);
        let i = self.spawn_genome(cell, genome);
        self.entities[i].pos = pos;
        self.entities[i].heading = heading;
        i
    }

    /// Adds the entities of `other`, a world compiled from the same file with only the
    /// lineage `lineage` of it, as a new lineage. They get new IDs, and their stores are
    /// endowed here, so the ledger stays balanced. Returns the new lineage's index, or an error
    /// for a world without the creature capability.
    pub fn inject(&mut self, other: World, lineage: usize) -> Result<usize, String> {
        if !self.creatures {
            return Err("this world has no creature capability (`upkeep` and `remains`); it cannot take an injected lineage".into());
        }
        let index = self.variation.len();
        self.variation.push(other.variation[lineage].clone());
        for e in other.entities.into_iter().filter(|e| e.lineage == lineage) {
            let i = self.spawn_at(e.pos, e.heading, e.genome);
            self.entities[i].lineage = index;
            for s in 0..self.chem.substances.len() {
                if e.store.amounts[s] > 0 {
                    self.endow(End::Entity(i), s, e.store.amounts[s]);
                }
            }
        }
        Ok(index)
    }

    /// Genesis: moves `amount` from a source into a store, recording it as emitted.
    pub fn endow(&mut self, to: End, s: SubstanceId, amount: Mu) {
        let store = match to {
            End::Cell(c) => &mut self.cells[c],
            End::Entity(e) => &mut self.entities[e].store,
            _ => panic!("endow needs a store"),
        };
        assert!(amount <= store.headroom(s), "endowment exceeds cap");
        store.amounts[s] += amount;
        add_elements(&mut self.emitted, &self.chem, s, amount);
    }

    pub fn entity_by_id(&self, id: u64) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }

    /// Checks `Σ stores + absorbed − emitted == 0` for every element (spec §3.1).
    pub fn audit(&self) -> Result<(), LedgerError> {
        for (e, name) in self.chem.elements.iter().enumerate() {
            let mut total: i128 = 0;
            let stores = self.cells.iter().chain(self.entities.iter().map(|x| &x.store));
            for store in stores {
                for (s, &amount) in store.amounts[..self.chem.substances.len()].iter().enumerate() {
                    total += amount as i128 * self.chem.substances[s].comp[e] as i128;
                }
            }
            let drift = total + self.absorbed[e] as i128 - self.emitted[e] as i128;
            if drift != 0 {
                return Err(LedgerError { tick: self.tick, element: name.clone(), drift });
            }
        }
        Ok(())
    }

    /// Runs one tick: Set, Move, Transform, Restructure. Audits the ledger afterwards.
    pub fn step(&mut self) -> Result<(), LedgerError> {
        self.run_tick();
        self.audit()
    }

    /// Runs one tick without auditing, for long runs that audit every `K` ticks.
    pub fn run_tick(&mut self) {
        self.begin_tick();
        // State rules are signal rules (spec §7.4): every cell reads the previous states, and the
        // next states are committed together. Nothing else in the tick reads a state field.
        for f in &mut self.states {
            f.step();
        }
        let n = self.entities.len();
        // ---- 1 Set ----
        // Senses read the snapshot, minds run, action genes become intents (spec §7.1).
        let plans = self.set_phase();
        self.finish_tick_n(n, plans);
    }

    /// The contact index of the current snapshot (after `begin_tick`).
    pub fn near_index(&self) -> &crate::near::Index {
        &self.near
    }

    /// Takes the snapshot the tick reads: positions, IDs and the contact index.
    pub fn begin_tick(&mut self) {
        if let Some(space) = &self.space {
            self.snap_pos.clear();
            self.snap_pos.extend(self.entities.iter().map(|e| e.pos));
            self.snap_ids.clear();
            self.snap_ids.extend(self.entities.iter().map(|e| e.id));
            self.near = crate::near::Index::build(space, self.snap_pos.iter().copied());
            self.near.contract = self.contract;
        }

        if let Some(a) = &mut self.accounts {
            let group = self.entities.iter().map(|e| a.group_of(&self.chem, e)).collect();
            a.group = group;
        }
    }

    /// Set (spec §7.1) for every entity, after `begin_tick`: updates the signals (and `prev`
    /// of entities with a `delta` gene) and returns each entity's intents and reaction caps.
    pub fn set_phase(&mut self) -> Vec<Plan> {
        (0..self.entities.len()).map(|i| self.set(i)).collect()
    }

    /// Move (spec §7.2) with the plans `set_phase` returned: upkeep, fees, transfers and
    /// movement. Returns which entities are dying; the plans' `live` flags are updated.
    pub fn move_phase(&mut self, plans: &mut [Plan]) -> Vec<bool> {
        let n = self.entities.len();
        let mut dying = vec![false; n];
        // ---- 2 Move ----
        // 2.1 Upkeep, before anything else.
        for (i, e) in self.entities.iter_mut().enumerate() {
            let cost = upkeep_cost(&self.upkeep, e);
            if e.store.amounts[self.upkeep.pay] as u64 >= cost {
                e.store.amounts[self.upkeep.pay] -= cost as Mu;
                add_elements(&mut self.absorbed, &self.chem, self.upkeep.becomes, cost as Mu);
                if let Some(a) = &mut self.accounts {
                    let g = a.group[i];
                    a.add(g, A_UPKEEP, cost);
                }
            } else {
                dying[i] = true;
                plans[i].intents.iter_mut().for_each(|x| x.live = false);
            }
        }
        // 2.2 Attempt fees, in genome order.
        for (i, (e, plan)) in self.entities.iter_mut().zip(plans.iter_mut()).enumerate() {
            for x in plan.intents.iter_mut().filter(|x| x.live && x.fee > 0) {
                if e.store.amounts[self.upkeep.pay] >= x.fee {
                    e.store.amounts[self.upkeep.pay] -= x.fee;
                    add_elements(&mut self.absorbed, &self.chem, self.upkeep.becomes, x.fee);
                    if let Some(a) = &mut self.accounts {
                        let g = a.group[i];
                        a.add(g, A_FEES, x.fee as u64);
                    }
                } else {
                    x.live = false;
                }
            }
        }
        // 2.3 Transfers: entity intents and transfer rules, arbitrated together.
        let mut requests = std::mem::take(&mut self.requests);
        requests.clear();
        let mut req_law = std::mem::take(&mut self.req_law);
        req_law.clear();
        let cur = std::cell::Cell::new(u32::MAX);
        let mut request = |origin: End, dest: End, s: SubstanceId, amount: Mu, overflow: Option<SinkId>, steal: bool| {
            if amount > 0 {
                let overflow = overflow.map_or(u16::MAX, |o| o as u16);
                req_law.push(cur.get());
                requests.push(Req { origin: pack(origin), dest: pack(dest), s: s as u16, overflow, requested: amount, steal });
            }
        };
        // Bit `s`: a live `guard(s)` intent, which drops every `steal` of `s` against the entity.
        let guards: Vec<u32> = plans
            .iter()
            .map(|p| p.intents.iter().filter(|x| x.live).fold(0, |m, x| if let Act::Guard { s } = x.act { m | 1 << s } else { m }))
            .collect();
        for (i, (e, plan)) in self.entities.iter().zip(plans.iter()).enumerate() {
            for x in plan.intents.iter().filter(|x| x.live) {
                match x.act {
                    // The thief's fee is kept.
                    Act::Steal { s, amount, other } if guards[other] & (1 << s) != 0 => self.guarded += amount as u64,
                    Act::Take { s, amount } => request(End::Cell(e.cell), End::Entity(i), s, amount, None, false),
                    Act::Drop { s, amount } => request(End::Entity(i), End::Cell(e.cell), s, amount, None, false),
                    // A dying entity takes part in no transfer, as origin or destination.
                    Act::Give { s, amount, other } if !dying[other] => request(End::Entity(i), End::Entity(other), s, amount, None, false),
                    Act::Steal { s, amount, other } if !dying[other] => request(End::Entity(other), End::Entity(i), s, amount, None, true),
                    Act::Give { .. } | Act::Steal { .. } | Act::Guard { .. } | Act::Move { .. } | Act::Turn { .. } | Act::Divide => {}
                }
            }
        }
        // A condition reads the cell as the tick begins, which is how the cell is now: nothing has
        // changed a cell's amounts yet (transfers are only requested here, and settled below).
        self.react_masks = vec![None; self.laws.len()];
        if self.law_stats.len() != self.laws.len() {
            self.law_stats = vec![LawStat::default(); self.laws.len()];
            self.law_since = self.tick;
        }
        for (li, law) in self.laws.iter().enumerate() {
            cur.set(li as u32);
            let (law, cond) = law.parts();
            if cond.is_some() && matches!(law, Law::React { .. }) {
                self.react_masks[li] = Some(vec![true; self.cells.len()]);
            }
            for c in 0..self.cells.len() {
                self.law_stats[li].cells += 1;
                if let Some(cond) = cond {
                    let (cells, space) = (&self.cells, self.space.as_ref());
                    let neighbour = |d: usize| -> Option<&[Mu]> {
                        let sp = space.filter(|sp| c < sp.grid_w * sp.grid_h)?;
                        crate::cond::neighbour(sp.grid_w, sp.grid_h, sp.wrap, c, d).map(|n| &cells[n].amounts[..])
                    };
                    let cell = &self.cells[c];
                    if !cond.holds(&crate::cond::Ctx { tick: self.tick, amounts: &cell.amounts[..], caps: &cell.caps[..], neighbour: &neighbour }) {
                        if let Some(m) = &mut self.react_masks[li] {
                            m[c] = false;
                        }
                        continue;
                    }
                }
                self.law_stats[li].passed += 1;
                match law {
                    Law::Inflow { s, source, amount, overflow } => {
                        request(End::Source(*source), End::Cell(c), *s, *amount, *overflow, false)
                    }
                    Law::Shift { s, dir, amount } => {
                        let to = self.space.as_ref().filter(|sp| c < sp.grid_w * sp.grid_h).and_then(|sp| crate::cond::neighbour(sp.grid_w, sp.grid_h, sp.wrap, c, *dir));
                        if let Some(nb) = to {
                            request(End::Cell(c), End::Cell(nb), *s, *amount, None, false);
                        }
                    }
                    Law::Diffuse { substances, rate_bp } => {
                        let nbs = &self.neighbours[c];
                        if nbs.is_empty() {
                            continue;
                        }
                        for &s in substances {
                            let offer = (self.cells[c].amounts[s] as u64 * *rate_bp as u64 / 10_000) as Mu;
                            let each = offer / nbs.len() as Mu;
                            for &nb in nbs {
                                request(End::Cell(c), End::Cell(nb), s, each, None, false);
                            }
                        }
                    }
                    Law::React { .. } => {}
                    Law::When { .. } => unreachable!("a `when` is never nested"),
                }
            }
        }
        self.req_law = req_law;
        self.arbitrate(&requests);
        self.requests = requests;
        // 2.4 Movement: paid turns, then paid moves. No repulsion in P0 (spec §7.2).
        if let Some(space) = &self.space {
            for (e, plan) in self.entities.iter_mut().zip(plans.iter()) {
                for x in plan.intents.iter().filter(|x| x.live) {
                    if let Act::Turn { angle_deg } = x.act {
                        if self.contract {
                            let (s, c) = crate::contract::sincos(angle_deg * crate::contract::DEG);
                            e.heading = crate::contract::rotate(e.heading, s, c);
                            continue;
                        }
                        let (s, c) = angle_deg.to_radians().sin_cos();
                        let [hx, hy] = e.heading;
                        let (nx, ny) = (hx * c - hy * s, hx * s + hy * c);
                        let len = (nx * nx + ny * ny).sqrt();
                        e.heading = [nx / len, ny / len];
                    }
                }
                for x in plan.intents.iter().filter(|x| x.live) {
                    if let Act::Move { distance } = x.act {
                        if self.contract {
                            e.pos = space.place_contract(crate::contract::advance(e.pos, e.heading, distance));
                            continue;
                        }
                        let p = [e.pos[0] + e.heading[0] * distance, e.pos[1] + e.heading[1] * distance];
                        e.pos = space.place(p);
                    }
                }
            }
        }

        dying
    }

    /// Move, Transform and Restructure (spec §7.2 to §7.5) with the plans `set_phase` returned.
    pub fn finish_tick(&mut self, plans: Vec<Plan>) {
        self.finish_tick_n(self.entities.len(), plans)
    }

    fn finish_tick_n(&mut self, _n: usize, mut plans: Vec<Plan>) {
        let dying = self.move_phase(&mut plans);
        self.after_move(plans, dying);
    }

    /// Transform and Restructure (spec §7.3, §7.5) after `move_phase`.
    pub fn after_move(&mut self, plans: Vec<Plan>, dying: Vec<bool>) {
        self.transform_phase(&plans, &dying);
        self.restructure_phase(plans, dying);
    }

    /// Transform (spec §7.3): entity reactions, world reactions and venting.
    pub fn transform_phase(&mut self, plans: &[Plan], dying: &[bool]) {
        // ---- 3 Transform ----
        let contract = self.contract;
        for (i, e) in self.entities.iter_mut().enumerate() {
            if dying[i] {
                continue;
            }
            for g in 0..e.genome.slots.len() {
                if !e.genome.active(g) {
                    continue;
                }
                if let Gene::React { reaction, .. } = e.genome.slots[g].gene {
                    let r = &self.chem.reactions[reaction];
                    let cap = match r.catalyst {
                        Some(k) => k.scale_in(contract, plans[i].react_caps[g], e.store.amounts[k.s]),
                        None => plans[i].react_caps[g],
                    };
                    let mut extent = cap;
                    for &(s, c) in &r.inputs {
                        extent = extent.min(e.store.amounts[s] / c);
                    }
                    for &(s, c) in &r.outputs {
                        extent = extent.min(e.store.headroom(s) / c);
                    }
                    if let Some(b) = &mut self.blocked {
                        let b = &mut b[reaction];
                        match r.inputs.iter().find(|&&(s, c)| e.store.amounts[s] < c) {
                            _ if extent > 0 && extent == cap => b.full += 1,
                            _ if extent > 0 => b.limited += 1,
                            _ if cap == 0 => b.cap_zero += 1,
                            Some(&(s, _)) => {
                                b.no_input.resize(self.chem.substances.len(), 0);
                                b.no_input[s] += 1;
                            }
                            None => b.no_room += 1,
                        }
                    }
                    apply_reaction(&mut e.store, r, extent);
                    self.entity_extent[reaction] += extent as u64;
                    if let Some(a) = &mut self.accounts {
                        let (g, pay, flux) = (a.group[i], self.upkeep.pay, a.flux);
                        for &(s, c) in &r.inputs {
                            let x = c as u64 * extent as u64;
                            if s == pay {
                                a.add(g, A_PAY_USED, x);
                            }
                            if Some(s) == flux {
                                a.add(g, A_FLUX_USED, x);
                            }
                        }
                        for &(s, c) in &r.outputs {
                            let x = c as u64 * extent as u64;
                            if s == pay {
                                a.add(g, A_PAY_MADE, x);
                            }
                            if Some(s) == flux {
                                a.add(g, A_FLUX_MADE, x);
                            }
                        }
                    }
                    if let Some(t) = self.track {
                        e.made += r.outputs.iter().filter(|o| o.0 == t).map(|o| o.1 as u64 * extent as u64).sum::<u64>();
                    }
                }
            }
        }
        for (li, law) in self.laws.iter().enumerate() {
            if let Law::React { reaction, rate_bp } = *law.parts().0 {
                let r = &self.chem.reactions[reaction];
                for (ci, cell) in self.cells.iter_mut().enumerate() {
                    if self.react_masks.get(li).and_then(|m| m.as_ref()).is_some_and(|m| !m[ci]) {
                        continue;
                    }
                    // At most the whole input: a catalysed rate above 10_000bp would take more than the cell holds.
                    let rate_bp = r.catalyst.map_or(rate_bp, |k| k.scale_in(contract, rate_bp, cell.amounts[k.s]).min(10_000));
                    let mut extent = Mu::MAX;
                    for &(s, c) in &r.inputs {
                        extent = extent.min((cell.amounts[s] as u64 * rate_bp as u64 / 10_000 / c as u64) as Mu);
                    }
                    for &(s, c) in &r.outputs {
                        extent = extent.min(cell.headroom(s) / c);
                    }
                    apply_reaction(cell, r, extent);
                    if extent > 0 {
                        self.law_stats[li].ran += 1;
                        self.law_stats[li].extent += extent as u64;
                    }
                }
            }
        }
        for (i, e) in self.entities.iter_mut().enumerate() {
            for &(s, sink) in &self.waste {
                debug_assert!(self.sinks[sink].accepts(s));
                let amount = std::mem::take(&mut e.store.amounts[s]);
                add_elements(&mut self.absorbed, &self.chem, s, amount);
                if let Some(a) = &mut self.accounts {
                    let g = a.group[i];
                    if s == self.upkeep.pay {
                        a.add(g, A_PAY_WASTED, amount as u64);
                    }
                    if Some(s) == a.flux {
                        a.add(g, A_FLUX_WASTED, amount as u64);
                    }
                }
            }
        }

    }

    /// Restructure (spec §7.5): disposal, division, ageing, newborns.
    pub fn restructure_phase(&mut self, plans: Vec<Plan>, dying: Vec<bool>) {
        let n = dying.len();
        // ---- 4 Restructure ----
        let tick = self.tick;
        // 4.1 Disposal, in entity order, before division.
        let mut removed = vec![false; n];
        for i in 0..n {
            let aged = self.entities[i].age + 1 >= self.max_age;
            if !(dying[i] || aged) {
                continue;
            }
            removed[i] = true;
            let cause = if dying[i] { Cause::Starved } else { Cause::Age };
            let cell = self.entities[i].cell;
            if let Some(a) = &mut self.accounts {
                let (g, held) = (a.group[i], &self.entities[i].store.amounts);
                a.add(g, A_PAY_DIED, held[self.upkeep.pay] as u64);
                if let Some(f) = a.flux {
                    a.add(g, A_FLUX_DIED, held[f] as u64);
                }
            }
            if let Some(r) = self.decompose {
                let r = &self.chem.reactions[r];
                let store = &mut self.entities[i].store;
                let extent = r.inputs.iter().map(|&(s, c)| store.amounts[s] / c).min().unwrap_or(0);
                apply_reaction(store, r, extent);
            }
            let (ns, lin) = (self.chem.substances.len(), self.entities[i].lineage);
            if self.disposal.to_cell.len() <= lin {
                self.disposal.to_cell.resize(lin + 1, vec![0; ns]);
                self.disposal.to_remains.resize(lin + 1, vec![0; ns]);
            }
            for s in 0..self.chem.substances.len() {
                let amount = std::mem::take(&mut self.entities[i].store.amounts[s]);
                let to_cell = amount.min(self.cells[cell].headroom(s));
                self.cells[cell].amounts[s] += to_cell;
                let rest = amount - to_cell;
                self.disposal.to_cell[lin][s] += to_cell as u64;
                self.disposal.to_remains[lin][s] += rest as u64;
                if rest > 0 {
                    assert!(self.sinks[self.remains].accepts(s), "remains must accept every substance");
                    add_elements(&mut self.absorbed, &self.chem, s, rest);
                }
            }
            self.deaths += 1;
            if self.record_genome_changes {
                // The age it would have reached this tick: `max_age` for a death of age.
                self.deaths_log.push((tick, self.entities[i].id, cause, self.entities[i].age as u32 + 1));
            }
            if let Some(a) = &mut self.accounts {
                let g = a.group[i];
                a.add(g, if cause == Cause::Starved { A_STARVED } else { A_AGED }, 1);
            }
            if self.record_events {
                self.events.push(Event::Died { tick, id: self.entities[i].id, cause });
            }
        }
        // 4.2 Division, on the inventory after Transform.
        let mut living = removed.iter().filter(|r| !**r).count();
        let mut children = Vec::new();
        for i in 0..n {
            if removed[i] {
                continue;
            }
            let e = &self.entities[i];
            let Some(x) = plans[i].intents.iter().find(|x| x.act == Act::Divide) else { continue };
            let Gene::Divide { split_bp, requires: (rs, ramount), .. } = e.genome.slots[x.gene].gene else {
                unreachable!()
            };
            let outcome = if !x.live {
                1
            } else if e.store.amounts[rs] < ramount {
                2
            } else if living >= self.max_entities {
                3
            } else {
                0
            };
            if let Some(b) = &mut self.binding {
                b.divide[outcome] += 1;
            }
            if outcome != 0 {
                continue;
            }
            let mut child = Store { amounts: [0; MAX_SUBSTANCES], caps: e.store.caps };
            let parent = &mut self.entities[i];
            for s in 0..child.amounts.len() {
                let share = (parent.store.amounts[s] as u64 * split_bp as u64 / 10_000) as Mu;
                parent.store.amounts[s] -= share;
                child.amounts[s] = share;
            }
            let id = self.next_id;
            self.next_id += 1;
            self.births += 1;
            if let Some(a) = &mut self.accounts {
                let g = a.group[i];
                a.add(g, A_BIRTHS, 1);
                if let Some(c) = &mut a.clade {
                    if c.contains(&parent.id) {
                        c.insert(id);
                    }
                }
            }
            if self.record_events {
                self.events.push(Event::Born { tick, parent: parent.id, child: id });
            }
            // The child is placed behind its parent's position after movement (spec §7.5).
            let (pos, cell) = match &self.space {
                Some(space) if self.contract => {
                    let behind = [-parent.heading[0], -parent.heading[1]];
                    let p = space.place_contract(crate::contract::advance(parent.pos, behind, space.child_offset));
                    (p, space.cell_at(p))
                }
                Some(space) => {
                    let p = [
                        parent.pos[0] - parent.heading[0] * space.child_offset,
                        parent.pos[1] - parent.heading[1] * space.child_offset,
                    ];
                    let p = space.place(p);
                    (p, space.cell_at(p))
                }
                None => (parent.pos, parent.cell),
            };
            let genome = match self.variation.get(parent.lineage) {
                Some(Some((rates, cap))) if self.mutation => {
                    let stream = philox::Stream {
                        seed: self.seed,
                        tick: tick as u32,
                        stream: id as u32,
                        domain: philox::DOMAIN_ENTITY,
                        aux: 0,
                        pass: philox::MUTATE,
                    };
                    crate::mutate::mutate(&parent.genome, rates, cap, &self.menus, &mut crate::mutate::Draws { contract: self.contract, ..crate::mutate::Draws::new(stream) })
                }
                _ => parent.genome.clone(),
            };
            let signals = vec![0.0; genome.keep.len()];
            let lineage = parent.lineage;
            let shape = if genome == parent.genome { parent.shape.clone() } else { GenomeShape::of(&genome) };
            if self.record_genome_changes {
                let changes = if genome == parent.genome { Vec::new() } else { crate::diff::diff(&parent.genome, &genome) };
                self.genome_changes.push((tick, parent.id, id, changes));
            }
            let newborn = Entity { id, cell, pos, heading: parent.heading, store: child, genome, signals, age: 0, lineage, made: 0, shape, prev: Vec::new() };
            if let Some(a) = &mut self.accounts {
                // The share the parent hands over leaves its group and enters the child's.
                let (from, to) = (a.group[i], a.group_of(&self.chem, &newborn));
                let kinds = [(Some(self.upkeep.pay), A_PAY_TO_CHILD, A_PAY_FROM_PARENT), (a.flux, A_FLUX_TO_CHILD, A_FLUX_FROM_PARENT)];
                for (s, out_of, into) in kinds {
                    if let Some(s) = s {
                        a.add(from, out_of, newborn.store.amounts[s] as u64);
                        a.add(to, into, newborn.store.amounts[s] as u64);
                    }
                }
            }
            children.push(newborn);
            living += 1;
        }
        // 4.3 Survivors age and move to the cell under them; newborns join at the end of the
        // tick and first act next tick.
        let mut i = 0;
        self.entities.retain(|_| {
            i += 1;
            !removed[i - 1]
        });
        for e in &mut self.entities {
            e.age += 1;
            if let Some(space) = &self.space {
                e.cell = space.cell_at(e.pos);
            }
        }
        self.entities.extend(children);
        self.tick += 1;
    }

    /// Set for one entity: senses read the snapshot, the mind runs `mind_steps` steps, and
    /// active genes form intents and reaction caps from the resulting signals.
    fn set(&mut self, i: usize) -> Plan {
        // Others in range, nearest first, if an active gene needs them.
        let mut near = Vec::new();
        let needs = |g: &Gene| matches!(g, Gene::Sense { sense: Sense::Neighbour { .. }, .. } | Gene::Give { .. } | Gene::Steal { .. });
        let e = &self.entities[i];
        if let Some(space) = &self.space {
            if (0..e.genome.slots.len()).any(|g| e.genome.active(g) && needs(&e.genome.slots[g].gene)) {
                self.near.within(space, &self.snap_ids, &self.snap_pos, i, space.radius, &mut near);
            }
        }
        let readings: Vec<[f32; 2]> = {
            e.genome.slots.iter().enumerate().map(|(g, slot)| match slot.gene {
                Gene::Sense { sense, .. } if e.genome.active(g) => {
                    let [a, b] = self.read(i, sense, &near);
                    if self.contract {
                        [crate::contract::sanitize(a, READING_LIMIT), crate::contract::sanitize(b, READING_LIMIT)]
                    } else {
                        [sanitize(a, READING_LIMIT), sanitize(b, READING_LIMIT)]
                    }
                }
                _ => [0.0, 0.0],
            }).collect()
        };
        let e = &mut self.entities[i];
        if e.shape.delta {
            e.prev.clear();
            e.prev.extend_from_slice(&e.store.amounts);
        }
        if self.contract {
            think_contract(&e.genome, &mut e.signals, &readings, self.mind_steps);
        } else {
            think(&e.genome, &mut e.signals, &readings, self.mind_steps);
        }

        let e = &self.entities[i];
        let cell = &self.cells[e.cell];
        let mut plan = Plan { intents: Vec::new(), react_caps: vec![0; e.genome.slots.len()] };
        let (mut moves, mut turns, mut divides) = (false, false, false);
        // Bit `s` of `fired[k]`: a `take`, `drop`, `give` or `steal` (k = 0..4) of `s` fired.
        let mut fired = [0u16; 4];
        let one = self.one_per_substance;
        let mut first = |k: usize, s: usize| {
            let new = !one || fired[k] & (1 << s) == 0;
            fired[k] |= 1 << s;
            new
        };
        for g in (0..e.genome.slots.len()).filter(|&g| e.genome.active(g)) {
            match e.genome.slots[g].gene {
                Gene::React { rate, speed, .. } => {
                    plan.react_caps[g] = scale(value(&e.signals, speed), rate);
                }
                Gene::Move { speed, fee_per_distance, src, threshold } => {
                    // Only the first firing move gene counts; the fee is on the requested distance.
                    let o = value(&e.signals, src);
                    if o > threshold && !moves {
                        moves = true;
                        let distance = o.clamp(0.0, 1.0) * speed;
                        let fee = (fee_per_distance as f32 * distance).ceil() as Mu;
                        plan.intents.push(Intent { gene: g, act: Act::Move { distance }, fee, live: true });
                    }
                }
                Gene::Turn { angle_deg, fee, src, threshold } => {
                    let o = value(&e.signals, src);
                    if o > threshold && !turns {
                        turns = true;
                        let act = Act::Turn { angle_deg: o.clamp(-1.0, 1.0) * angle_deg };
                        plan.intents.push(Intent { gene: g, act, fee, live: true });
                    }
                }
                Gene::Take { s, max, fee, src, threshold } | Gene::Drop { s, max, fee, src, threshold } => {
                    let o = value(&e.signals, src);
                    // No target: a cell that cannot hold the substance. Dropped, no fee.
                    let take = matches!(e.genome.slots[g].gene, Gene::Take { .. });
                    if o > threshold && cell.caps[s] > 0 && first(if take { 0 } else { 1 }, s) {
                        let amount = scale(o, max);
                        let act = if take {
                            Act::Take { s, amount }
                        } else {
                            Act::Drop { s, amount }
                        };
                        plan.intents.push(Intent { gene: g, act, fee, live: true });
                    }
                }
                Gene::Give { s, max, fee, src, threshold } | Gene::Steal { s, max, fee, src, threshold } => {
                    let o = value(&e.signals, src);
                    // No target: no other entity in range. Dropped, no fee.
                    let give = matches!(e.genome.slots[g].gene, Gene::Give { .. });
                    if let (true, Some(&(_, _, other))) = (o > threshold, near.first()) {
                        if !first(if give { 2 } else { 3 }, s) {
                            continue;
                        }
                        let amount = scale(o, max);
                        let act = if give {
                            Act::Give { s, amount, other }
                        } else {
                            Act::Steal { s, amount, other }
                        };
                        plan.intents.push(Intent { gene: g, act, fee, live: true });
                    }
                }
                Gene::Divide { fee, src, threshold, .. } => {
                    // Only the first firing divide gene counts; later ones pay nothing.
                    if value(&e.signals, src) > threshold && !divides {
                        divides = true;
                        plan.intents.push(Intent { gene: g, act: Act::Divide, fee, live: true });
                    }
                }
                Gene::Guard { s, fee, src, threshold } => {
                    // Needs no target: the fee is paid whether or not anyone steals.
                    if value(&e.signals, src) > threshold {
                        plan.intents.push(Intent { gene: g, act: Act::Guard { s }, fee, live: true });
                    }
                }
                Gene::Sense { .. } | Gene::Weigh { .. } | Gene::Product { .. } => {}
            }
        }
        plan
    }

    /// A sense reading from the snapshot. Amounts are in units: `f32(amount) × 0.001`.
    /// `near` lists the others in range, nearest first, if a gene needs them.
    fn read(&self, i: usize, sense: Sense, near: &[crate::near::Near]) -> [f32; 2] {
        let units = |amount: Mu| amount as f32 * 0.001;
        let e = &self.entities[i];
        match sense {
            Sense::Neighbour { s, rank } => [near.get(rank as usize).map_or(0.0, |&(_, _, j)| units(self.entities[j].store.amounts[s])), 0.0],
            Sense::Crowd { radius } => {
                let Some(space) = &self.space else { return [0.0, 0.0] };
                let mut found = Vec::new();
                self.near.within(space, &self.snap_ids, &self.snap_pos, i, radius, &mut found);
                [found.len() as f32, 0.0]
            }
            Sense::Probe { s, angle_deg, dist } => {
                let Some(space) = &self.space else { return [0.0, 0.0] };
                [units(self.cells[space.cell_ahead(e.pos, e.heading, angle_deg, dist, self.contract)].amounts[s]), 0.0]
            }
            Sense::Clock { period } => {
                if self.contract {
                    // No division of floats: the phase is an integer fraction of 2^24.
                    let p24 = (((self.tick % period as u64) << 24) / period as u64) as u32;
                    let phase = p24 as f32 * (1.0 / 16_777_216.0);
                    return [crate::contract::sincos(std::f32::consts::TAU * phase).0, 0.0];
                }
                let phase = (self.tick % period as u64) as f32 / period as f32;
                [(std::f32::consts::TAU * phase).sin(), 0.0]
            }
            Sense::Inventory(s) => [units(e.store.amounts[s]), 0.0],
            // The signed difference since the last reading, in both modes (decision 1, 2026-10-21).
            Sense::Delta(s) => [e.prev.get(s).map_or(0.0, |&p| crate::contract::amount_diff(e.store.amounts[s], p, 0.001)), 0.0],
            Sense::Cell(s) => [units(self.cells[e.cell].amounts[s]), 0.0],
            Sense::Gradient(s) => {
                let Some(space) = &self.space else { return [0.0, 0.0] };
                // Central differences over the neighbouring cells, per unit distance.
                let raw = |dx, dy| self.cells[space.offset(e.cell, dx, dy)].amounts[s];
                if self.contract {
                    use crate::contract::{amount_diff, fma};
                    let (kx, ky) = (0.001f32 * (space.grid_w as f32 / (2.0 * space.w)), 0.001f32 * (space.grid_h as f32 / (2.0 * space.h)));
                    let (gx, gy) = (amount_diff(raw(1, 0), raw(-1, 0), kx), amount_diff(raw(0, 1), raw(0, -1), ky));
                    let [hx, hy] = e.heading;
                    return [fma(gx, hx, gy * hy), fma(gy, hx, -(gx * hy))];
                }
                let at = |dx, dy| units(raw(dx, dy));
                let gx = (at(1, 0) - at(-1, 0)) * (space.grid_w as f32 / (2.0 * space.w));
                let gy = (at(0, 1) - at(0, -1)) * (space.grid_h as f32 / (2.0 * space.h));
                let [hx, hy] = e.heading;
                [gx * hx + gy * hy, -gx * hy + gy * hx]
            }
        }
    }

    /// Two-stage arbitration and commit (spec §7.2 step 3).
    ///
    /// Requests are summed per store and substance; only oversubscribed groups are shared
    /// pro-rata. The result is the same as arbitrating every group.
    fn arbitrate(&mut self, reqs: &[Req]) {
        let ns = self.chem.substances.len();
        let nc = self.cells.len();
        let key = |p: u32, s: u16| match p >> TAG_SHIFT {
            0 => Some((p & INDEX) as usize * ns + s as usize),
            1 => Some((nc + (p & INDEX) as usize) * ns + s as usize),
            _ => None,
        };
        let slots = (nc + self.entities.len()) * ns;
        let mut stage1 = std::mem::take(&mut self.stage1);
        stage1.clear();
        stage1.extend(reqs.iter().map(|r| r.requested));

        // Stage 1, per origin and substance, against the origin's balance.
        let mut sum = std::mem::take(&mut self.sums);
        sum.clear();
        sum.resize(slots, 0);
        for r in reqs {
            if let Some(k) = key(r.origin, r.s) {
                sum[k] += r.requested as u64;
            }
        }
        // Oversubscribed groups as (slot, request) pairs; sorting keeps each group's
        // requests in request order, which the tie draws are indexed by.
        let mut over = std::mem::take(&mut self.over);
        over.clear();
        for (i, r) in reqs.iter().enumerate() {
            if let Some(k) = key(r.origin, r.s) {
                if sum[k] > self.balance(r.origin, r.s) as u64 {
                    over.push((k, i as u32));
                }
            }
        }
        over.sort_unstable();
        for group in over.chunk_by(|a, b| a.0 == b.0) {
            let (origin, s) = (reqs[group[0].1 as usize].origin, reqs[group[0].1 as usize].s);
            let available = self.balance(origin, s);
            if let [(_, k)] = group {
                // One request over its origin's balance gets all of it; no remainder, no draw.
                stage1[*k as usize] = available;
                continue;
            }
            let wanted: Vec<u64> = group.iter().map(|&(_, k)| reqs[k as usize].requested as u64).collect();
            let draws = self.stream(unpack(origin), s as usize, philox::ARBITRATE_ORIGIN);
            let granted = pro_rata(&wanted, available as u64, |i| draws.draw(i as u64));
            for (&(_, k), g) in group.iter().zip(granted) {
                stage1[k as usize] = g as Mu;
            }
        }

        // Stage 2, per destination and substance, against headroom at the start of the step.
        let mut stage2 = std::mem::take(&mut self.stage2);
        stage2.clear();
        stage2.extend_from_slice(&stage1);
        sum.iter_mut().for_each(|x| *x = 0);
        for (r, &g) in reqs.iter().zip(&stage1) {
            if let Some(k) = key(r.dest, r.s) {
                sum[k] += g as u64;
            }
        }
        over.clear();
        for (i, r) in reqs.iter().enumerate() {
            if let Some(k) = key(r.dest, r.s) {
                if sum[k] > self.headroom(r.dest, r.s) as u64 {
                    over.push((k, i as u32));
                }
            }
        }
        over.sort_unstable();
        for group in over.chunk_by(|a, b| a.0 == b.0) {
            let (dest, s) = (reqs[group[0].1 as usize].dest, reqs[group[0].1 as usize].s);
            let available = self.headroom(dest, s);
            if let [(_, k)] = group {
                stage2[*k as usize] = available;
                continue;
            }
            let wanted: Vec<u64> = group.iter().map(|&(_, k)| stage1[k as usize] as u64).collect();
            let draws = self.stream(unpack(dest), s as usize, philox::ARBITRATE_DEST);
            let granted = pro_rata(&wanted, available as u64, |i| draws.draw(i as u64));
            for (&(_, k), g) in group.iter().zip(granted) {
                stage2[k as usize] = g as Mu;
            }
        }

        // Commit.
        self.last_transfers.clear();
        self.last_transfer_meta.clear();
        for (k, r) in reqs.iter().enumerate() {
            let s = r.s as usize;
            let delivered = stage2[k];
            let overflowed = match r.overflow {
                u16::MAX => 0,
                sink if self.sinks[sink as usize].accepts(s) => stage1[k] - stage2[k],
                _ => 0,
            };
            let debit = delivered + overflowed;
            match unpack(r.origin) {
                End::Source(_) => add_elements(&mut self.emitted, &self.chem, s, debit),
                o => self.store_mut(o).amounts[s] -= debit,
            }
            match unpack(r.dest) {
                End::Sink(_) => add_elements(&mut self.absorbed, &self.chem, s, delivered),
                d => self.store_mut(d).amounts[s] += delivered,
            }
            if overflowed > 0 {
                add_elements(&mut self.absorbed, &self.chem, s, overflowed);
            }
            if r.steal {
                self.stolen += delivered as u64;
            }
            if let (Some(a), true) = (&mut self.accounts, delivered > 0) {
                let (pay, flux, x) = (self.upkeep.pay, a.flux, delivered as u64);
                // The pay and flux columns are counted separately, so both count when they coincide.
                let columns = [
                    (s == pay).then_some((A_PAY_TAKEN, A_PAY_IN, A_PAY_OUT, A_PAY_GIFT_IN, A_PAY_GIFT_OUT, A_PAY_DROPPED)),
                    (Some(s) == flux).then_some((A_FLUX_TAKEN, A_FLUX_IN, A_FLUX_OUT, A_FLUX_GIFT_IN, A_FLUX_GIFT_OUT, A_FLUX_DROPPED)),
                ];
                for (taken, stolen_in, stolen_out, gift_in, gift_out, dropped) in columns.into_iter().flatten() {
                    match (unpack(r.origin), unpack(r.dest)) {
                        (End::Cell(_), End::Entity(d)) => a.add(a.group[d], taken, x),
                        (End::Entity(o), End::Entity(d)) => {
                            let (into, out_of) = if r.steal { (stolen_in, stolen_out) } else { (gift_in, gift_out) };
                            a.add(a.group[d], into, x);
                            a.add(a.group[o], out_of, x);
                        }
                        // Everything that leaves the entity counts, including what overflows to a sink.
                        (End::Entity(o), End::Cell(_) | End::Sink(_)) => a.add(a.group[o], dropped, debit as u64),
                        _ => {}
                    }
                }
            }
            if let Some(st) = self.req_law.get(k).and_then(|&l| self.law_stats.get_mut(l as usize)) {
                st.requests += 1;
                st.requested += r.requested as u64;
                st.delivered += delivered as u64;
                st.cut_by_origin += (stage1[k] < r.requested) as u64;
                st.cut_by_dest += (stage2[k] < stage1[k]) as u64;
            }
            if let Some(b) = &mut self.binding {
                let c = b.transfers.entry((transfer_kind(r), s)).or_default();
                c.requests += 1;
                c.by_origin += (stage1[k] < r.requested) as u64;
                c.by_dest += (stage2[k] < stage1[k]) as u64;
                c.requested += r.requested as u64;
                c.delivered += delivered as u64;
            }
            if self.record_transfers {
                let id = |p: u32| match unpack(p) { End::Entity(i) => Some(self.entities[i].id), _ => None };
                self.last_transfer_meta.push(TransferMeta { origin_id: id(r.origin), dest_id: id(r.dest), law: self.req_law.get(k).copied().filter(|&l| l != u32::MAX), steal: r.steal });
                self.last_transfers.push(Transfer {
                    origin: unpack(r.origin),
                    dest: unpack(r.dest),
                    s,
                    requested: r.requested,
                    overflow: (r.overflow != u16::MAX).then_some(r.overflow as usize),
                    stage1: stage1[k],
                    stage2: stage2[k],
                    delivered,
                    overflowed,
                });
            }
        }
        (self.sums, self.stage1, self.stage2, self.over) = (sum, stage1, stage2, over);
    }

    fn balance(&self, p: u32, s: u16) -> Mu {
        self.store(unpack(p)).map_or(0, |st| st.amounts[s as usize])
    }

    fn headroom(&self, p: u32, s: u16) -> Mu {
        self.store(unpack(p)).map_or(Mu::MAX, |st| st.headroom(s as usize))
    }

    /// The Philox stream for arbitrating one store's requests for one substance (spec §10.2).
    fn stream(&self, end: End, s: SubstanceId, pass: u32) -> Stream {
        let (stream, domain) = match end {
            End::Cell(c) => (c as u32, philox::DOMAIN_CELL),
            End::Entity(e) => (self.entities[e].id as u32, philox::DOMAIN_ENTITY),
            End::Source(_) | End::Sink(_) => unreachable!("not a store"),
        };
        Stream { seed: self.seed, tick: self.tick as u32, stream, domain, aux: s as u32, pass }
    }

    fn store(&self, end: End) -> Option<&Store> {
        match end {
            End::Cell(c) => Some(&self.cells[c]),
            End::Entity(e) => Some(&self.entities[e].store),
            End::Source(_) | End::Sink(_) => None,
        }
    }

    fn store_mut(&mut self, end: End) -> &mut Store {
        match end {
            End::Cell(c) => &mut self.cells[c],
            End::Entity(e) => &mut self.entities[e].store,
            End::Source(_) | End::Sink(_) => unreachable!("not a store"),
        }
    }
}

/// `floor(clamp(o, 0, 1) × max)` µ, computed in f32 as a GPU would (spec §5.2, §5.5).
fn scale(o: f32, max: Mu) -> Mu {
    (o.clamp(0.0, 1.0) * max as f32).floor() as Mu
}

/// Upkeep for one entity (spec §3.3), in u64.
fn upkeep_cost(u: &Upkeep, e: &Entity) -> u64 {
    let g = &e.genome;
    let in_use = &e.shape.in_use;
    let keep: u64 = in_use.iter().map(|&s| (u.per_signal_keep as f32 * g.keep[s]).ceil() as u64).sum();
    let carried: u64 = e.store.amounts.iter().map(|&a| a as u64).sum::<u64>() / 1_000;
    u.base as u64
        + u.per_gene as u64 * e.shape.active_genes as u64
        + u.per_signal as u64 * in_use.len() as u64
        + keep
        + u.per_carried_u as u64 * carried
}

fn apply_reaction(store: &mut Store, r: &crate::chem::Reaction, extent: Mu) {
    for &(s, c) in &r.inputs {
        store.amounts[s] -= extent * c;
    }
    for &(s, c) in &r.outputs {
        store.amounts[s] += extent * c;
    }
}

fn add_elements(ledger: &mut [u64], chem: &Chemistry, s: SubstanceId, amount: Mu) {
    for (e, &n) in chem.substances[s].comp.iter().enumerate() {
        ledger[e] += amount as u64 * n as u64;
    }
}

/// A state field: `w × h` cells (row-major, row 0 at the top), each one of `names`.
#[derive(Debug, Clone, PartialEq)]
pub struct StateField {
    pub name: String,
    pub names: Vec<String>,
    pub w: usize,
    pub h: usize,
    pub wrap: bool,
    pub cells: Vec<u8>,
    /// `life_like(born, survives)` on a two-state field, as bit masks over the number of live Moore
    /// neighbours (0 to 8): a dead cell is born if its bit is set in the first, a live one survives
    /// if its bit is set in the second.
    pub life_like: Option<(u16, u16)>,
}

impl StateField {
    pub fn new(name: &str, names: Vec<String>, w: usize, h: usize, wrap: bool) -> StateField {
        StateField { name: name.to_string(), names, w, h, wrap, cells: vec![0; w * h], life_like: None }
    }

    /// One update: every cell from the previous states, all committed together.
    pub fn step(&mut self) {
        let Some((born, survives)) = self.life_like else { return };
        let (w, h) = (self.w as i64, self.h as i64);
        let old = &self.cells;
        let alive = |x: i64, y: i64| -> u32 {
            let (x, y) = if self.wrap { (x.rem_euclid(w), y.rem_euclid(h)) } else { (x, y) };
            u32::from((0..w).contains(&x) && (0..h).contains(&y) && old[(y * w + x) as usize] == 1)
        };
        let next: Vec<u8> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let n = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)].iter().map(|(dx, dy)| alive(x + dx, y + dy)).sum::<u32>();
                let mask = if old[(y * w + x) as usize] == 1 { survives } else { born };
                u8::from(mask >> n & 1 == 1)
            })
            .collect();
        self.cells = next;
    }
}

/// The cells of a run-length-encoded pattern (`b` off, `o` on, `$` next row, `!` end, a count
/// before a tag repeats it), as `(x, y)` of the `o` cells from the top-left. `Err` names what is wrong.
pub fn rle_cells(rle: &str) -> Result<Vec<(usize, usize)>, String> {
    let (mut x, mut y, mut count, mut out) = (0usize, 0usize, 0usize, Vec::new());
    for c in rle.chars() {
        match c {
            '0'..='9' => count = count * 10 + c.to_digit(10).unwrap() as usize,
            'b' | 'o' | '$' => {
                let k = count.max(1);
                match c {
                    'b' => x += k,
                    'o' => {
                        out.extend((x..x + k).map(|i| (i, y)));
                        x += k;
                    }
                    _ => {
                        y += k;
                        x = 0;
                    }
                }
                count = 0;
            }
            '!' => return Ok(out),
            ' ' => {}
            other => return Err(format!("`{other}` is not part of a run-length pattern (b, o, $, !, digits)")),
        }
    }
    Err("a run-length pattern ends with `!`".into())
}

