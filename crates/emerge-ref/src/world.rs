//! A world of cells and entities, run on the four-phase schedule
//! (`core.md` §6.1, spec §7).

use std::collections::BTreeMap;

use crate::arbitrate::pro_rata;
use crate::chem::{ChemError, Chemistry, Mu, ReactionId, SubstanceId};
use crate::mind::{sanitize, think, value, Gene, Genome, Sense, READING_LIMIT};
use crate::philox::{self, Stream};

pub type SourceId = usize;
pub type SinkId = usize;

/// Conserved amounts and caps, one slot per substance. A cap of `0` means the store
/// cannot hold that substance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    pub amounts: Vec<Mu>,
    pub caps: Vec<Mu>,
}

impl Store {
    pub fn new(caps: Vec<Mu>) -> Self {
        Store { amounts: vec![0; caps.len()], caps }
    }

    pub fn headroom(&self, s: SubstanceId) -> Mu {
        self.caps[s] - self.amounts[s]
    }
}

/// An action gene's intent, formed in Set (spec §5.5, §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Take { s: SubstanceId, amount: Mu },
    Drop { s: SubstanceId, amount: Mu },
    Divide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub cell: usize,
    pub store: Store,
    pub genome: Genome,
    pub signals: Vec<f32>,
    /// Ticks completed.
    pub age: u64,
}

/// World rules. Each runs on every cell; its kind sets its phase (spec §7.4).
#[derive(Debug, Clone)]
pub enum Law {
    /// Transfer rule: `transfer(s, from: source, to: self, amount, overflow)`. Move.
    Inflow { s: SubstanceId, source: SourceId, amount: Mu, overflow: Option<SinkId> },
    /// Transfer rule: `diffuse_conserved(substances, rate)`. Move.
    Diffuse { substances: Vec<SubstanceId>, rate_bp: u32 },
    /// Reaction rule: `react(reaction, rate)`. Transform.
    React { reaction: ReactionId, rate_bp: u32 },
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerError {
    pub tick: u64,
    pub element: String,
    /// Σ stores + absorbed − emitted, which must be zero.
    pub drift: i128,
}

#[derive(Debug, Clone)]
pub struct World {
    pub chem: Chemistry,
    pub cells: Vec<Store>,
    pub neighbours: Vec<Vec<usize>>,
    pub entities: Vec<Entity>,
    pub sources: Vec<String>,
    pub sinks: Vec<Sink>,
    pub laws: Vec<Law>,
    pub upkeep: Upkeep,
    pub waste: Vec<(SubstanceId, SinkId)>,
    pub remains: SinkId,
    pub max_age: u64,
    pub max_entities: usize,
    pub mind_steps: u32,
    pub inventory_cap: Mu,
    pub seed: u64,
    pub tick: u64,
    pub next_id: u64,
    /// Cumulative µ per element emitted by sources and absorbed by sinks.
    pub emitted: Vec<u64>,
    pub absorbed: Vec<u64>,
    pub events: Vec<Event>,
    /// The transfers of the last tick, in request order.
    pub last_transfers: Vec<Transfer>,
}

impl World {
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
        let n = chem.elements.len();
        Ok(World {
            chem,
            cells: Vec::new(),
            neighbours: Vec::new(),
            entities: Vec::new(),
            sources: Vec::new(),
            sinks,
            laws: Vec::new(),
            upkeep,
            waste: Vec::new(),
            remains,
            max_age: u64::MAX,
            max_entities: usize::MAX,
            mind_steps: 1,
            inventory_cap: 100 * crate::chem::U,
            seed: 0,
            tick: 0,
            next_id: 1,
            emitted: vec![0; n],
            absorbed: vec![0; n],
            events: Vec::new(),
            last_transfers: Vec::new(),
        })
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

    /// Adds an entity with an empty inventory and one module of expressed genes.
    pub fn spawn(&mut self, cell: usize, genes: Vec<Gene>) -> usize {
        self.spawn_genome(cell, Genome::simple(genes))
    }

    /// Adds an entity with an empty inventory. Its signals start at `0.0`.
    pub fn spawn_genome(&mut self, cell: usize, genome: Genome) -> usize {
        let caps = vec![self.inventory_cap; self.chem.substances.len()];
        let signals = vec![0.0; genome.keep.len()];
        self.entities.push(Entity { id: self.next_id, cell, store: Store::new(caps), genome, signals, age: 0 });
        self.next_id += 1;
        self.entities.len() - 1
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
                for (s, &amount) in store.amounts.iter().enumerate() {
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
        let n = self.entities.len();

        // ---- 1 Set ----
        // Senses read the snapshot, minds run, action genes become intents (spec §7.1).
        let mut plans: Vec<Plan> = (0..n).map(|i| self.set(i)).collect();
        let mut dying = vec![false; n];

        // ---- 2 Move ----
        // 2.1 Upkeep, before anything else.
        for (i, e) in self.entities.iter_mut().enumerate() {
            let cost = upkeep_cost(&self.upkeep, e);
            if e.store.amounts[self.upkeep.pay] as u64 >= cost {
                e.store.amounts[self.upkeep.pay] -= cost as Mu;
                add_elements(&mut self.absorbed, &self.chem, self.upkeep.becomes, cost as Mu);
            } else {
                dying[i] = true;
                plans[i].intents.iter_mut().for_each(|x| x.live = false);
            }
        }
        // 2.2 Attempt fees, in genome order.
        for (e, plan) in self.entities.iter_mut().zip(&mut plans) {
            for x in plan.intents.iter_mut().filter(|x| x.live && x.fee > 0) {
                if e.store.amounts[self.upkeep.pay] >= x.fee {
                    e.store.amounts[self.upkeep.pay] -= x.fee;
                    add_elements(&mut self.absorbed, &self.chem, self.upkeep.becomes, x.fee);
                } else {
                    x.live = false;
                }
            }
        }
        // 2.3 Transfers: entity intents and transfer rules, arbitrated together.
        let mut requests: Vec<Transfer> = Vec::new();
        let mut request = |origin, dest, s, amount, overflow| {
            if amount > 0 {
                requests.push(Transfer {
                    origin, dest, s, requested: amount, overflow,
                    stage1: 0, stage2: 0, delivered: 0, overflowed: 0,
                });
            }
        };
        for (i, (e, plan)) in self.entities.iter().zip(&plans).enumerate() {
            for x in plan.intents.iter().filter(|x| x.live) {
                match x.act {
                    Act::Take { s, amount } => request(End::Cell(e.cell), End::Entity(i), s, amount, None),
                    Act::Drop { s, amount } => request(End::Entity(i), End::Cell(e.cell), s, amount, None),
                    Act::Divide => {}
                }
            }
        }
        for law in &self.laws {
            for c in 0..self.cells.len() {
                match law {
                    Law::Inflow { s, source, amount, overflow } => {
                        request(End::Source(*source), End::Cell(c), *s, *amount, *overflow)
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
                                request(End::Cell(c), End::Cell(nb), s, each, None);
                            }
                        }
                    }
                    Law::React { .. } => {}
                }
            }
        }
        self.arbitrate(&mut requests);
        self.last_transfers = requests;
        // 2.4 Movement: not in this slice.

        // ---- 3 Transform ----
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
                    let mut extent = plans[i].react_caps[g];
                    for &(s, c) in &r.inputs {
                        extent = extent.min(e.store.amounts[s] / c);
                    }
                    for &(s, c) in &r.outputs {
                        extent = extent.min(e.store.headroom(s) / c);
                    }
                    apply_reaction(&mut e.store, r, extent);
                }
            }
        }
        for law in &self.laws {
            if let Law::React { reaction, rate_bp } = *law {
                let r = &self.chem.reactions[reaction];
                for cell in &mut self.cells {
                    let mut extent = Mu::MAX;
                    for &(s, c) in &r.inputs {
                        extent = extent.min((cell.amounts[s] as u64 * rate_bp as u64 / 10_000 / c as u64) as Mu);
                    }
                    for &(s, c) in &r.outputs {
                        extent = extent.min(cell.headroom(s) / c);
                    }
                    apply_reaction(cell, r, extent);
                }
            }
        }
        for e in &mut self.entities {
            for &(s, sink) in &self.waste {
                debug_assert!(self.sinks[sink].accepts(s));
                let amount = std::mem::take(&mut e.store.amounts[s]);
                add_elements(&mut self.absorbed, &self.chem, s, amount);
            }
        }

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
            for s in 0..self.chem.substances.len() {
                let amount = std::mem::take(&mut self.entities[i].store.amounts[s]);
                let to_cell = amount.min(self.cells[cell].headroom(s));
                self.cells[cell].amounts[s] += to_cell;
                let rest = amount - to_cell;
                if rest > 0 {
                    assert!(self.sinks[self.remains].accepts(s), "remains must accept every substance");
                    add_elements(&mut self.absorbed, &self.chem, s, rest);
                }
            }
            self.events.push(Event::Died { tick, id: self.entities[i].id, cause });
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
            if !x.live || e.store.amounts[rs] < ramount || living >= self.max_entities {
                continue;
            }
            let mut child = Store::new(e.store.caps.clone());
            let parent = &mut self.entities[i];
            for s in 0..child.amounts.len() {
                let share = (parent.store.amounts[s] as u64 * split_bp as u64 / 10_000) as Mu;
                parent.store.amounts[s] -= share;
                child.amounts[s] = share;
            }
            let id = self.next_id;
            self.next_id += 1;
            self.events.push(Event::Born { tick, parent: parent.id, child: id });
            let genome = parent.genome.clone();
            let signals = vec![0.0; genome.keep.len()];
            children.push(Entity { id, cell: parent.cell, store: child, genome, signals, age: 0 });
            living += 1;
        }
        // 4.3 Survivors age; newborns join at the end of the tick and first act next tick.
        let mut i = 0;
        self.entities.retain(|_| {
            i += 1;
            !removed[i - 1]
        });
        self.entities.iter_mut().for_each(|e| e.age += 1);
        self.entities.extend(children);

        self.tick += 1;
        self.audit()
    }

    /// Set for one entity: senses read the snapshot, the mind runs `mind_steps` steps, and
    /// active genes form intents and reaction caps from the resulting signals.
    fn set(&mut self, i: usize) -> Plan {
        let e = &self.entities[i];
        let cell = &self.cells[e.cell];
        // Readings are in units: amount × 0.001, as f32.
        let readings: Vec<f32> = e.genome.slots.iter().map(|slot| match slot.gene {
            Gene::Sense { sense: Sense::Inventory(s), .. } => e.store.amounts[s] as f32 * 0.001,
            Gene::Sense { sense: Sense::Cell(s), .. } => cell.amounts[s] as f32 * 0.001,
            _ => 0.0,
        }).map(|r| sanitize(r, READING_LIMIT)).collect();
        let e = &mut self.entities[i];
        think(&e.genome, &mut e.signals, &readings, self.mind_steps);

        let e = &self.entities[i];
        let cell = &self.cells[e.cell];
        let mut plan = Plan { intents: Vec::new(), react_caps: vec![0; e.genome.slots.len()] };
        let mut divides = false;
        for g in (0..e.genome.slots.len()).filter(|&g| e.genome.active(g)) {
            match e.genome.slots[g].gene {
                Gene::React { rate, speed, .. } => {
                    plan.react_caps[g] = scale(value(&e.signals, speed), rate);
                }
                Gene::Take { s, max, fee, src, threshold } | Gene::Drop { s, max, fee, src, threshold } => {
                    let o = value(&e.signals, src);
                    // No target: a cell that cannot hold the substance. Dropped, no fee.
                    if o > threshold && cell.caps[s] > 0 {
                        let amount = scale(o, max);
                        let act = if matches!(e.genome.slots[g].gene, Gene::Take { .. }) {
                            Act::Take { s, amount }
                        } else {
                            Act::Drop { s, amount }
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
                Gene::Sense { .. } | Gene::Weigh { .. } | Gene::Product { .. } => {}
            }
        }
        plan
    }

    /// Two-stage arbitration and commit (spec §7.2 step 3).
    fn arbitrate(&mut self, reqs: &mut [Transfer]) {
        // Stage 1, per origin and substance, against the origin's balance.
        let mut by_origin: BTreeMap<(End, SubstanceId), Vec<usize>> = BTreeMap::new();
        for (k, r) in reqs.iter().enumerate() {
            by_origin.entry((r.origin, r.s)).or_default().push(k);
        }
        for ((origin, s), ks) in &by_origin {
            let wanted: Vec<u64> = ks.iter().map(|&k| reqs[k].requested as u64).collect();
            let granted = match self.store(*origin) {
                Some(store) => {
                    let draws = self.stream(*origin, *s, philox::ARBITRATE_ORIGIN);
                    pro_rata(&wanted, store.amounts[*s] as u64, |i| draws.draw(i as u64))
                }
                None => wanted, // a source has no balance
            };
            for (&k, g) in ks.iter().zip(granted) {
                reqs[k].stage1 = g as Mu;
            }
        }
        // Stage 2, per destination and substance, against headroom at the start of the step.
        let mut by_dest: BTreeMap<(End, SubstanceId), Vec<usize>> = BTreeMap::new();
        for (k, r) in reqs.iter().enumerate() {
            by_dest.entry((r.dest, r.s)).or_default().push(k);
        }
        for ((dest, s), ks) in &by_dest {
            let wanted: Vec<u64> = ks.iter().map(|&k| reqs[k].stage1 as u64).collect();
            let granted = match self.store(*dest) {
                Some(store) => {
                    let draws = self.stream(*dest, *s, philox::ARBITRATE_DEST);
                    pro_rata(&wanted, store.headroom(*s) as u64, |i| draws.draw(i as u64))
                }
                None => wanted, // a sink has no cap
            };
            for (&k, g) in ks.iter().zip(granted) {
                reqs[k].stage2 = g as Mu;
            }
        }
        // Commit.
        for r in reqs.iter_mut() {
            r.delivered = r.stage2;
            r.overflowed = match r.overflow {
                Some(sink) if self.sinks[sink].accepts(r.s) => r.stage1 - r.stage2,
                _ => 0,
            };
            let debit = r.delivered + r.overflowed;
            match r.origin {
                End::Source(_) => add_elements(&mut self.emitted, &self.chem, r.s, debit),
                o => self.store_mut(o).amounts[r.s] -= debit,
            }
            match r.dest {
                End::Sink(_) => add_elements(&mut self.absorbed, &self.chem, r.s, r.delivered),
                d => self.store_mut(d).amounts[r.s] += r.delivered,
            }
            add_elements(&mut self.absorbed, &self.chem, r.s, r.overflowed);
        }
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
    let in_use = g.signals_in_use();
    let keep: u64 = in_use.iter().map(|&s| (u.per_signal_keep as f32 * g.keep[s]).ceil() as u64).sum();
    let carried: u64 = e.store.amounts.iter().map(|&a| a as u64).sum::<u64>() / 1_000;
    u.base as u64
        + u.per_gene as u64 * g.active_genes() as u64
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
