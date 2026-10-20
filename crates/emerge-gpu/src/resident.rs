//! The whole tick on the GPU with the world kept there (milestone M3, without mutation).
//!
//! The host drives the kernels and reads a few counts back each tick (record totals, list
//! lengths, survivors); the state itself never leaves the device except to be compared.

use crate::gpu::Gpu;
use crate::pack::*;
use crate::shader;
use emerge_ref::World;

const P_NBT: usize = 50;
const P_ISX: usize = 51;
const P_ISY: usize = 52;

/// A snapshot of the device state for comparison.
pub struct Snapshot {
    pub n: usize,
    pub ns: usize,
    pub tick: u32,
    pub next_id: u64,
    pub ent: Vec<u32>,
    pub epos: Vec<f32>,
    pub amounts: Vec<u32>,
    pub ecap: Vec<u32>,
    pub sig: Vec<f32>,
    pub camt: Vec<u32>,
    pub gw: Vec<u32>,
    pub mw: Vec<u32>,
}

struct Module {
    bgl: wgpu::BindGroupLayout,
    pipes: std::collections::HashMap<String, wgpu::ComputePipeline>,
}

impl Module {
    fn pipe(&self, name: &str) -> &wgpu::ComputePipeline {
        self.pipes.get(name).unwrap_or_else(|| panic!("no pipeline `{name}`"))
    }
}

pub struct Resident<'a> {
    gpu: &'a Gpu,
    pub n: usize,
    ns: usize,
    nc: usize,
    cap: usize,
    nbt: usize,
    pub tick: u32,
    pub next_id: u64,
    /// Children born, entities removed, and entity-reaction extent by reaction, since the start.
    pub births: u64,
    pub deaths: u64,
    pub extent: Vec<u64>,
    /// Seconds spent in each phase (prep, set, move, transform, restructure), when `profile` is set.
    pub profile: Option<[f64; 5]>,
    params: Vec<u32>,
    sort_cache: std::collections::HashMap<(u8, usize, bool), Vec<(u8, wgpu::BindGroup)>>,
    parity: bool,
    // modules
    set_m: Module,
    move_m: Module,
    tr_m: Module,
    rs_m: Module,
    prep_m: Module,
    mut_m: Module,
    gc_m: Module,
    pool_words: usize,
    /// How many times the genome pools have been collected.
    pub collections: u32,
    gc_pending: bool,
    /// The most active genes a genome may hold (the largest capacity of any lineage, or of the first population).
    max_genes: usize,
    /// Words of the larger pool after the last collection: the next one waits until twice that.
    live_after_gc: usize,
    // persistent state (the second of each pair is the destination of Restructure)
    b: Buffers,
}

struct Buffers {
    params: wgpu::Buffer,
    ent: wgpu::Buffer,
    ent2: wgpu::Buffer,
    epos: wgpu::Buffer,
    epos2: wgpu::Buffer,
    amounts: wgpu::Buffer,
    amounts2: wgpu::Buffer,
    ecap: wgpu::Buffer,
    ecap2: wgpu::Buffer,
    sig: wgpu::Buffer,
    sig2: wgpu::Buffer,
    camt: wgpu::Buffer,
    ccap: wgpu::Buffer,
    gw: wgpu::Buffer,
    mw: wgpu::Buffer,
    gw2: wgpu::Buffer,
    mw2: wgpu::Buffer,
    gv_in: wgpu::Buffer,
    gv_out: wgpu::Buffer,
    gv_blocks: wgpu::Buffer,
    pool: wgpu::Buffer,
    menus: wgpu::Buffer,
    rates: wgpu::Buffer,
    work: wgpu::Buffer,
    prev: wgpu::Buffer,
    prev_out: wgpu::Buffer,
    prev2: wgpu::Buffer,
    nbr: wgpu::Buffer,
    laws: wgpu::Buffer,
    sinkacc: wgpu::Buffer,
    reactions: wgpu::Buffer,
    rlaws: wgpu::Buffer,
    waste: wgpu::Buffer,
    // per tick
    dying: wgpu::Buffer,
    guards: wgpu::Buffer,
    alive_in: wgpu::Buffer,
    alive_out: wgpu::Buffer,
    alive_blocks: wgpu::Buffer,
    cand_in: wgpu::Buffer,
    cand_out: wgpu::Buffer,
    cand_blocks: wgpu::Buffer,
    bkt_in: wgpu::Buffer,
    bkt_out: wgpu::Buffer,
    bkt_blocks: wgpu::Buffer,
    cursor: wgpu::Buffer,
    norder: wgpu::Buffer,
    gen_in: wgpu::Buffer,
    gen_out: wgpu::Buffer,
    gen_blocks: wgpu::Buffer,
    counters: wgpu::Buffer,
    sortp0: wgpu::Buffer,
    sum_lo: wgpu::Buffer,
    sum_hi: wgpu::Buffer,
    stats: wgpu::Buffer,
    // sized each tick
    rec: wgpu::Buffer,
    rec_ent: wgpu::Buffer,
    rq: wgpu::Buffer,
    list: wgpu::Buffer,
    scr: wgpu::Buffer,
    rec_cap: usize,
    rq_cap: usize,
    list_cap: usize,
}

fn pool_buffer(gpu: &Gpu, initial: &[u32], words: usize) -> wgpu::Buffer {
    let b = gpu.storage(words * 4);
    gpu.queue.write_buffer(&b, 0, bytemuck::cast_slice(initial));
    b
}

fn module(gpu: &Gpu, src: &str, bindings: u32, entries: &[&str]) -> Module {
    let (bgl, layout) = gpu.storage_layout(bindings);
    let m = gpu.module(src);
    let pipes = entries.iter().map(|e| (e.to_string(), gpu.pipeline_in(&m, e, &layout))).collect();
    Module { bgl, pipes }
}

impl<'a> Resident<'a> {
    pub fn new(gpu: &'a Gpu, world: &mut World) -> Resident<'a> {
        world.begin_tick();
        let mut p = pack(world);
        pack_move(world, &mut p);
        pack_transform(world, &mut p);
        pack_restructure(world, &mut p);
        let (n, ns) = (p.n, p.ns);
        let nc = world.cells.len();
        let space = world.space.as_ref().unwrap();
        let cap = world.max_entities.min(1 << 19).max(2 * n).max(1024);
        // Contact buckets, a little wider than the interaction radius so that a rounding in the
        // position-to-bucket step cannot hide a neighbour.
        let radius = space.radius;
        let nbx = ((space.w / (radius * 1.001)).floor() as usize).max(1);
        let nby = ((space.h / (radius * 1.001)).floor() as usize).max(1);
        let nbt = nbx * nby;
        p.params[P_NBX] = nbx as u32;
        p.params[P_NBY] = nby as u32;
        p.params[P_NBT] = nbt as u32;
        p.params[P_ISX] = (1.0 / (space.w / nbx as f32)).to_bits();
        p.params[P_ISY] = (1.0 / (space.h / nby as f32)).to_bits();
        p.params[P_RADIUS] = radius.to_bits();
        p.params[P_R2] = (radius * radius).to_bits();
        let pool_words: usize = std::env::var("EMERGE_GPU_POOL_WORDS").ok().and_then(|v| v.parse().ok()).unwrap_or(48 << 20);
        assert!(p.gw.len() < pool_words && p.mw.len() < pool_words);
        let (menus, rates_t) = pack_menus(world);
        let max_genes = world.variation.iter().flatten().map(|(_, c)| c.genes).chain(p.genes.iter().map(Vec::len)).max().unwrap_or(1).max(1);
        let ce = |words: usize| gpu.storage(cap * words * 4);
        let pad = |v: &[u32]| -> Vec<u32> { v.iter().copied().chain([0u32, 0]).collect() };
        let zeros = |count: usize| gpu.storage(count.max(1) * 4);
        let mut ent = vec![0u32; cap * ENT];
        ent[..p.ent.len()].copy_from_slice(&p.ent);
        let mut epos = vec![0f32; cap * 4];
        epos[..p.epos.len()].copy_from_slice(&p.epos);
        let mut amounts = vec![0u32; cap * ns];
        amounts[..p.amounts.len()].copy_from_slice(&p.amounts);
        let mut ecap = vec![0u32; cap * ns];
        ecap[..p.ecap.len()].copy_from_slice(&p.ecap);
        let mut sig = vec![0f32; cap * NSIG];
        sig[..p.sig.len()].copy_from_slice(&p.sig);
        let b = Buffers {
            params: gpu.storage_with(&p.params),
            ent: gpu.storage_with(&ent),
            ent2: ce(ENT),
            epos: gpu.storage_with(&epos),
            epos2: ce(4),
            amounts: gpu.storage_with(&amounts),
            amounts2: ce(ns),
            ecap: gpu.storage_with(&ecap),
            ecap2: ce(ns),
            sig: gpu.storage_with(&sig),
            sig2: ce(NSIG),
            camt: gpu.storage_with(&p.camt),
            ccap: gpu.storage_with(&p.ccap),
            gw: pool_buffer(gpu, &p.gw, pool_words),
            mw: pool_buffer(gpu, &p.mw, pool_words),
            gw2: gpu.storage(pool_words * 4),
            mw2: gpu.storage(pool_words * 4),
            gv_in: zeros(cap),
            gv_out: zeros(cap),
            gv_blocks: zeros(cap.div_ceil(256) + 1),
            pool: gpu.storage_with(&[p.mw.len() as u32, p.gw.len() as u32, 0, 0]),
            menus: gpu.storage_with(&menus),
            rates: gpu.storage_with(&rates_t),
            work: zeros(cap),
            prev: zeros(cap * ns),
            prev_out: zeros(cap * ns),
            prev2: zeros(cap * ns),
            nbr: gpu.storage_with(&p.nbr),
            laws: gpu.storage_with(&pad(&p.laws)),
            sinkacc: gpu.storage_with(&pad(&p.sinkacc)),
            reactions: gpu.storage_with(&pad(&p.reactions)),
            rlaws: gpu.storage_with(&pad(&p.rlaws)),
            waste: gpu.storage_with(&pad(&p.waste)),
            dying: zeros(cap),
            guards: zeros(cap),
            alive_in: zeros(cap),
            alive_out: zeros(cap),
            alive_blocks: zeros(cap.div_ceil(256) + 1),
            cand_in: zeros(cap),
            cand_out: zeros(cap),
            cand_blocks: zeros(cap.div_ceil(256) + 1),
            bkt_in: zeros(nbt + 1),
            bkt_out: zeros(nbt + 1),
            bkt_blocks: zeros(nbt.div_ceil(256) + 1),
            cursor: zeros(nbt + 1),
            norder: zeros(cap),
            gen_in: zeros(cap),
            gen_out: zeros(cap),
            gen_blocks: zeros(cap.div_ceil(256) + 1),
            counters: zeros(4),
            sortp0: gpu.storage_with(&[0u32, 0, 0, 0]),
            sum_lo: zeros((nc + cap) * ns),
            sum_hi: zeros((nc + cap) * ns),
            stats: zeros(p.reactions.len() / RX + 1),
            rec: zeros(1),
            rec_ent: zeros(1),
            rq: zeros(1),
            list: zeros(2),
            scr: zeros(2),
            rec_cap: 0,
            rq_cap: 0,
            list_cap: 0,
        };
        let common = format!("{}\n{}", shader::constants(), shader::contract());
        let set_m = module(gpu, &format!("{common}\n{}", include_str!("set.wgsl")), 13, &["main"]);
        let move_m = module(gpu, &format!("{common}\n{}", include_str!("move.wgsl")), 22, &["upkeep_fees", "collect_guards", "gen_requests", "sums", "flag_over", "pad_list", "bitonic", "sort_local", "sort_merge_local", "share", "init_stage2", "commit", "movement"]);
        let tr_m = module(gpu, &format!("{}\n{}", shader::constants(), include_str!("transform.wgsl")), 13, &["react_entities", "react_cells", "vent"]);
        let rs_src = format!("{common}\n{}\n{}\n{}", include_str!("restructure.wgsl"), crate::scan::kernels("alive", "params[P_N]", "{name}_in[i]"), crate::scan::kernels("cand", "params[P_N]", "{name}_in[i]"));
        let rs_m = module(gpu, &rs_src, 29, &["mark", "finish_scans", "pad_list", "bitonic", "sort_local", "sort_merge_local", "dispose", "divide", "compact", "alive_local", "alive_scan_blocks", "alive_add", "cand_local", "cand_scan_blocks", "cand_add"]);
        let prep_src = format!("{}\n{}\n{}\n{}", shader::constants(), include_str!("prep.wgsl"), crate::scan::kernels("bkt", "params[P_NBT]", "atomicLoad(&{name}_in[i])"), crate::scan::kernels("gen", "params[P_N]", "{name}_in[i]"));
        let prep_m = module(gpu, &prep_src, 12, &["assign", "finish_nstart", "finish_prep", "scatter", "set_roff", "bkt_local", "bkt_scan_blocks", "bkt_add", "gen_local", "gen_scan_blocks", "gen_add"]);
        let mut_src = format!("{common}\n{}\n{}", include_str!("mutate.wgsl"), include_str!("mutate_children.wgsl"));
        let mut_m = module(gpu, &mut_src, 9, &["mutate_children"]);
        let gc_src = format!("{}\n{}\n{}\n{}", shader::constants(), include_str!("gc.wgsl"), crate::scan::kernels("gm", "params[P_N]", "{name}_in[i]"), crate::scan::kernels("gv", "params[P_N]", "{name}_in[i]"));
        let gc_m = module(gpu, &gc_src, 12, &["gc_sizes", "gc_copy", "gm_local", "gm_scan_blocks", "gm_add", "gv_local", "gv_scan_blocks", "gv_add"]);
        Resident { gpu, n, ns, nc, cap, nbt, tick: world.tick as u32, next_id: world.next_id, births: world.births, deaths: world.deaths, extent: world.entity_extent.clone(), profile: std::env::var_os("EMERGE_GPU_PROFILE").map(|_| [0.0; 5]), params: p.params.clone(), sort_cache: Default::default(), parity: false, set_m, move_m, tr_m, rs_m, prep_m, mut_m, gc_m, pool_words, collections: 0, gc_pending: false, max_genes, live_after_gc: 0, b }
    }

    fn write_param(&mut self, index: usize, value: u32) {
        self.params[index] = value;
        self.gpu.queue.write_buffer(&self.b.params, (index * 4) as u64, bytemuck::bytes_of(&value));
    }

    fn ensure(&self, buf: &mut wgpu::Buffer, have: &mut usize, bytes: usize) {
        if *have < bytes {
            let want = bytes.next_power_of_two();
            *buf = self.gpu.storage(want);
            *have = want;
        }
    }

    /// One tick, in the order of spec §7: Set, Move, Transform, Restructure.
    pub fn tick(&mut self) {
        let gpu = self.gpu;
        let n = self.n;
        let (ns, nc) = (self.ns, self.nc);
        let wg = |count: usize| (count.max(1) as u32).div_ceil(64);
        let wg256 = |count: usize| (count.max(1) as u32).div_ceil(256);
        self.write_param(P_N, n as u32);
        self.write_param(P_TICK, self.tick);
        self.write_param(P_STAGE, 0);
        self.write_param(P_NEXTID_LO, self.next_id as u32);
        self.write_param(P_NEXTID_HI, (self.next_id >> 32) as u32);
        if self.gc_pending {
            self.collect_garbage();
            self.gc_pending = false;
        }

        let mut mark = std::time::Instant::now();
        let mut lap = |me: &mut Self, k: usize| {
            if me.profile.is_some() {
                me.gpu.sync();
                let now = std::time::Instant::now();
                me.profile.as_mut().unwrap()[k] += (now - mark).as_secs_f64();
                mark = now;
            }
        };
        // ---- preparation: buckets and record offsets ----
        // The record total is only known on the device, so the buffers are sized for the most the
        // genomes can hold (`max_genes` active genes each) and the kernels stop at the real total.
        let records = n * self.max_genes;
        let rq_slots = records + self.params[P_LAWTOT] as usize;
        let (mut rec, mut rec_ent, mut rq, mut list, mut scr) = (
            std::mem::replace(&mut self.b.rec, gpu.storage(4)),
            std::mem::replace(&mut self.b.rec_ent, gpu.storage(4)),
            std::mem::replace(&mut self.b.rq, gpu.storage(4)),
            std::mem::replace(&mut self.b.list, gpu.storage(4)),
            std::mem::replace(&mut self.b.scr, gpu.storage(4)),
        );
        let (mut rc, mut qc, mut lc) = (self.b.rec_cap, self.b.rq_cap, self.b.list_cap);
        let before = rc;
        self.ensure(&mut rec, &mut rc, records.max(1) * REC * 4);
        if rc != before {
            rec_ent = gpu.storage(rc / REC);
        }
        self.ensure(&mut rq, &mut qc, rq_slots.max(1) * 8 * 4);
        let list_slots = rq_slots.max(n).next_power_of_two().max(2);
        self.ensure(&mut list, &mut lc, list_slots * 8);
        let mut sc = 0usize;
        self.ensure(&mut scr, &mut sc, list_slots * 8);
        (self.b.rec, self.b.rec_ent, self.b.rq, self.b.list, self.b.scr) = (rec, rec_ent, rq, list, scr);
        if (self.b.rec_cap, self.b.rq_cap, self.b.list_cap) != (rc, qc, lc) {
            self.sort_cache.clear(); // the bind groups point at the old buffers
        }
        (self.b.rec_cap, self.b.rq_cap, self.b.list_cap) = (rc, qc, lc);
        gpu.clear(&self.b.bkt_in);
        gpu.clear(&self.b.cursor);
        {
            let b = &self.b;
            let g = gpu.group(&self.prep_m.bgl, &[&b.params, &b.ent, &b.epos, &b.bkt_in, &b.bkt_out, &b.bkt_blocks, &b.cursor, &b.norder, &b.gen_in, &b.gen_out, &b.gen_blocks, &b.rec_ent]);
            let pipe = |name: &str| self.prep_m.pipe(name);
            gpu.dispatch(&[
                (pipe("assign"), &g, wg(n)),
                (pipe("bkt_local"), &g, wg256(self.nbt)),
                (pipe("bkt_scan_blocks"), &g, 1),
                (pipe("bkt_add"), &g, wg256(self.nbt)),
                (pipe("finish_nstart"), &g, 1),
                (pipe("scatter"), &g, wg(n)),
                (pipe("gen_local"), &g, wg256(n)),
                (pipe("gen_scan_blocks"), &g, 1),
                (pipe("gen_add"), &g, wg256(n)),
                (pipe("finish_prep"), &g, 1),
                (pipe("set_roff"), &g, wg(n)),
            ]);
        }

        lap(self, 0);
        // ---- Set ----
        {
            let b = &self.b;
            let g = gpu.group(&self.set_m.bgl, &[&b.params, &b.ent, &b.epos, &b.amounts, &b.prev, &b.gw, &b.sig, &b.camt, &b.ccap, &b.bkt_out, &b.norder, &b.rec, &b.prev_out]);
            gpu.dispatch(&[(self.set_m.pipe("main"), &g, wg(n))]);
        }

        lap(self, 1);
        // ---- Move ----
        self.run_move(records, rq_slots);
        lap(self, 2);

        // ---- Transform ----
        {
            let b = &self.b;
            let g = gpu.group(&self.tr_m.bgl, &[&b.params, &b.ent, &b.amounts, &b.camt, &b.ccap, &b.ecap, &b.rec, &b.gw, &b.dying, &b.reactions, &b.rlaws, &b.waste, &b.stats]);
            let pipe = |name: &str| self.tr_m.pipe(name);
            gpu.dispatch(&[(&pipe("react_entities"), &g, wg(n))]);
            for l in 0..self.params[P_NRLAWS] {
                gpu.queue.write_buffer(&self.b.params, (P_LAWIDX * 4) as u64, bytemuck::bytes_of(&l));
                gpu.dispatch(&[(&pipe("react_cells"), &g, wg(nc))]);
            }
            gpu.dispatch(&[(&pipe("vent"), &g, wg(n))]);
            let nr = self.extent.len();
            let stats: Vec<u32> = gpu.read(&self.b.stats, nr);
            for (total, add) in self.extent.iter_mut().zip(stats) {
                *total += add as u64;
            }
            gpu.queue.write_buffer(&self.b.stats, 0, &vec![0u8; nr * 4]);
        }

        lap(self, 3);
        // ---- Restructure ----
        self.run_restructure();
        lap(self, 4);
        let _ = ns;
    }

    /// The bind groups of a bitonic sort of `size` list entries (a power of two): kind 0 is the local
    /// sort, 1 a global step, 2 the local tail of a merge. Cached per module, size and buffer parity.
    fn sort_groups(&mut self, which: u8, size: usize) {
        let key = (which, size, self.parity);
        if self.sort_cache.contains_key(&key) {
            return;
        }
        let mut steps: Vec<(u8, [u32; 4])> = Vec::new();
        let first = size.min(1024);
        steps.push((0, [0, 0, size as u32, first as u32]));
        let mut k = 2048;
        while k <= size {
            let mut j = k / 2;
            while j >= 1024 {
                steps.push((1, [k as u32, j as u32, size as u32, 0]));
                j /= 2;
            }
            steps.push((2, [k as u32, 0, size as u32, 0]));
            k *= 2;
        }
        let groups = steps
            .into_iter()
            .map(|(kind, words)| {
                let buf = self.gpu.storage_with(&words);
                let g = if which == 0 { self.move_group(&buf) } else { self.rs_group(&buf) };
                (kind, g)
            })
            .collect();
        self.sort_cache.insert(key, groups);
    }

    /// Sorts the first `count` entries of `list` (the rest is treated as the largest key).
    fn sort_steps<'s>(&'s self, which: u8, count: usize, module: &'s Module) -> Vec<(&'s wgpu::ComputePipeline, &'s wgpu::BindGroup, u32)> {
        let size = count.next_power_of_two().max(2);
        let groups = &self.sort_cache[&(which, size, self.parity)];
        let wg1024 = (size as u32).div_ceil(1024);
        let wg64 = (size as u32).div_ceil(64);
        groups
            .iter()
            .map(|(kind, g)| match kind {
                0 => (module.pipe("sort_local"), g, wg1024),
                1 => (module.pipe("bitonic"), g, wg64),
                _ => (module.pipe("sort_merge_local"), g, wg1024),
            })
            .collect()
    }

    fn move_group(&self, sortp: &wgpu::Buffer) -> wgpu::BindGroup {
        let b = &self.b;
        self.gpu.group(
            &self.move_m.bgl,
            &[&b.params, &b.ent, &b.epos, &b.amounts, &b.camt, &b.ccap, &b.rec, &b.rec_ent, &b.laws, &b.sinkacc, &b.nbr, &b.dying, &b.guards, &b.rq, &b.sum_lo, &b.sum_hi, &b.list, &b.counters, &b.gw, &b.ecap, &b.scr, sortp],
        )
    }

    fn run_move(&mut self, _records: usize, r: usize) {
        let gpu = self.gpu;
        let prof = self.profile.is_some();
        let t0 = std::time::Instant::now();
        let mut marks: Vec<(&str, f64)> = Vec::new();
        let mut lap = |name: &'static str| {
            if prof {
                gpu.sync();
                marks.push((name, t0.elapsed().as_secs_f64()));
            }
        };
        let n = self.n;
        let (ns, nc) = (self.ns, self.nc);
        let keys = ((nc + self.n) * ns).max(1);
        let wg = |count: usize| (count.max(1) as u32).div_ceil(64);
        let group = self.move_group(&self.b.sortp0);
        // The sums start at zero every tick.
        gpu.clear(&self.b.sum_lo);
        gpu.clear(&self.b.sum_hi);
        gpu.clear(&self.b.counters);
        let _ = keys;
        {
            let m = &self.move_m;
            gpu.dispatch(&[(m.pipe("upkeep_fees"), &group, wg(n)), (m.pipe("collect_guards"), &group, wg(n)), (m.pipe("gen_requests"), &group, wg(r))]);
        }
        lap("upkeep+guards+gen");
        for stage in 0..2u32 {
            if stage == 1 {
                gpu.queue.write_buffer(&self.b.params, (P_STAGE * 4) as u64, bytemuck::bytes_of(&1u32));
                gpu.dispatch(&[(self.move_m.pipe("init_stage2"), &group, wg(r))]);
                gpu.clear(&self.b.sum_lo);
                gpu.clear(&self.b.sum_hi);
                gpu.clear(&self.b.counters);
            }
            gpu.dispatch(&[(self.move_m.pipe("sums"), &group, wg(r)), (self.move_m.pipe("flag_over"), &group, wg(r))]);
            let m = gpu.read::<u32>(&self.b.counters, 4)[0] as usize;
            lap(if stage == 0 { "stage1 sums" } else { "stage2 sums" });
            if m > 0 {
                self.sort_groups(0, m.next_power_of_two().max(2));
                let mut steps = self.sort_steps(0, m, &self.move_m);
                steps.push((self.move_m.pipe("share"), &group, wg(m)));
                gpu.dispatch(&steps);
                lap(if stage == 0 { "stage1 sort+share" } else { "stage2 sort+share" });
            }
        }
        gpu.dispatch(&[(self.move_m.pipe("commit"), &group, wg(r)), (self.move_m.pipe("movement"), &group, wg(n))]);
        lap("commit+movement");
        if prof && std::env::var_os("EMERGE_GPU_PROFILE_MOVE").is_some() {
            let mut prev = 0.0;
            let line: Vec<String> = marks.iter().map(|(k, t)| { let d = t - prev; prev = *t; format!("{k} {:.1}", d * 1e3) }).collect();
            eprintln!("move (ms): {}", line.join(", "));
        }
    }

    /// Copies every live genome into fresh pools. The pool bind groups depend on the buffers, so the
    /// cached sort groups are dropped.
    fn collect_garbage(&mut self) {
        let gpu = self.gpu;
        let n = self.n;
        let wg256 = (n.max(1) as u32).div_ceil(256);
        let b = &self.b;
        let g = gpu.group(&self.gc_m.bgl, &[&b.params, &b.ent, &b.mw, &b.gw, &b.mw2, &b.gw2, &b.alive_in, &b.alive_out, &b.alive_blocks, &b.gv_in, &b.gv_out, &b.gv_blocks]);
        let m = &self.gc_m;
        gpu.dispatch(&[(m.pipe("gc_sizes"), &g, (n as u32).div_ceil(64))]);
        // The scans of the two size arrays use the kernels' own names: gm_* reads alive_in (bound as gm_in).
        gpu.dispatch(&[(m.pipe("gm_local"), &g, wg256), (m.pipe("gm_scan_blocks"), &g, 1), (m.pipe("gm_add"), &g, wg256)]);
        gpu.dispatch(&[(m.pipe("gv_local"), &g, wg256), (m.pipe("gv_scan_blocks"), &g, 1), (m.pipe("gv_add"), &g, wg256)]);
        let last = |inb: &wgpu::Buffer, outb: &wgpu::Buffer| -> u32 {
            let a: Vec<u32> = gpu.read_at(inb, n - 1, 1);
            let c: Vec<u32> = gpu.read_at(outb, n - 1, 1);
            a[0] + c[0]
        };
        let (tm, tv) = (last(&b.alive_in, &b.alive_out), last(&b.gv_in, &b.gv_out));
        gpu.dispatch(&[(m.pipe("gc_copy"), &g, (n as u32).div_ceil(64))]);
        gpu.queue.write_buffer(&b.pool, 0, bytemuck::cast_slice(&[tm, tv]));
        let b = &mut self.b;
        std::mem::swap(&mut b.mw, &mut b.mw2);
        std::mem::swap(&mut b.gw, &mut b.gw2);
        self.sort_cache.clear();
        self.collections += 1;
        self.live_after_gc = tm.max(tv) as usize;
    }

    fn rs_group(&self, sortp: &wgpu::Buffer) -> wgpu::BindGroup {
        let b = &self.b;
        self.gpu.group(
            &self.rs_m.bgl,
            &[&b.params, &b.ent, &b.epos, &b.amounts, &b.ecap, &b.sig, &b.camt, &b.ccap, &b.rec, &b.dying, &b.gw, &b.reactions, &b.ent2, &b.epos2, &b.amounts2, &b.ecap2, &b.sig2, &b.alive_in, &b.alive_out, &b.alive_blocks, &b.cand_in, &b.cand_out, &b.cand_blocks, &b.list, &b.counters, sortp, &b.work, &b.prev_out, &b.prev2],
        )
    }

    fn run_restructure(&mut self) {
        let gpu = self.gpu;
        let n = self.n;
        let wg = |count: usize| (count.max(1) as u32).div_ceil(64);
        let wg256 = (n.max(1) as u32).div_ceil(256);
        let group = self.rs_group(&self.b.sortp0);
        gpu.queue.write_buffer(&self.b.counters, 0, &[0u8; 16]);
        {
            let m = &self.rs_m;
            let mut steps = vec![(m.pipe("mark"), &group, wg(n))];
            for name in ["alive", "cand"] {
                steps.push((m.pipe(&format!("{name}_local")), &group, wg256));
                steps.push((m.pipe(&format!("{name}_scan_blocks")), &group, 1));
                steps.push((m.pipe(&format!("{name}_add")), &group, wg256));
            }
            steps.push((m.pipe("finish_scans"), &group, 1));
            gpu.dispatch(&steps);
        }
        // removed, survivors, division candidates, children that fit
        let c: Vec<u32> = gpu.read(&self.b.counters, 4);
        let (removed, alive, _candidates, children) = (c[0] as usize, c[1] as usize, c[2] as usize, c[3] as usize);
        if removed > 0 {
            self.sort_groups(1, removed.next_power_of_two().max(2));
            let mut steps = self.sort_steps(1, removed, &self.rs_m);
            steps.push((self.rs_m.pipe("dispose"), &group, wg(removed)));
            gpu.dispatch(&steps);
        }
        gpu.dispatch(&[(self.rs_m.pipe("divide"), &group, wg(n)), (self.rs_m.pipe("compact"), &group, wg(n))]);
        // Children of lineages with variation get mutated genomes.
        if children > 0 {
            let b = &self.b;
            let g = gpu.group(&self.mut_m.bgl, &[&b.params, &b.menus, &b.rates, &b.mw, &b.ent, &b.ent2, &b.gw, &b.pool, &b.work]);
            gpu.dispatch(&[(self.mut_m.pipe("mutate_children"), &g, wg(children))]);
            if self.tick % 8 != 0 {
                // The pools are checked every eighth tick (a read-back is a wait).
            } else {
            let tops: Vec<u32> = gpu.read(&self.b.pool, 2);
            assert!((tops[0] as usize) < self.pool_words - 4096 && (tops[1] as usize) < self.pool_words - 4096, "the genome pools are full ({} and {} of {} words)", tops[0], tops[1], self.pool_words);
            let limit = (self.pool_words / 2).max(2 * self.live_after_gc);
            if tops[0] as usize > limit || tops[1] as usize > limit {
                self.gc_pending = true;
            }
            }
        }
        assert!(alive + children <= self.cap, "more entities than the device arrays hold ({})", self.cap);
        // The destination arrays become the state.
        let b = &mut self.b;
        std::mem::swap(&mut b.ent, &mut b.ent2);
        std::mem::swap(&mut b.epos, &mut b.epos2);
        std::mem::swap(&mut b.amounts, &mut b.amounts2);
        std::mem::swap(&mut b.ecap, &mut b.ecap2);
        std::mem::swap(&mut b.sig, &mut b.sig2);
        std::mem::swap(&mut b.prev, &mut b.prev2);
        self.parity = !self.parity;
        self.n = alive + children;
        self.births += children as u64;
        self.deaths += removed as u64;
        self.next_id += children as u64;
        self.tick += 1;
    }

    /// Words used in the master and view pools.
    pub fn pool_tops(&self) -> (u32, u32) {
        let t: Vec<u32> = self.gpu.read(&self.b.pool, 2);
        (t[0], t[1])
    }

    pub fn snapshot(&self) -> Snapshot {
        let (n, ns) = (self.n, self.ns);
        let g = self.gpu;
        Snapshot {
            n,
            ns,
            tick: self.tick,
            next_id: self.next_id,
            ent: g.read(&self.b.ent, n * ENT),
            epos: g.read(&self.b.epos, n * 4),
            amounts: g.read(&self.b.amounts, n * ns),
            ecap: g.read(&self.b.ecap, n * ns),
            sig: g.read(&self.b.sig, n * NSIG),
            camt: g.read(&self.b.camt, self.nc * ns),
            gw: g.read(&self.b.gw, g.read::<u32>(&self.b.pool, 2)[1] as usize),
            mw: g.read(&self.b.mw, g.read::<u32>(&self.b.pool, 2)[0] as usize),
        }
    }
}
