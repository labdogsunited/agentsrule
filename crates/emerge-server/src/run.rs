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
}

impl Run {
    /// Compiles the source (in canonical form) and starts at tick 0, revision 0. The journal's
    /// header is written at once.
    pub fn create(mut spec: Spec, mut journal: Journal) -> Result<Run, Rejected> {
        spec.contract |= spec.gpu;
        let names: Vec<&str> = spec.lineages.iter().map(String::as_str).collect();
        let mut live = Live::new(&spec.source, &names, Options { seed: spec.seed, allow_partial: true, mutation: spec.mutation })?;
        spec.source = live.source.clone();
        live.world.contract = spec.contract;
        let (backend, device) = if spec.gpu {
            let gpu: &'static Gpu = Box::leak(Box::new(Gpu::new()));
            (Backend::Gpu(Box::new(Resident::new(gpu, &mut live.world))), format!("gpu: {}", gpu.name))
        } else {
            (Backend::Cpu, "cpu: emerge-ref".to_string())
        };
        journal.header(&spec, &device);
        Ok(Run { spec, live, backend, journal, device, last_swap: None })
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

    /// Runs `n` ticks, or fewer if the world dies out. Returns the ticks run.
    pub fn step(&mut self, n: u64) -> u64 {
        let mut done = 0;
        while done < n && self.population() > 0 {
            match &mut self.backend {
                Backend::Cpu => self.live.world.run_tick(),
                Backend::Gpu(r) => r.tick(),
            }
            done += 1;
        }
        done
    }

    /// A hot-swap at the current tick boundary, journalled whatever its outcome.
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

    /// The genome of entity `id` in plain language (spec §12.1 `explain`). The CPU runner only:
    /// the GPU keeps genomes as packed words, which are not decoded back yet.
    pub fn explain(&self, id: u64) -> Result<String, String> {
        if let Backend::Gpu(_) = self.backend {
            return Err("genome.explain needs the CPU backend for now (GPU genomes are not decoded back yet)".into());
        }
        let w = &self.live.world;
        let e = w.entities.iter().find(|e| e.id == id).ok_or_else(|| format!("no living entity {id}"))?;
        let field = self.live.field.as_deref().unwrap_or("cells");
        Ok(format!("entity {id}, lineage {}, age {}:\n{}", self.spec.lineages[e.lineage], e.age, emerge_ref::print::explain(w, &e.genome, field)))
    }

    /// Entities, sampled evenly: ID, lineage, age, position, active genes, amounts carried.
    pub fn entities(&self, limit: usize) -> Result<Value, String> {
        if let Backend::Gpu(_) = self.backend {
            return Err("entities.query needs the CPU backend for now".into());
        }
        let w = &self.live.world;
        let ns = w.chem.substances.len();
        let every = (w.entities.len() / limit.max(1)).max(1);
        let list: Vec<Value> = w
            .entities
            .iter()
            .step_by(every)
            .take(limit)
            .map(|e| {
                let carried: serde_json::Map<String, Value> = (0..ns).filter(|&s| e.store.amounts[s] > 0).map(|s| (w.chem.substances[s].name.clone(), json!(e.store.amounts[s]))).collect();
                json!({"id": e.id, "lineage": self.spec.lineages[e.lineage], "age": e.age, "x": e.pos[0], "y": e.pos[1], "active_genes": e.genome.active_genes(), "carried_mu": carried})
            })
            .collect();
        Ok(json!({"tick": self.tick(), "total": w.entities.len(), "entities": list}))
    }
}
