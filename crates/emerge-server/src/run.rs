//! A run: one world, on the CPU runner or on the GPU, with its source, revision and journal
//! (spec §13, `proposals/engine-api.md`). Everything that changes a run goes through here, so
//! the journal sees every swap, committed or rejected.

use crate::journal::{fnv, Journal};
use emerge_gpu::gpu::Gpu;
use emerge_gpu::resident::Resident;
use emerge_ref::compile::Options;
use emerge_ref::swap::{Live, Rejected};
use serde_json::{json, Value};

/// What a run was made from: the journal header holds exactly this.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub source: String,
    pub lineages: Vec<String>,
    pub seed: u64,
    pub mutation: bool,
    /// Contract mode (`--contract gpu`); always on for the GPU.
    pub contract: bool,
    pub gpu: bool,
}

enum Backend {
    Cpu,
    /// The device is leaked so the world can borrow it for the rest of the process (a few runs).
    Gpu(Box<Resident<'static>>),
}

pub struct Run {
    pub spec: Spec,
    pub live: Live,
    backend: Backend,
    pub journal: Journal,
    /// The backend, and the device for the GPU.
    pub device: String,
    /// Pause and compile time of the last committed swap, in seconds.
    pub last_swap: Option<(f64, f64)>,
    /// The ledger audit after every tick, if on (CPU: ticks checked and ticks off; the GPU counts its own).
    audit: Option<(u64, u64)>,
    /// The window `made` counts over, once `count_made` started one.
    made: Option<emerge_view::frame::Made>,
    /// What charts draw: sampled every tick on the CPU, every 10 on the GPU (counting lineages
    /// there needs a read-back), thinned past `emerge_view::frame::MAX_SAMPLES`.
    pub history: emerge_view::frame::History,
    /// Births and deaths at the last sample.
    sampled: (u64, u64),
    /// Where the resource budget window began (`accounts.start`).
    acct: Option<AcctStart>,
    /// The tick the GPU's per-statement counters began (they restart when a swap changes the number of statements).
    law_since: u64,
    /// A bounded recording of transfer requests (`trace.start`).
    trace: Option<Trace>,
    /// This run's incarnation (`proposals/agent-tracking.md` §13): unique to the server process and the
    /// world, never reused, so a reference to a creature names its run and not a `w1` handle that a new
    /// server hands out again. Set by the engine.
    pub run_id: String,
    /// Watches, trackers and their recordings (`crate::track`): observation only, never part of the source.
    pub track: crate::track::Tracking,
}

/// One creature as an exact lookup reads it (`entities.get`).
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRow {
    pub id: u64,
    pub lineage: usize,
    pub age: u64,
    pub pos: [f32; 2],
    pub heading: [f32; 2],
    pub active_genes: usize,
    pub carried: Vec<u32>,
}

#[derive(Clone)]
/// One recorded transfer request: the tick, who asked (a rule's statement, or a creature), the two ends, and what happened.
struct TraceRec {
    tick: u64,
    law: Option<u32>,
    origin: String,
    dest: String,
    steal: bool,
    substance: usize,
    requested: u32,
    delivered: u32,
    overflowed: u32,
}

/// A recording window: the ticks it covers, what it keeps, and how much it had to drop.
struct Trace {
    from: u64,
    until: u64,
    cap: usize,
    /// `Some`: only these laws' requests; and `creatures` whether creature actions are kept.
    laws: Option<Vec<u32>>,
    creatures: bool,
    records: Vec<TraceRec>,
    seen: u64,
    dropped: u64,
    last_tick: u64,
    /// Who started it (`trace.start` names itself, or a tracker): the one slot is not taken over silently.
    owner: String,
    /// Only requests with one of these creatures at either end (a tracker's recording on the CPU; the device keeps every
    /// creature's requests and they are filtered when read).
    ids: Option<std::collections::HashSet<u64>>,
}

/// The start of a budget window: tick, revision, and each lineage's creature count and stock of the
/// upkeep substance (µ) then.
struct AcctStart {
    tick: u64,
    revision: u64,
    /// The groups: founding lineages, or `everyone else` and one clade.
    names: Vec<String>,
    count: Vec<u64>,
    stock: Vec<u64>,
    /// What the dead's inventory had gone to by the start, per lineage: (cells, remains) of the upkeep substance (µ).
    disposal0: Vec<(u64, u64)>,
}

/// Records the GPU keeps of births and deaths (24 bytes each, 96 MB of device memory); past that it counts what it dropped.
const GPU_EVENT_LOG: usize = 4 << 20;

/// A living creature as the family and life-history queries need it.
struct Living { id: u64, lineage: usize, age: u64, stock: u64 }

impl Run {
    /// Every living creature, from the CPU world or the GPU's entity table.
    fn living(&self) -> Vec<Living> {
        match &self.backend {
            Backend::Cpu => {
                let ns = self.live.world.chem.substances.len();
                self.live.world.entities.iter().map(|e| Living { id: e.id, lineage: e.lineage, age: e.age, stock: e.store.amounts[..ns].iter().map(|&a| u64::from(a)).sum() }).collect()
            }
            Backend::Gpu(r) => {
                let ns = r.ns();
                let (ent, _, amounts) = r.entity_table();
                ent.chunks(emerge_gpu::pack::ENT).zip(amounts.chunks(ns))
                    .map(|(e, a)| Living { id: u64::from(e[0]) | (u64::from(e[1]) << 32), lineage: e[11] as usize, age: u64::from(e[10]), stock: a.iter().map(|&x| u64::from(x)).sum() })
                    .collect()
            }
        }
    }

    /// Birth and death events since tick 0 (the GPU: since the run started), and how many the GPU's log dropped.
    fn event_list(&self) -> Result<(std::borrow::Cow<'_, [emerge_ref::world::Event]>, u64), String> {
        match &self.backend {
            Backend::Cpu => Ok((std::borrow::Cow::Borrowed(&self.live.world.events[..]), 0)),
            Backend::Gpu(r) => r.events().map(|(v, dropped)| (std::borrow::Cow::Owned(v), dropped)).ok_or_else(|| "the GPU run keeps no events".to_string()),
        }
    }

    /// Compiles the source (in canonical form) and starts at tick 0, revision 0. The journal's
    /// header is written at once.
    pub fn create(mut spec: Spec, mut journal: Journal) -> Result<Run, Rejected> {
        spec.contract |= spec.gpu;
        let names: Vec<&str> = spec.lineages.iter().map(String::as_str).collect();
        let mut live = Live::new(&spec.source, &names, Options { seed: spec.seed, allow_partial: true, mutation: spec.mutation })?;
        spec.source = live.source.clone();
        live.world.contract = spec.contract;
        if spec.gpu && !live.world.states.is_empty() {
            return Err(Rejected::Invalid { code: "E1001".into(), path: "fields".into(), message: "state fields run on the CPU only; the GPU runtime would leave them out".into() });
        }
        if spec.gpu {
            if let Some(why) = live.world.gpu_unsupported() {
                return Err(Rejected::Invalid { code: "E1001".into(), path: "rules".into(), message: why });
            }
        }
        let (backend, device) = if spec.gpu {
            let gpu: &'static Gpu = Box::leak(Box::new(Gpu::new()));
            let mut r = Resident::new(gpu, &mut live.world);
            r.enable_events(GPU_EVENT_LOG);
            r.enable_law_stats(&live.world);
            (Backend::Gpu(Box::new(r)), format!("gpu: {}", gpu.name))
        } else {
            (Backend::Cpu, "cpu: emerge-ref".to_string())
        };
        journal.header(&spec, &device);
        let every = if spec.gpu { 10 } else { 1 };
        let mut run = Run { spec, live, backend, journal, device, last_swap: None, audit: None, made: None, history: emerge_view::frame::History::new(every), sampled: (0, 0), acct: None, law_since: 0, trace: None, run_id: String::new(), track: Default::default() };
        run.sampled = (run.births(), run.deaths());
        run.sample();
        run.track.founders_below = run.next_id();
        Ok(run)
    }

    pub fn tick(&self) -> u64 {
        match &self.backend {
            Backend::Cpu => self.live.world.tick,
            Backend::Gpu(r) => r.tick as u64,
        }
    }

    pub fn revision(&self) -> u64 {
        self.live.revision
    }

    pub fn population(&self) -> usize {
        match &self.backend {
            Backend::Cpu => self.live.world.entities.len(),
            Backend::Gpu(r) => r.n,
        }
    }

    /// Whether the world can still change: it has entities, or a state field (an automaton has
    /// no entities; `proposals/field-rules.md`).
    pub fn alive(&self) -> bool {
        self.population() > 0 || !self.live.world.states.is_empty()
    }

    /// Runs `n` ticks, or fewer if the world dies out. Returns the ticks run.
    pub fn step(&mut self, n: u64) -> u64 {
        let mut done = 0;
        while done < n && self.alive() {
            match &mut self.backend {
                Backend::Cpu => {
                    self.live.world.run_tick();
                    self.capture_trace();
                    if let Some((ticks, off)) = &mut self.audit {
                        *ticks += 1;
                        *off += u64::from(self.live.world.audit().is_err());
                    }
                }
                Backend::Gpu(r) => {
                    r.tick();
                    self.capture_trace();
                }
            }
            done += 1;
            if self.tick().is_multiple_of(self.history.every) {
                self.sample();
            }
            if self.track.due(self.tick()) {
                let mut t = std::mem::take(&mut self.track);
                t.observe(self);
                self.track = t;
            }
        }
        done
    }

    /// Adds the current state to the history: population by lineage, births and deaths since the
    /// last sample, and the ledger check (the CPU audits every sample; the GPU reports its audit
    /// if it is on, else nothing).
    fn sample(&mut self) {
        let lineages = self.spec.lineages.len();
        let (by_lineage, drift) = match &self.backend {
            Backend::Cpu => {
                let w = &self.live.world;
                let mut by = vec![0u64; lineages];
                for e in &w.entities {
                    if let Some(c) = by.get_mut(e.lineage) {
                        *c += 1;
                    }
                }
                (by, Some(w.audit().map_or_else(|e| e.drift, |_| 0)))
            }
            Backend::Gpu(r) => (r.lineage_counts(lineages), r.audit().map(|a| if a.drifted == 0 { 0 } else { a.worst })),
        };
        let (b, d) = (self.births(), self.deaths());
        self.history.push(emerge_view::frame::Sample { tick: self.tick(), revision: self.revision(), by_lineage, births: b - self.sampled.0, deaths: d - self.sampled.1, drift });
        self.sampled = (b, d);
    }

    /// A hot-swap at the current tick boundary, journalled whatever its outcome.
    /// Whether `patch` would be accepted at `base`: checked, compiled and compared with the running world as `hotswap` does
    /// before it commits, changing nothing. The balances are not looked at (a lowered cap can still fail when applied), and
    /// on a GPU world the device may refuse more when it is applied.
    pub fn check_patch(&self, patch: &str, base: u64) -> Result<(), Rejected> {
        self.live.stage(patch, base).map(|_| ())
    }

    pub fn hotswap(&mut self, patch: &str, base: u64, reason: &str) -> Result<Value, Rejected> {
        let tick = self.tick();
        let result = match &mut self.backend {
            Backend::Cpu => {
                let staged = self.live.stage(patch, base);
                staged.and_then(|s| {
                    let compile = s.compile_s;
                    self.live.commit(s).map(|pause| (pause, compile))
                })
            }
            Backend::Gpu(r) => emerge_gpu::swap::hotswap(&mut self.live, r, patch, base),
        };
        match result {
            Ok((pause, compile)) => {
                self.last_swap = Some((pause, compile));
                if let Backend::Gpu(r) = &mut self.backend {
                    if r.law_stats().is_some_and(|v| v.len() != self.live.world.laws.len()) {
                        r.enable_law_stats(&self.live.world);
                        self.law_since = u64::from(r.tick);
                    }
                }
                let hash = fnv(&self.live.source);
                self.journal.swap(tick, base, patch, reason, Ok(hash));
                Ok(json!({"revision": self.live.revision, "tick": tick, "pause_ms": pause * 1e3, "compile_ms": compile * 1e3, "source_hash": format!("{hash:016x}")}))
            }
            Err(e) => {
                self.journal.swap(tick, base, patch, reason, Err(&e));
                Err(e)
            }
        }
    }

    /// Checks the element ledger after every tick from now on (spec §15.2): totals equal the start
    /// plus what sources emitted minus what sinks absorbed.
    pub fn enable_audit(&mut self) {
        match &mut self.backend {
            Backend::Cpu => self.audit = Some((0, 0)),
            Backend::Gpu(r) => r.enable_audit(),
        }
    }

    /// Ticks audited, and ticks whose element totals were off.
    pub fn audit(&self) -> Option<(u64, u64)> {
        match &self.backend {
            Backend::Cpu => self.audit,
            Backend::Gpu(r) => r.audit().map(|a| (a.ticks, a.drifted)),
        }
    }

    pub fn births(&self) -> u64 {
        match &self.backend {
            Backend::Cpu => self.live.world.births,
            Backend::Gpu(r) => r.births,
        }
    }

    pub fn deaths(&self) -> u64 {
        match &self.backend {
            Backend::Cpu => self.live.world.deaths,
            Backend::Gpu(r) => r.deaths,
        }
    }

    /// A hash of the whole state, to check that a replay reached the same world.
    pub fn digest(&self) -> u64 {
        match &self.backend {
            Backend::Cpu => fnv(&format!("{:?}", self.live.world)),
            Backend::Gpu(r) => {
                use emerge_gpu::pack::{ENT, MASTER_HDR, SLOT};
                let s = r.snapshot();
                let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
                // Genomes by content: where one sits in the pools depends on the order in which
                // children were given room, which can differ between two runs of the same world.
                let genomes: Vec<u32> = s
                    .ent
                    .chunks(ENT)
                    .flat_map(|e| {
                        let (g, m) = (e[4] as usize, e[12] as usize);
                        let view = &s.gw[g..g + e[6] as usize + e[5] as usize * SLOT];
                        let master = &s.mw[m..m + MASTER_HDR + s.mw[m + 2] as usize + s.mw[m] as usize * SLOT];
                        e.iter().enumerate().filter(|(k, _)| *k != 4 && *k != 12).map(|(_, &w)| w).chain(view.iter().copied()).chain(master.iter().copied()).collect::<Vec<_>>()
                    })
                    .collect();
                fnv(&format!("{} {} {:?} {:?} {:?} {:?} {:?} {:?} {} {} {:?}", s.n, s.next_id, genomes, bits(&s.epos), s.amounts, s.ecap, bits(&s.sig), s.camt, r.births, r.deaths, r.extent()))
            }
        }
    }

    /// Ends the run in the journal, with the tick and the state's digest.
    pub fn close(&mut self) {
        let (tick, digest) = (self.tick(), self.digest());
        self.journal.end(tick, digest);
    }

    /// Population, births, deaths, entity-reaction extent and the mean of each substance carried,
    /// by lineage where it matters (spec §12.2 `stats.query`).
    pub fn stats(&self) -> Value {
        let w = &self.live.world;
        let ns = w.chem.substances.len();
        let (n, births, deaths, extent, amounts, lineage): (usize, u64, u64, Vec<u64>, Vec<u32>, Vec<usize>) = match &self.backend {
            Backend::Cpu => (
                w.entities.len(),
                w.births,
                w.deaths,
                w.entity_extent.clone(),
                w.entities.iter().flat_map(|e| e.store.amounts[..ns].to_vec()).collect(),
                w.entities.iter().map(|e| e.lineage).collect(),
            ),
            Backend::Gpu(r) => {
                let s = r.snapshot();
                let lineage = s.ent.chunks(emerge_gpu::pack::ENT).map(|e| e[11] as usize).collect();
                (r.n, r.births, r.deaths, r.extent(), s.amounts, lineage)
            }
        };
        let mean: serde_json::Map<String, Value> = (0..ns)
            .map(|s| (w.chem.substances[s].name.clone(), json!(amounts.iter().skip(s).step_by(ns).map(|&a| a as f64).sum::<f64>() / n.max(1) as f64 / 1000.0)))
            .collect();
        let mut by_lineage = vec![0usize; self.spec.lineages.len()];
        for l in lineage {
            if let Some(c) = by_lineage.get_mut(l) {
                *c += 1;
            }
        }
        let reactions: serde_json::Map<String, Value> = extent
            .iter()
            .enumerate()
            .filter(|(_, &x)| x > 0)
            .map(|(r, &x)| (w.chem.formula(&w.chem.reactions[r].inputs, &w.chem.reactions[r].outputs), json!(x)))
            .collect();
        json!({
            "tick": self.tick(), "revision": self.revision(), "backend": self.device,
            "population": n, "births": births, "deaths": deaths,
            "population_by_lineage": self.spec.lineages.iter().cloned().zip(by_lineage.into_iter().map(Value::from)).collect::<serde_json::Map<String, Value>>(),
            "mean_carried_u": mean, "entity_reaction_extent_mu": reactions,
        })
    }

    /// A view's data (spec §12.2 `view.snapshot`): one substance of the conserved field over the
    /// grid, and up to `agents` entities with position, heading, lineage and ID. Stamped with the
    /// tick and revision, so a client can tell when a swap has reached it.
    pub fn snapshot(&self, substance: &str, agents: usize) -> Result<Value, String> {
        let w = &self.live.world;
        let ns = w.chem.substances.len();
        let s = w.chem.substances.iter().position(|x| x.name == substance).ok_or_else(|| format!("no substance `{substance}`"))?;
        let space = w.space.as_ref().ok_or("the world has no space")?;
        let (camt, ent, epos): (Vec<u32>, Vec<(u64, usize)>, Vec<f32>) = match &self.backend {
            Backend::Cpu => (
                w.cells.iter().map(|c| c.amounts[s]).collect(),
                w.entities.iter().map(|e| (e.id, e.lineage)).collect(),
                w.entities.iter().flat_map(|e| [e.pos[0], e.pos[1], e.heading[0], e.heading[1]]).collect(),
            ),
            Backend::Gpu(r) => {
                let snap = r.snapshot();
                let ent = snap.ent.chunks(emerge_gpu::pack::ENT).map(|e| (e[0] as u64 | (e[1] as u64) << 32, e[11] as usize)).collect();
                (snap.camt.iter().skip(s).step_by(ns).copied().collect(), ent, snap.epos)
            }
        };
        let every = (ent.len() / agents.max(1)).max(1);
        let list: Vec<Value> = ent
            .iter()
            .enumerate()
            .step_by(every)
            .take(agents)
            .map(|(i, (id, l))| json!({"id": id, "lineage": self.spec.lineages.get(*l), "x": epos[4 * i], "y": epos[4 * i + 1], "hx": epos[4 * i + 2], "hy": epos[4 * i + 3]}))
            .collect();
        Ok(json!({
            "tick": self.tick(), "revision": self.revision(),
            "field": {"substance": substance, "w": space.grid_w, "h": space.grid_h, "unit": "mu", "values": camt},
            "agents": {"total": ent.len(), "shown": list},
        }))
    }

    /// What a view reads now (`emerge_view::Frame`), from either backend.
    /// Starts a window for `made`: from the next tick, each entity counts the µ of `substance` its
    /// own reactions make (exit test 9's producers). Counting changes nothing in the run.
    pub fn count_made(&mut self, substance: &str) -> Result<(), String> {
        let s = self.live.world.chem.substances.iter().position(|x| x.name == substance).ok_or_else(|| format!("no substance `{substance}`"))?;
        self.live.world.track = Some(s);
        match &mut self.backend {
            Backend::Cpu => self.live.world.entities.iter_mut().for_each(|e| e.made = 0),
            Backend::Gpu(r) => {
                r.set_track(Some(s));
                r.clear_made();
            }
        }
        self.made = Some(emerge_view::frame::Made { substance: substance.to_string(), from_tick: self.tick() });
        Ok(())
    }

    pub fn frame(&self) -> emerge_view::Frame {
        let field = self.live.field.as_deref().unwrap_or("cells");
        let mut f = emerge_view::Frame::of_world(&self.live.world, field, &self.spec.lineages, self.revision());
        f.made = self.made.clone();
        f.history = Some(self.history.clone());
        if let Backend::Gpu(r) = &self.backend {
            use emerge_gpu::pack::ENT;
            let s = r.snapshot();
            let ns = s.ns;
            f.tick = r.tick as u64;
            f.cells = s.camt.clone();
            f.agents = (0..s.n)
                .map(|i| {
                    let e = &s.ent[i * ENT..(i + 1) * ENT];
                    emerge_view::Agent {
                        id: e[0] as u64 | (e[1] as u64) << 32,
                        lineage: e[11] as usize,
                        pos: [s.epos[4 * i], s.epos[4 * i + 1]],
                        heading: [s.epos[4 * i + 2], s.epos[4 * i + 3]],
                        age: e[10] as u64,
                        expressed_genes: e[5] as usize,
                        amounts: s.amounts[i * ns..(i + 1) * ns].to_vec(),
                        made: e[14] as u64 | (e[15] as u64) << 32,
                    }
                })
                .collect();
        }
        f
    }

    /// A living entity's lineage, age and genome. On the GPU the genome is decoded from the master
    /// pool (`emerge_gpu::pack::decode_master`), and equals the CPU's in contract mode.
    fn genome_of(&self, id: u64) -> Result<(usize, u64, emerge_ref::Genome), String> {
        match &self.backend {
            Backend::Cpu => {
                let e = self.live.world.entities.iter().find(|e| e.id == id).ok_or_else(|| format!("no living entity {id}"))?;
                Ok((e.lineage, e.age, e.genome.clone()))
            }
            Backend::Gpu(r) => r.genome_of(id).ok_or_else(|| format!("no living entity {id}")),
        }
    }

    /// The genome of entity `id` in plain language (spec §12.1 `explain`).
    pub fn explain(&self, id: u64) -> Result<String, String> {
        let (lineage, age, genome) = self.genome_of(id)?;
        let field = self.live.field.as_deref().unwrap_or("cells");
        Ok(format!("entity {id}, lineage {}, age {age}:\n{}", self.spec.lineages[lineage], emerge_ref::print::explain(&self.live.world, &genome, field)))
    }

    /// A living entity's genome as boxes and signals (`proposals/node-editor.md` N5): its lineage and
    /// age, and the graph.
    pub fn genome_graph(&self, id: u64) -> Result<(String, u64, emerge_ref::print::GenomeGraph), String> {
        let (lineage, age, genome) = self.genome_of(id)?;
        let field = self.live.field.as_deref().unwrap_or("cells");
        Ok((self.spec.lineages[lineage].clone(), age, emerge_ref::print::genome_graph(&self.live.world, &genome, field)))
    }

    /// What is at a point of the world (`view.pick`, the viewer's info picker): the cell there with its
    /// amounts, and the creatures within `radius` world units, nearest first.
    pub fn pick(&self, x: f32, y: f32, radius: f32, limit: usize) -> Result<Value, String> {
        let w = &self.live.world;
        let space = w.space.as_ref().ok_or("the world has no space to pick in")?;
        if !(x.is_finite() && y.is_finite()) || x < 0.0 || y < 0.0 || x > space.w || y > space.h {
            return Err(format!("({x}, {y}) is outside the world ({} by {})", space.w, space.h));
        }
        let ns = w.chem.substances.len();
        let cell = space.cell_at([x, y]);
        let amounts = match &self.backend {
            Backend::Cpu => w.cells[cell].amounts[..ns].to_vec(),
            Backend::Gpu(r) => r.cell_amounts(cell),
        };
        let cell_amounts: serde_json::Map<String, Value> = (0..ns).filter(|&s| amounts[s] > 0).map(|s| (w.chem.substances[s].name.clone(), json!(amounts[s]))).collect();
        struct Near {
            id: u64,
            lineage: usize,
            age: u64,
            pos: [f32; 2],
            active: usize,
            carried: Vec<u32>,
            d2: f32,
        }
        let near = |id, lineage, age, pos: [f32; 2], active, carried: Vec<u32>| {
            let (dx, dy) = (pos[0] - x, pos[1] - y);
            Some(Near { id, lineage, age, pos, active, carried, d2: dx * dx + dy * dy }).filter(|n| n.d2 <= radius * radius)
        };
        let mut found: Vec<Near> = match &self.backend {
            Backend::Cpu => w.entities.iter().filter_map(|e| near(e.id, e.lineage, e.age, e.pos, e.genome.active_genes(), e.store.amounts[..ns].to_vec())).collect(),
            Backend::Gpu(r) => {
                let (ent, epos, amounts) = r.entity_table();
                ent.chunks(emerge_gpu::pack::ENT)
                    .enumerate()
                    .filter_map(|(i, e)| near(u64::from(e[0]) | (u64::from(e[1]) << 32), e[11] as usize, u64::from(e[10]), [epos[i * 4], epos[i * 4 + 1]], e[5] as usize, amounts[i * ns..(i + 1) * ns].to_vec()))
                    .collect()
            }
        };
        found.sort_by(|a, b| a.d2.total_cmp(&b.d2).then(a.id.cmp(&b.id)));
        let total = found.len();
        let entities: Vec<Value> = found
            .iter()
            .take(limit)
            .map(|e| {
                let carried: serde_json::Map<String, Value> = (0..ns).filter(|&s| e.carried[s] > 0).map(|s| (w.chem.substances[s].name.clone(), json!(e.carried[s]))).collect();
                json!({"id": e.id, "ref": e.id.to_string(), "lineage": self.spec.lineages[e.lineage], "age": e.age, "x": e.pos[0], "y": e.pos[1], "distance": e.d2.sqrt(), "active_genes": e.active, "carried_mu": carried})
            })
            .collect();
        Ok(json!({
            "tick": self.tick(), "x": x, "y": y, "radius": radius,
            "cell": {"x": cell % space.grid_w, "y": cell / space.grid_w, "field": self.live.field, "amounts_mu": cell_amounts},
            "creatures_near": total, "creatures": entities,
        }))
    }

    /// The ID of some living entity (the first), for a call that names none.
    pub fn first_entity(&self) -> Option<u64> {
        match &self.backend {
            Backend::Cpu => self.live.world.entities.first().map(|e| e.id),
            Backend::Gpu(r) => {
                let (ent, _, _) = r.entity_table();
                ent.chunks(emerge_gpu::pack::ENT).next().map(|e| u64::from(e[0]) | (u64::from(e[1]) << 32))
            }
        }
    }

    /// Entity-reaction extent by reaction since the world began.
    pub fn entity_extent(&self) -> Vec<u64> {
        match &self.backend {
            Backend::Cpu => self.live.world.entity_extent.clone(),
            Backend::Gpu(r) => r.extent(),
        }
    }

    /// What each world statement did (`LawStat`, in the CPU's order of laws) and the tick the counts began:
    /// kept by the CPU runner, and counted on the device for a GPU run.
    pub fn law_stats(&self) -> Result<(Vec<emerge_ref::world::LawStat>, u64), String> {
        match &self.backend {
            Backend::Cpu => Ok((self.live.world.law_stats.clone(), self.live.world.law_since)),
            Backend::Gpu(r) => r.law_stats().map(|v| (v, self.law_since)).ok_or_else(|| "the GPU run has no statement counters".to_string()),
        }
    }

    /// Starts the resource budget window (`accounts.start`, `proposals/science-panels.md` §8): the
    /// runtime's own per-lineage accounts of the upkeep substance, counted from now. A GPU run keeps them
    /// by founding lineage only (no clade, no split by who makes a substance).
    pub fn start_accounts(&mut self, clade: Option<u64>, flux: Option<&str>) -> Result<Value, String> {
        if let Backend::Gpu(_) = &self.backend {
            if clade.is_some() || flux.is_some() {
                return Err("the GPU keeps resource accounts by founding lineage only: a clade or a split by who makes a substance needs the CPU runner".into());
            }
            if self.spec.lineages.len() > 16 { return Err("the GPU keeps accounts for at most 16 lineages".into()); }
            if !self.live.world.creatures {
                return Err("the world has no creatures, so there is no upkeep substance to account for".into());
            }
            let pay = self.live.world.upkeep.pay;
            let n = self.spec.lineages.len();
            let (mut count, mut stock) = (vec![0u64; n], vec![0u64; n]);
            if let Backend::Gpu(r) = &self.backend {
                let ns = r.ns();
                let (ent, _, amounts) = r.entity_table();
                for (e, a) in ent.chunks(emerge_gpu::pack::ENT).zip(amounts.chunks(ns)) { count[e[11] as usize] += 1; stock[e[11] as usize] += u64::from(a[pay]); }
            }
            if let Backend::Gpu(r) = &mut self.backend { r.enable_accounts(n); }
            self.acct = Some(AcctStart { tick: self.tick(), revision: self.live.revision, names: self.spec.lineages.clone(), count, stock, disposal0: vec![] });
            return Ok(json!({"started_at_tick": self.tick(), "substance": self.live.world.chem.substances[pay].name}));
        }
        if !self.live.world.creatures {
            return Err("the world has no creatures, so there is no upkeep substance to account for".into());
        }
        let members = match clade { Some(id) => Some(self.clade_of(id)?), None => None };
        if let Some(id) = clade {
            if !self.live.world.entities.iter().any(|e| e.id == id) { return Err(format!("entity {id} is not alive: a clade starts from a living creature")); }
        }
        let flux_id = match flux {
            Some(f) => Some(self.live.world.chem.substances.iter().position(|s| s.name == f).ok_or_else(|| format!("no substance `{f}`"))?),
            None => None,
        };
        let w = &mut self.live.world;
        w.accounts = Some(emerge_ref::world::Accounts { clade: members, flux: flux_id, ..Default::default() });
        let pay = w.upkeep.pay;
        let base: Vec<String> = match clade { Some(id) => vec!["everyone else".into(), format!("clade of #{id}")], None => self.spec.lineages.clone() };
        let names: Vec<String> = match flux { Some(f) => base.iter().flat_map(|b| [format!("{b} / not making {f}"), format!("{b} / making {f}")]).collect(), None => base };
        let (mut count, mut stock) = (vec![0u64; names.len()], vec![0u64; names.len()]);
        let acc = w.accounts.as_ref().unwrap();
        for e in &w.entities {
            let g = acc.group_of(&w.chem, e);
            count[g] += 1;
            stock[g] += u64::from(e.store.amounts[pay]);
        }
        let disposal0 = (0..self.spec.lineages.len()).map(|l| (w.disposal.to_cell.get(l).map_or(0, |v| v[pay]), w.disposal.to_remains.get(l).map_or(0, |v| v[pay]))).collect();
        self.acct = Some(AcctStart { tick: w.tick, revision: self.live.revision, names, count, stock, disposal0 });
        Ok(json!({"started_at_tick": self.tick(), "substance": self.live.world.chem.substances[pay].name}))
    }

    /// The budget of each lineage since `start_accounts`: stock then and now, the runtime's account
    /// columns, and what they leave unexplained.
    pub fn accounts(&self) -> Result<Value, String> {
        if let Backend::Gpu(r) = &self.backend {
            let Some(start) = &self.acct else { return Err("no budget window: start one with accounts.start".into()) };
            let totals = r.accounts().ok_or("no budget window: start one with accounts.start")?;
            let w = &self.live.world;
            let pay = w.upkeep.pay;
            let n = start.names.len();
            let (mut count, mut stock) = (vec![0u64; n], vec![0u64; n]);
            let ns = r.ns();
            let (ent, _, amounts) = r.entity_table();
            for (e, a) in ent.chunks(emerge_gpu::pack::ENT).zip(amounts.chunks(ns)) { count[e[11] as usize] += 1; stock[e[11] as usize] += u64::from(a[pay]); }
            let col = |name: &str| emerge_ref::world::ACCOUNT_COLUMNS.iter().position(|c| *c == name).unwrap();
            let lineages: Vec<Value> = (0..n)
                .map(|g| {
                    let t = totals[g];
                    let lines = budget_lines(&t);
                    let get = |name: &str| t[col(name)] as i128;
                    let explained: i128 = lines.iter().map(|(_, s, x)| s * x).sum();
                    let change = stock[g] as i128 - start.stock[g] as i128;
                    json!({"disposal": {"unavailable": "where the dead's stock went is counted on the CPU runner only"}, "lineage": start.names[g], "creatures_start": start.count[g], "creatures_now": count[g],
                        "births": get("births"), "starved": get("starved"), "aged": get("aged"),
                        "stock_start_mu": start.stock[g], "stock_now_mu": stock[g], "change_mu": change,
                        "lines": lines.iter().map(|(l, s, x)| json!({"name": l, "sign": s, "mu": x})).collect::<Vec<_>>(),
                        "explained_mu": explained, "unreconciled_mu": change - explained})
                })
                .collect();
            return Ok(json!({"substance": w.chem.substances[pay].name, "unit": "µ (millionths of a unit)", "from_tick": start.tick, "to_tick": self.tick(),
                "revision_start": start.revision, "revision_now": self.live.revision, "crosses_edit": start.revision != self.live.revision,
                "scope": "each founding lineage as a whole; a creature's stock moves with it, births count in the parent's lineage (counted on the GPU, per lineage)",
                "not_counted": "stock that creatures bring in by genesis or injection after the start is not in the account columns and shows as unreconciled",
                "lineages": lineages}));
        }
        let (Some(start), Some(a)) = (&self.acct, &self.live.world.accounts) else { return Err("no budget window: start one with accounts.start".into()) };
        let w = &self.live.world;
        let pay = w.upkeep.pay;
        let n = start.names.len();
        let (mut count, mut stock) = (vec![0u64; n], vec![0u64; n]);
        for e in &w.entities {
            let g = a.group_of(&w.chem, e);
            count[g] += 1;
            stock[g] += u64::from(e.store.amounts[pay]);
        }
        let col = |name: &str| emerge_ref::world::ACCOUNT_COLUMNS.iter().position(|c| *c == name).unwrap();
        let lineages: Vec<Value> = (0..n)
            .map(|g| {
                let t = a.totals.get(g).copied().unwrap_or([0; emerge_ref::world::ACCOUNT_COLUMNS.len()]);
                let lines = budget_lines(&t);
                let get = |name: &str| t[col(name)] as i128;
                let explained: i128 = lines.iter().map(|(_, s, x)| s * x).sum();
                let change = stock[g] as i128 - start.stock[g] as i128;
                // Where the dead's stock went after `decompose` (cells, or the `remains` sink); by founding lineage only.
                let disposal = if a.clade.is_none() && a.flux.is_none() {
                    let (c0, r0) = start.disposal0[g];
                    let (c1, r1) = (w.disposal.to_cell.get(g).map_or(0, |v| v[pay]), w.disposal.to_remains.get(g).map_or(0, |v| v[pay]));
                    let after = (c1 - c0 + r1 - r0) as i128;
                    json!({"to_cells_mu": c1 - c0, "to_remains_mu": r1 - r0, "died_with_mu": get("pay_died_with_mu"), "changed_by_decompose_mu": get("pay_died_with_mu") - after,
                        "note": "died with is the stock before decompose ran; the two routes are what was left after it. The difference is what decompose turned into other substances (or into this one)."})
                } else { json!({"unavailable": "disposal routes are kept by founding lineage, not by clade or by who makes a substance"}) };
                json!({"disposal": disposal, "lineage": start.names[g], "creatures_start": start.count[g], "creatures_now": count[g], "births": get("births"), "starved": get("starved"), "aged": get("aged"),
                    "stock_start_mu": start.stock[g], "stock_now_mu": stock[g], "change_mu": change,
                    "lines": lines.iter().map(|(l, s, x)| json!({"name": l, "sign": s, "mu": x})).collect::<Vec<_>>(),
                    "explained_mu": explained, "unreconciled_mu": change - explained})
            })
            .collect();
        Ok(json!({"substance": w.chem.substances[pay].name, "unit": "µ (millionths of a unit)", "from_tick": start.tick, "to_tick": self.tick(),
            "revision_start": start.revision, "revision_now": self.live.revision, "crosses_edit": start.revision != self.live.revision,
            "scope": if a.flux.is_some() { "each group is a founding lineage (or the clade) split into creatures with an active reaction gene that makes the chosen substance, and the rest; a creature's genome never changes after birth, so it stays in its group, and a child joins its parent's lineage" } else if a.clade.is_some() { "a clade (the creature and every descendant, children join at birth) against everyone else; stock moving between the two is a line, not an error" } else { "each founding lineage as a whole; a creature's stock moves with it, births count in the parent's lineage" },
            "not_counted": "stock that creatures bring in by genesis or injection after the start is not in the account columns and shows as unreconciled",
            "lineages": lineages}))
    }

    /// One creature's family (`ancestry.query`, `proposals/science-panels.md` §6) from the birth and
    /// death events the runner keeps: its ancestors back to a founder, and the descendants it
    /// has had (ever, and alive now). A GPU run keeps them from its start (the CPU from tick 0).
    pub fn ancestry(&self, id: u64, max_ancestors: usize) -> Result<Value, String> {
        use emerge_ref::world::Event;
        let living = self.living();
        let me = living.iter().find(|e| e.id == id).ok_or_else(|| format!("entity {id} is not alive (ancestry starts from a living creature)"))?;
        let (events, dropped) = self.event_list()?;
        let mut parent: std::collections::HashMap<u64, (u64, u64)> = Default::default();
        let mut children: std::collections::HashMap<u64, Vec<u64>> = Default::default();
        let mut died: std::collections::HashMap<u64, (u64, &'static str)> = Default::default();
        for ev in events.iter() {
            match ev {
                Event::Born { tick, parent: p, child } => {
                    parent.insert(*child, (*p, *tick));
                    children.entry(*p).or_default().push(*child);
                }
                Event::Died { tick, id, cause } => {
                    died.insert(*id, (*tick, if *cause == emerge_ref::Cause::Starved { "starved" } else { "aged" }));
                }
            }
        }
        let alive: std::collections::HashSet<u64> = living.iter().map(|e| e.id).collect();
        let row = |x: u64| -> Value {
            let (born, par) = match parent.get(&x) { Some((p, t)) => (json!(t), json!(p)), None => (Value::Null, Value::Null) };
            json!({"id": x, "parent": par, "born": born, "alive": alive.contains(&x), "died": died.get(&x).map(|(t, c)| json!({"tick": t, "cause": c})),
                "children": children.get(&x).map_or(0, Vec::len)})
        };
        let mut up = vec![];
        let mut at = id;
        let mut reached_founder = false;
        while up.len() < max_ancestors {
            match parent.get(&at) {
                Some((p, _)) => { up.push(row(*p)); at = *p; }
                None => { reached_founder = true; break; }
            }
        }
        // Descendants, breadth first.
        let (mut ever, mut living, mut frontier) = (0u64, 0u64, vec![id]);
        let mut deepest = 0;
        while !frontier.is_empty() {
            let mut next = vec![];
            for x in &frontier { for c in children.get(x).into_iter().flatten() { ever += 1; living += u64::from(alive.contains(c)); next.push(*c); } }
            if !next.is_empty() { deepest += 1; }
            frontier = next;
        }
        Ok(json!({"entity": row(id), "lineage": self.spec.lineages[me.lineage], "tick": self.tick(),
            "ancestors": up, "ancestors_complete": reached_founder, "ancestor_limit": max_ancestors,
            "founder": if reached_founder { json!(at) } else { Value::Null },
            "descendants": {"ever": ever, "alive_now": living, "generations": deepest},
            "source": if matches!(self.backend, Backend::Cpu) { "birth and death events kept by the CPU runner since tick 0; a genesis creature has no recorded parent" } else { "birth and death events logged on the GPU since the run started; a genesis creature has no recorded parent" },
            "events_dropped": dropped, "events_dropped_note": "records the GPU's event log had no room for; a chain that crosses them looks like it reaches a founder",
            "not_recorded": "genome differences between parent and child are not kept unless recorded at birth"}))
    }

    /// Totals over a rectangle of the world, read in one call so cells and creatures are of the same
    /// tick (`region.query`, `proposals/science-panels.md` §9). With `wrap`, a range whose start is past
    /// its end runs across the edge. Cell amounts need the CPU runner.
    pub fn region(&self, x: [f32; 2], y: [f32; 2]) -> Result<Value, String> {
        let w = &self.live.world;
        let space = w.space.as_ref().ok_or("the world has no space")?;
        let within = |v: f32, r: [f32; 2], size: f32| -> Result<bool, String> {
            if r.iter().any(|a| !a.is_finite() || *a < 0.0 || *a > size) { return Err(format!("a range must lie inside 0 to {size}")); }
            Ok(if r[0] <= r[1] { v >= r[0] && v <= r[1] } else if space.wrap { v >= r[0] || v <= r[1] } else { return Err("a range that starts after it ends needs a wrapping world".into()) })
        };
        within(0.0, x, space.w)?;
        within(0.0, y, space.h)?;
        let ns = w.chem.substances.len();
        let n = self.spec.lineages.len();
        let (mut count, mut carried) = (vec![0u64; n], vec![0u64; ns]);
        let rows: Vec<(usize, [f32; 2], Vec<u32>)> = match &self.backend {
            Backend::Cpu => w.entities.iter().map(|e| (e.lineage, e.pos, e.store.amounts[..ns].to_vec())).collect(),
            Backend::Gpu(r) => {
                let (ent, epos, amounts) = r.entity_table();
                ent.chunks(emerge_gpu::pack::ENT).enumerate().map(|(i, e)| (e[11] as usize, [epos[i * 4], epos[i * 4 + 1]], amounts[i * ns..(i + 1) * ns].to_vec())).collect()
            }
        };
        for (l, pos, amt) in &rows {
            if within(pos[0], x, space.w)? && within(pos[1], y, space.h)? {
                count[*l] += 1;
                for s in 0..ns { carried[s] += u64::from(amt[s]); }
            }
        }
        let (mut cells, mut cell_amounts) = (0u64, vec![0u64; ns]);
        let cell_side = |size: f32, grid: usize| size / grid as f32;
        if matches!(self.backend, Backend::Cpu) {
            for cy in 0..space.grid_h {
                for cx in 0..space.grid_w {
                    let centre = [(cx as f32 + 0.5) * cell_side(space.w, space.grid_w), (cy as f32 + 0.5) * cell_side(space.h, space.grid_h)];
                    if within(centre[0], x, space.w)? && within(centre[1], y, space.h)? {
                        cells += 1;
                        for s in 0..ns { cell_amounts[s] += u64::from(w.cells[cy * space.grid_w + cx].amounts[s]); }
                    }
                }
            }
        }
        let by_sub = |v: &[u64]| -> serde_json::Map<String, Value> { (0..ns).filter(|&s| v[s] > 0).map(|s| (w.chem.substances[s].name.clone(), json!(v[s]))).collect() };
        Ok(json!({"tick": self.tick(), "region": {"x": x, "y": y, "wraps": x[0] > x[1] || y[0] > y[1]}, "world": {"w": space.w, "h": space.h, "wrap": space.wrap},
            "creatures": self.spec.lineages.iter().zip(&count).map(|(l, c)| json!({"lineage": l, "count": c})).collect::<Vec<_>>(), "creatures_total": count.iter().sum::<u64>(),
            "carried_mu": by_sub(&carried),
            "cells": if matches!(self.backend, Backend::Cpu) { json!({"count": cells, "amounts_mu": by_sub(&cell_amounts), "rule": "a cell counts when its centre is inside the region"}) } else { json!({"unavailable": "cell amounts of a GPU run are not read by this query"}) },
            "unit": "µ (millionths of a unit); creatures count when their position is inside the region"}))
    }

    /// The creature `id` and every descendant born so far, from the runner's birth events.
    pub fn clade_of(&self, id: u64) -> Result<std::collections::HashSet<u64>, String> {
        use emerge_ref::world::Event;
        let mut children: std::collections::HashMap<u64, Vec<u64>> = Default::default();
        let (events, _) = self.event_list()?;
        for ev in events.iter() {
            if let Event::Born { parent, child, .. } = ev { children.entry(*parent).or_default().push(*child); }
        }
        let mut seen: std::collections::HashSet<u64> = [id].into();
        let mut stack = vec![id];
        while let Some(x) = stack.pop() {
            for c in children.get(&x).into_iter().flatten() { if seen.insert(*c) { stack.push(*c); } }
        }
        Ok(seen)
    }

    /// The traits of the living population (`genome.atlas`, `proposals/science-panels.md` §7): for each
    /// group (a founding lineage, or a clade against the rest), how many creatures carry each gene, as an
    /// active gene or only dormant. Whole population up to `MAX`, an even sample past it, and it says which.
    pub fn atlas(&self, clade: Option<u64>) -> Result<Value, String> {
        const MAX: usize = 4000;
        let w = &self.live.world;
        if !matches!(self.backend, Backend::Cpu) { return Err("the atlas reads genomes from the CPU runner".into()); }
        let members = match clade { Some(id) => Some(self.clade_of(id)?), None => None };
        let field = self.live.field.as_deref().unwrap_or("cells");
        let every = (w.entities.len() / MAX).max(1);
        let groups: Vec<String> = match clade { Some(id) => vec![format!("clade of #{id}"), "everyone else".into()], None => self.spec.lineages.clone() };
        let mut count = vec![0u64; groups.len()];
        let mut traits: Vec<std::collections::BTreeMap<(String, String, String), (u64, u64)>> = vec![Default::default(); groups.len()];
        for e in w.entities.iter().step_by(every) {
            let g = match &members { Some(m) => usize::from(!m.contains(&e.id)), None => e.lineage };
            count[g] += 1;
            let gg = emerge_ref::print::genome_graph(w, &e.genome, field);
            for x in &gg.genes {
                let t = traits[g].entry((x.kind.to_string(), x.title.clone(), x.text.clone())).or_default();
                if x.active { t.0 += 1 } else { t.1 += 1 }
            }
        }
        let rows: Vec<Value> = groups.iter().enumerate().map(|(i, name)| json!({"group": name, "creatures": count[i],
            "traits": traits[i].iter().map(|((k, t, x), (a, d))| json!({"kind": k, "title": t, "text": x, "active": a, "dormant": d})).collect::<Vec<_>>()})).collect();
        Ok(json!({"tick": self.tick(), "coverage": if every == 1 { "every living creature".to_string() } else { format!("an even sample, every {every}th of {} creatures", w.entities.len()) },
            "trait": "a gene (kind, name and plain-words text); a creature may carry the same gene more than once (counted each time)", "groups": rows}))
    }

    /// Keeps the tick's transfer requests that the open recording wants, up to its cap.
    fn capture_trace(&mut self) {
        let Some(t) = &mut self.trace else { return };
        if let Backend::Gpu(r) = &mut self.backend {
            // The device writes the records itself; here the window only closes.
            let tick = u64::from(r.tick);
            t.last_tick = tick.min(t.until).max(t.from);
            if tick >= t.until { r.trace_stop(); }
            return;
        }
        let w = &self.live.world;
        let tick = w.tick;
        if tick <= t.from || tick > t.until { return; }
        t.last_tick = tick;
        let end = |e: &emerge_ref::world::End, id: Option<u64>| -> String { use emerge_ref::world::End::*; match e { Cell(c) => format!("cell {c}"), Entity(_) => format!("creature #{}", id.unwrap_or(0)), Source(s) => format!("source {s}"), Sink(s) => format!("sink {s}") } };
        for (x, m) in w.last_transfers.iter().zip(&w.last_transfer_meta) {
            let wanted = match m.law { Some(l) => t.laws.as_ref().is_none_or(|ls| ls.contains(&l)), None => t.creatures };
            if !wanted { continue; }
            if let Some(ids) = &t.ids {
                if !(m.origin_id.is_some_and(|i| ids.contains(&i)) || m.dest_id.is_some_and(|i| ids.contains(&i))) { continue; }
            }
            t.seen += 1;
            if t.records.len() >= t.cap { t.dropped += 1; continue; }
            t.records.push(TraceRec { tick, law: m.law, origin: end(&x.origin, m.origin_id), dest: end(&x.dest, m.dest_id), steal: m.steal, substance: x.s, requested: x.requested, delivered: x.delivered, overflowed: x.overflowed });
        }
    }

    /// Starts a bounded recording of transfer requests over the next `ticks` ticks (`trace.start`,
    /// `proposals/science-panels.md` §4): all of them, or only some rules' statements (`laws`) and/or creatures'
    /// own actions. It keeps at most `cap` records and counts the rest as dropped. Counted on the device for a GPU run (at most 32768 records).
    pub fn start_trace(&mut self, ticks: u64, cap: usize, laws: Option<Vec<u32>>, creatures: bool) -> Result<Value, String> {
        self.start_trace_as(ticks, cap, laws, creatures, "trace.start", false)
    }

    /// `start_trace` by `owner`. A recording still open that someone else started is refused unless `replace`
    /// (`proposals/agent-tracking.md` §14: a tracker must not silently replace the Rule Lens's evidence, or the other way).
    pub fn start_trace_as(&mut self, ticks: u64, cap: usize, laws: Option<Vec<u32>>, creatures: bool, owner: &str, replace: bool) -> Result<Value, String> {
        let from = self.tick();
        if let Some(t) = &self.trace {
            if from < t.until && t.owner != owner && !replace {
                return Err(format!("the transfer recording is held by {} until tick {}: wait for it, or pass replace: true", t.owner, t.until));
            }
        }
        if let Backend::Gpu(r) = &mut self.backend {
            if cap > emerge_gpu::pack::TRACE_CAP { return Err(format!("the GPU keeps at most {} records", emerge_gpu::pack::TRACE_CAP)); }
            r.trace_start(cap, laws.as_deref(), creatures)?;
        }
        self.trace = Some(Trace { from, until: from + ticks, cap, laws, creatures, records: vec![], seen: 0, dropped: 0, last_tick: from, owner: owner.to_string(), ids: None });
        Ok(json!({"from_tick": from, "until_tick": from + ticks, "cap": cap, "owner": owner}))
    }

    /// A tracker's claim on the transfer recording: an open one that keeps creatures' actions is shared (its owner
    /// is returned); an open one that does not is refused; with none open, one of creatures' actions starts.
    pub fn claim_trace(&mut self, owner: &str, ticks: u64, ids: Option<std::collections::HashSet<u64>>) -> Result<String, String> {
        let now = self.tick();
        if let Some(t) = &self.trace {
            if now < t.until {
                if t.creatures { return Ok(t.owner.clone()); }
                return Err(format!("the transfer recording is held by {} until tick {} and does not keep creatures' actions", t.owner, t.until));
            }
        }
        let cap = if self.spec.gpu { emerge_gpu::pack::TRACE_CAP } else { 100_000 };
        self.start_trace_as(ticks, cap, Some(vec![]), true, owner, false)?;
        if let Some(t) = &mut self.trace { t.ids = ids; }
        Ok(owner.to_string())
    }

    /// The kept transfer records, how many matched and how many were dropped.
    fn trace_records(&self) -> Result<(std::borrow::Cow<'_, [TraceRec]>, u64, u64), String> {
        let t = self.trace.as_ref().ok_or("no recording: start one with trace.start")?;
        match &self.backend {
            Backend::Cpu => Ok((std::borrow::Cow::Borrowed(&t.records[..]), t.seen, t.dropped)),
            Backend::Gpu(r) => {
                let (recs, seen) = r.trace_read().ok_or("no recording: start one with trace.start")?;
                let name = |k: u8, v: u64| match k { 0 => format!("cell {v}"), 1 => format!("creature #{v}"), 2 => format!("source {v}"), _ => format!("sink {v}") };
                let mut v: Vec<TraceRec> = recs.iter().map(|x| TraceRec { tick: x.tick, law: x.law.map(|l| l as u32), origin: name(x.origin.0, x.origin.1), dest: name(x.dest.0, x.dest.1), steal: x.steal, substance: x.substance, requested: x.requested, delivered: x.delivered, overflowed: x.overflowed }).collect();
                v.sort_by(|a, b| (a.tick, a.law, &a.origin, &a.dest, a.substance).cmp(&(b.tick, b.law, &b.origin, &b.dest, b.substance)));
                let kept = v.len() as u64;
                Ok((std::borrow::Cow::Owned(v), seen, seen - kept))
            }
        }
    }

    /// The kept transfer requests that involve any of `ids` at either end (a tracker's recorded interactions), with
    /// the recording's window, owner and losses: a full recording cannot give an exact count.
    pub fn interactions_of(&self, ids: &std::collections::HashSet<u64>) -> Result<Value, String> {
        let t = self.trace.as_ref().ok_or("no transfer recording")?;
        let (records, seen, dropped) = self.trace_records()?;
        let who = |e: &str| e.strip_prefix("creature #").and_then(|x| x.parse::<u64>().ok());
        let names = self.substance_names();
        let rows: Vec<Value> = records.iter().filter(|r| who(&r.origin).is_some_and(|i| ids.contains(&i)) || who(&r.dest).is_some_and(|i| ids.contains(&i)))
            .map(|r| json!({"tick": r.tick, "from": r.origin, "to": r.dest, "kind": if r.steal { "steal" } else if r.law.is_none() { "creature" } else { "rule" }, "substance": names.get(r.substance), "requested_mu": r.requested, "delivered_mu": r.delivered})).collect();
        let kept = match (&t.ids, &self.backend) { (Some(_), Backend::Cpu) => "only requests involving the tracked creatures were kept", (_, Backend::Gpu(_)) => "the device keeps every creature's requests (up to its cap); these are filtered from them", _ => "the recording keeps what its owner asked for; these are filtered from it" };
        Ok(json!({"owner": t.owner, "from_tick": t.from, "until_tick": t.until, "records": rows, "requests_matching_recording": seen, "dropped": dropped, "kept": kept,
            "complete": dropped == 0, "note": "requests where a tracked creature is at either end, from the run's one transfer recording; a recording that dropped records gives no exact interaction count"}))
    }

    /// The recording so far, with what it kept and what it dropped, and who gave to or took from whom
    /// (`trace.query`).
    pub fn trace(&self, laws: &[String], limit: usize) -> Result<Value, String> {
        let t = self.trace.as_ref().ok_or("no recording: start one with trace.start")?;
        let w = &self.live.world;
        let gpu_view;
        let (records, seen, dropped): (&[TraceRec], u64, u64) = match &self.backend {
            Backend::Cpu => (&t.records, t.seen, t.dropped),
            Backend::Gpu(r) => {
                let (recs, seen) = r.trace_read().ok_or("no recording: start one with trace.start")?;
                let name = |k: u8, v: u64| match k { 0 => format!("cell {v}"), 1 => format!("creature #{v}"), 2 => format!("source {v}"), _ => format!("sink {v}") };
                let mut v: Vec<TraceRec> = recs.iter().map(|x| TraceRec { tick: x.tick, law: x.law.map(|l| l as u32), origin: name(x.origin.0, x.origin.1), dest: name(x.dest.0, x.dest.1), steal: x.steal, substance: x.substance, requested: x.requested, delivered: x.delivered, overflowed: x.overflowed }).collect();
                v.sort_by(|a, b| (a.tick, a.law, &a.origin, &a.dest, a.substance).cmp(&(b.tick, b.law, &b.origin, &b.dest, b.substance)));
                let kept = v.len() as u64;
                gpu_view = v;
                (&gpu_view[..], seen, seen - kept)
            }
        };
        let sub = |s: usize| w.chem.substances[s].name.clone();
        let who = |l: Option<u32>| match l { Some(l) => laws.get(l as usize).cloned().unwrap_or_else(|| format!("law {l}")), None => "a creature's action".to_string() };
        let mut pairs: std::collections::BTreeMap<(String, String, String, bool), (u64, u64)> = Default::default();
        for r in records.iter().filter(|r| r.law.is_none() && r.origin.starts_with("creature") && r.dest.starts_with("creature")) {
            let e = pairs.entry((r.origin.clone(), r.dest.clone(), sub(r.substance), r.steal)).or_default();
            e.0 += r.requested as u64; e.1 += r.delivered as u64;
        }
        Ok(json!({"from_tick": t.from, "until_tick": t.until, "last_tick_seen": t.last_tick, "tick_now": self.tick(), "open": self.tick() < t.until,
            "requests_matching": seen, "kept": records.len(), "dropped": dropped, "cap": t.cap,
            "complete": dropped == 0 && self.tick() >= t.until,
            "records": records.iter().take(limit).map(|r| json!({"tick": r.tick, "by": who(r.law), "from": r.origin, "to": r.dest, "kind": if r.steal { "steal" } else if r.law.is_none() { "creature" } else { "rule" }, "substance": sub(r.substance), "requested_mu": r.requested, "delivered_mu": r.delivered, "overflowed_mu": r.overflowed})).collect::<Vec<_>>(),
            "interactions": pairs.iter().map(|((a, b, s, steal), (req, del))| json!({"from": a, "to": b, "substance": s, "kind": if *steal { "steal" } else { "give" }, "requested_mu": req, "delivered_mu": del})).collect::<Vec<_>>(),
            "meaning": "a request is what a rule or creature asked to move; delivered is what arbitration let arrive. Interactions are creature-to-creature give and steal requests among the kept records.",
            "scope": "only requests of the ticks after the start, within the cap; nothing before the start can be recovered"}))
    }

    /// Life histories (`life.query`, `proposals/science-panels.md` §10): the age and carried-stock distributions of
    /// the living, by lineage (the whole population), and the cohort born in ticks `from` to `to`: its size, how
    /// it has fared (still alive counts as censored, never as dead), survival by age, age at first child and
    /// offspring so far. From the runner's birth and death events.
    pub fn life(&self, from: u64, to: u64, bins: usize) -> Result<Value, String> {
        use emerge_ref::world::Event;
        let living = self.living();
        let (events, dropped) = self.event_list()?;
        let now = self.tick();
        let n = self.spec.lineages.len();
        let max_age = living.iter().map(|e| e.age).max().unwrap_or(0).max(1);
        let width = (max_age / bins as u64 + 1).max(1);
        let mut ages = vec![vec![0u64; bins]; n];
        let mut stock: Vec<Vec<u64>> = vec![vec![]; n];
        for e in &living {
            ages[e.lineage][((e.age / width) as usize).min(bins - 1)] += 1;
            stock[e.lineage].push(e.stock);
        }
        let stock_rows: Vec<Value> = stock.iter_mut().enumerate().map(|(i, v)| { v.sort_unstable(); let k = v.len(); json!({"lineage": self.spec.lineages[i], "creatures": k, "total_carried_mu": if k == 0 { Value::Null } else { json!({"min": v[0], "median": v[k / 2], "max": v[k - 1], "mean": v.iter().sum::<u64>() as f64 / k as f64}) }}) }).collect();
        // Cohort: creatures born in [from, to], by their birth events.
        let mut born: std::collections::HashMap<u64, u64> = Default::default();
        let mut parent_of: std::collections::HashMap<u64, u64> = Default::default();
        let mut died: std::collections::HashMap<u64, (u64, bool)> = Default::default();
        let mut first_child: std::collections::HashMap<u64, u64> = Default::default();
        let mut kids: std::collections::HashMap<u64, u64> = Default::default();
        for ev in events.iter() {
            match ev {
                Event::Born { tick, parent, child } => { born.insert(*child, *tick); parent_of.insert(*child, *parent); first_child.entry(*parent).or_insert(*tick); *kids.entry(*parent).or_default() += 1; }
                Event::Died { tick, id, cause } => { died.insert(*id, (*tick, *cause == emerge_ref::Cause::Starved)); }
            }
        }
        let cohort: Vec<u64> = { let mut v: Vec<u64> = born.iter().filter(|(_, t)| **t >= from && **t <= to).map(|(id, _)| *id).collect(); v.sort_unstable(); v };
        let (mut starved, mut aged, mut alive) = (0u64, 0u64, 0u64);
        let mut death_ages: Vec<u64> = vec![];
        let mut first_div: Vec<u64> = vec![];
        let mut offspring = 0u64;
        for id in &cohort {
            let b = born[id];
            match died.get(id) { Some((t, st)) => { if *st { starved += 1 } else { aged += 1 }; death_ages.push(t - b); } None => alive += 1 }
            if let Some(t) = first_child.get(id) { first_div.push(t - b); }
            offspring += kids.get(id).copied().unwrap_or(0);
        }
        // Survival by age over ages every member has had the chance to reach: a member born at tick b can be observed to age now-b.
        let horizon = cohort.iter().map(|id| now - born[id]).min().unwrap_or(0);
        let step = (horizon / 10).max(1);
        let survival: Vec<Value> = (0..=horizon).step_by(step as usize).map(|a| json!({"age": a, "alive": cohort.iter().filter(|id| died.get(id).is_none_or(|(t, _)| *t - born[id] > a)).count()})).collect();
        first_div.sort_unstable();
        let founders = living.iter().filter(|e| !born.contains_key(&e.id)).count();
        Ok(json!({"tick": now, "snapshot": {"coverage": "every living creature", "age_bin_width": width, "ages": self.spec.lineages.iter().zip(&ages).map(|(l, a)| json!({"lineage": l, "bins": a})).collect::<Vec<_>>(), "carried": stock_rows,
                "recent": self.history.samples.last().map(|s| json!({"tick": s.tick, "births_since_previous_sample": s.births, "deaths_since_previous_sample": s.deaths}))},
            "cohort": {"born_from": from, "born_to": to, "size": cohort.len(), "starved": starved, "aged": aged, "alive_censored": alive,
                "survival": survival, "survival_note": format!("ages beyond {horizon} ticks are not shown: the youngest member has only been observed that long"),
                "first_child_age": if first_div.is_empty() { Value::Null } else { json!({"count": first_div.len(), "min": first_div[0], "median": first_div[first_div.len() / 2], "max": first_div[first_div.len() - 1], "of": cohort.len(), "note": "only members that had a child by now; the rest may still"}) },
                "offspring_so_far": offspring, "offspring_note": "children born to cohort members by now, not lifetime reproductive success while any member lives",
                "mean_age_at_death": if death_ages.is_empty() { Value::Null } else { json!(death_ages.iter().sum::<u64>() as f64 / death_ages.len() as f64) }, "deaths_are_of_the_dead_only": true},
            "source": format!("birth and death events kept since tick 0 (a birth in tick t happens during that tick's step and shows in the sample taken at t + 1); {founders} living creatures are founders (no recorded birth) and belong to no cohort"),
            "events_dropped": dropped,
            "death_causes": "starved or aged, as the engine records them; why a creature starved is not recorded"}))
    }

    /// The active-gene count of every living entity, by lineage: the whole population, not a sample
    /// (`genome.census`, `proposals/science-panels.md` S2). Read-only.
    pub fn census(&self) -> Value {
        let mut by: Vec<Vec<u64>> = vec![vec![]; self.spec.lineages.len()];
        match &self.backend {
            Backend::Cpu => self.live.world.entities.iter().for_each(|e| by[e.lineage].push(e.genome.active_genes() as u64)),
            Backend::Gpu(r) => r.entity_table().0.chunks(emerge_gpu::pack::ENT).for_each(|e| by[e[11] as usize].push(u64::from(e[5]))),
        }
        let rows: Vec<Value> = by
            .iter_mut()
            .enumerate()
            .map(|(i, v)| {
                v.sort_unstable();
                let n = v.len();
                let mut hist = std::collections::BTreeMap::new();
                v.iter().for_each(|g| *hist.entry(*g).or_insert(0u64) += 1);
                json!({"lineage": self.spec.lineages[i], "count": n,
                    "active_genes": if n == 0 { Value::Null } else { json!({"min": v[0], "median": v[n / 2], "max": v[n - 1], "mean": v.iter().sum::<u64>() as f64 / n as f64}) },
                    "histogram": hist.iter().map(|(g, c)| json!({"active_genes": g, "count": c})).collect::<Vec<_>>()})
            })
            .collect();
        json!({"tick": self.tick(), "coverage": "every living entity (exact counts, no sampling)", "metric": "active genes per entity", "lineages": rows})
    }

    /// The creatures with these IDs, as they are now, in the order asked (`None`: not living). One read of the
    /// runner's state for all of them: the CPU world, or one read-back of the GPU's entity table.
    pub fn agents_by_id(&self, ids: &[u64]) -> Vec<Option<AgentRow>> {
        let want: std::collections::HashMap<u64, usize> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        let mut out = vec![None; ids.len()];
        match &self.backend {
            Backend::Cpu => {
                let ns = self.live.world.chem.substances.len();
                for e in &self.live.world.entities {
                    if let Some(&i) = want.get(&e.id) {
                        out[i] = Some(AgentRow { id: e.id, lineage: e.lineage, age: e.age, pos: e.pos, heading: e.heading, active_genes: e.genome.active_genes(), carried: e.store.amounts[..ns].to_vec() });
                    }
                }
            }
            Backend::Gpu(r) => {
                let ns = r.ns();
                let (ent, epos, amounts) = r.entity_table();
                for (k, e) in ent.chunks(emerge_gpu::pack::ENT).enumerate() {
                    let id = u64::from(e[0]) | (u64::from(e[1]) << 32);
                    if let Some(&i) = want.get(&id) {
                        out[i] = Some(AgentRow { id, lineage: e[11] as usize, age: u64::from(e[10]), pos: [epos[k * 4], epos[k * 4 + 1]], heading: [epos[k * 4 + 2], epos[k * 4 + 3]], active_genes: e[5] as usize, carried: amounts[k * ns..(k + 1) * ns].to_vec() });
                    }
                }
            }
        }
        // Several asks for one ID get the same row.
        for (i, id) in ids.iter().enumerate() {
            if out[i].is_none() {
                if let Some(&j) = want.get(id) { out[i] = out[j].clone(); }
            }
        }
        out
    }

    /// Every living creature as `agents_by_id` reads one, in the runner's order (`entities.find`, live trackers).
    /// Without `carried`, rows have no amounts (a condition that reads none need not copy them).
    pub fn all_agents(&self, carried: bool) -> Vec<AgentRow> {
        match &self.backend {
            Backend::Cpu => {
                let ns = if carried { self.live.world.chem.substances.len() } else { 0 };
                self.live.world.entities.iter().map(|e| AgentRow { id: e.id, lineage: e.lineage, age: e.age, pos: e.pos, heading: e.heading, active_genes: e.genome.active_genes(), carried: e.store.amounts[..ns].to_vec() }).collect()
            }
            Backend::Gpu(r) => {
                let ns = r.ns();
                let (ent, epos, amounts) = r.entity_table();
                let m = if carried { ns } else { 0 };
                ent.chunks(emerge_gpu::pack::ENT).enumerate().map(|(k, e)| AgentRow { id: u64::from(e[0]) | (u64::from(e[1]) << 32), lineage: e[11] as usize, age: u64::from(e[10]), pos: [epos[k * 4], epos[k * 4 + 1]], heading: [epos[k * 4 + 2], epos[k * 4 + 3]], active_genes: e[5] as usize, carried: amounts[k * ns..k * ns + m].to_vec() }).collect()
            }
        }
    }

    /// The next entity ID the runner will give out: every ID below it was given to some creature.
    pub fn next_id(&self) -> u64 {
        match &self.backend {
            Backend::Cpu => self.live.world.next_id,
            Backend::Gpu(r) => r.next_id,
        }
    }

    /// Birth and death events, and how many the GPU's log dropped (`entities.get`, trackers).
    pub fn events(&self) -> Result<(std::borrow::Cow<'_, [emerge_ref::world::Event]>, u64), String> {
        self.event_list()
    }

    /// The events after the first `from` of the log, the place to read from next time, and how many the GPU's log
    /// dropped: a tracker that keeps its place reads each event once (on the GPU, only the new records come back).
    pub fn events_since(&self, from: usize) -> Result<(std::borrow::Cow<'_, [emerge_ref::world::Event]>, usize, u64), String> {
        match &self.backend {
            Backend::Cpu => { let e = &self.live.world.events; Ok((std::borrow::Cow::Borrowed(&e[from.min(e.len())..]), e.len(), 0)) }
            Backend::Gpu(r) => r.events_since(from).map(|(v, next, dropped)| (std::borrow::Cow::Owned(v), next, dropped)).ok_or_else(|| "the GPU run keeps no events".to_string()),
        }
    }

    /// The names of the substances, in index order.
    pub fn substance_names(&self) -> Vec<String> {
        self.live.world.chem.substances.iter().map(|s| s.name.clone()).collect()
    }

    /// The space: width, height, whether it wraps.
    pub fn space(&self) -> Option<(f32, f32, bool)> {
        self.live.world.space.as_ref().map(|s| (s.w, s.h, s.wrap))
    }

    /// Entities, sampled evenly: ID, lineage, age, position, active genes, amounts carried.
    pub fn entities(&self, limit: usize) -> Result<Value, String> {
        let w = &self.live.world;
        let ns = w.chem.substances.len();
        // One row per entity, from the runner's own state: the CPU world, or the GPU's entity table.
        struct Row {
            id: u64,
            lineage: usize,
            age: u64,
            pos: [f32; 2],
            active: usize,
            carried: Vec<u32>,
        }
        let rows: Vec<Row> = match &self.backend {
            Backend::Cpu => w
                .entities
                .iter()
                .map(|e| Row { id: e.id, lineage: e.lineage, age: e.age, pos: e.pos, active: e.genome.active_genes(), carried: e.store.amounts[..ns].to_vec() })
                .collect(),
            Backend::Gpu(r) => {
                let (ent, epos, amounts) = r.entity_table();
                ent.chunks(emerge_gpu::pack::ENT)
                    .enumerate()
                    .map(|(i, e)| Row {
                        id: u64::from(e[0]) | (u64::from(e[1]) << 32),
                        lineage: e[11] as usize,
                        age: u64::from(e[10]),
                        pos: [epos[i * 4], epos[i * 4 + 1]],
                        active: e[5] as usize,
                        carried: amounts[i * ns..(i + 1) * ns].to_vec(),
                    })
                    .collect()
            }
        };
        let every = (rows.len() / limit.max(1)).max(1);
        let list: Vec<Value> = rows
            .iter()
            .step_by(every)
            .take(limit)
            .map(|e| {
                let carried: serde_json::Map<String, Value> = (0..ns).filter(|&s| e.carried[s] > 0).map(|s| (w.chem.substances[s].name.clone(), json!(e.carried[s]))).collect();
                json!({"id": e.id, "lineage": self.spec.lineages[e.lineage], "age": e.age, "x": e.pos[0], "y": e.pos[1], "active_genes": e.active, "carried_mu": carried})
            })
            .collect();
        Ok(json!({"tick": self.tick(), "total": rows.len(), "entities": list}))
    }
}

/// The account lines of the upkeep substance in the order of the proposal's balance: name, sign, µ.
pub fn budget_lines(t: &[u64; emerge_ref::world::ACCOUNT_COLUMNS.len()]) -> Vec<(&'static str, i128, i128)> {
    let get = |name: &str| t[emerge_ref::world::ACCOUNT_COLUMNS.iter().position(|c| *c == name).unwrap()] as i128;
    vec![
        ("made", 1, get("pay_made_mu")), ("consumed by reactions", -1, get("pay_used_mu")), ("taken", 1, get("pay_taken_mu")),
        ("stolen in", 1, get("pay_stolen_in_mu")), ("stolen out", -1, get("pay_stolen_out_mu")), ("gifts in", 1, get("pay_gift_in_mu")),
        ("gifts out", -1, get("pay_gift_out_mu")), ("dropped", -1, get("pay_dropped_mu")), ("wasted", -1, get("pay_wasted_mu")),
        ("died with", -1, get("pay_died_with_mu")), ("inherited from parent", 1, get("pay_from_parent_mu")), ("passed to children", -1, get("pay_to_child_mu")),
        ("upkeep", -1, get("upkeep_mu")), ("action fees", -1, get("fees_mu")),
    ]
}
