//! The whole tick on the GPU with the world kept there (milestone M3, without mutation).
//!
//! The host drives the kernels and reads a few counts back each tick (record totals, list
//! lengths, survivors); the state itself never leaves the device except to be compared.

use crate::gpu::{Gpu, Size};
use crate::pack::*;
use crate::shader;
use emerge_ref::World;

/// Most buckets of the grouping sort (a power of two).
const SB_MAX: usize = 32768;
const P_NBT: usize = 50;
const P_ISX: usize = 51;
const P_ISY: usize = 52;
const P_SBCAP: usize = 57;

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

/// The element ledger audit: element totals against what the sources emitted and the sinks absorbed.
pub struct Audit {
    start: Vec<i128>,
    emitted: Vec<i128>,
    absorbed: Vec<i128>,
    /// Ticks audited, and ticks whose totals did not match.
    pub ticks: u64,
    pub drifted: u64,
    /// The largest difference seen, in µ of any element.
    pub worst: i128,
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
    extent0: Vec<u64>,
    /// Seconds spent in each phase (prep, set, move, transform, restructure), when `profile` is set.
    pub profile: Option<[f64; 5]>,
    params: Vec<u32>,
    sort_cache: std::collections::HashMap<(u8, usize, bool), Vec<(u8, wgpu::BindGroup)>>,
    /// The action fees every live gene carries (`refee` rewrites them when a swap changes them).
    fees: [Option<u32>; 8],
    parity: bool,
    // modules
    set_m: Module,
    move_m: Module,
    plan_m: Module,
    /// Workgroups along x for the world-request kernels (the most bundles any law has, over 64), and the laws.
    world_grid: (u32, u32),
    tr_m: Module,
    rs_m: Module,
    prep_m: Module,
    mut_m: Module,
    gc_m: Module,
    pool_words: usize,
    /// How many times the genome pools have been collected.
    pub collections: u32,
    gc_pending: bool,
    audit: Option<Audit>,
    comp: Vec<Vec<u32>>,
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
    flows: wgpu::Buffer,
    consts: wgpu::Buffer,
    sb_in: wgpu::Buffer,
    sb_out: wgpu::Buffer,
    sb_blocks: wgpu::Buffer,
    sb_cur: wgpu::Buffer,
    wgr1: wgpu::Buffer,
    wgr2: wgpu::Buffer,
    camt0: wgpu::Buffer,
    ind: wgpu::Buffer,
    // sized each tick
    rec: wgpu::Buffer,
    rlist: wgpu::Buffer,
    rq: wgpu::Buffer,
    list: wgpu::Buffer,
    scr: wgpu::Buffer,
    rec_cap: usize,
    rq_cap: usize,
    list_cap: usize,
}

/// Workgroups along x for the world-request kernels (the most bundles any law has, over 64), and the laws.
fn world_grid(p: &Packed, nc: usize) -> (u32, u32) {
    let nlaws = p.params[P_NLAWS];
    let most = (0..nlaws as usize).map(|l| { let w = &p.laws[l * LAW..(l + 1) * LAW]; nc as u32 * if w[0] == 0 { 1 } else { w[6] } }).max().unwrap_or(0);
    (most.div_ceil(64).max(1), nlaws.max(1))
}

/// A list with two zero words after it, so that no buffer is empty.
fn padded(v: &[u32]) -> Vec<u32> {
    v.iter().copied().chain([0u32, 0]).collect()
}

/// The parameters a hot-swap may change (`emerge_ref::swap::Config`); all others are kept.
const CONFIG_PARAMS: [usize; 12] = [P_UBASE, P_UGENE, P_USIG, P_UKEEP, P_UCARRY, P_NSINK, P_NLAWS, P_LAWTOT, P_NRLAWS, P_MAXAGE, P_MAXENT, P_STEPS];

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
        // An automaton drawn as an empty board would mislead (`proposals/field-rules.md`).
        assert!(world.states.is_empty(), "state fields run on the CPU only; the GPU runtime does not run them");
        assert!(world.creatures, "a world without creatures runs on the CPU only; the GPU runtime packs upkeep and remains");
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
        let pool_words: usize = std::env::var("EMERGE_GPU_POOL_WORDS").ok().and_then(|v| v.parse().ok()).unwrap_or(192 << 20);
        assert!(p.gw.len() < pool_words && p.mw.len() < pool_words);
        let (menus, rates_t) = pack_menus(world);
        let max_genes = world.variation.iter().flatten().map(|(_, c)| c.genes).chain(p.genes.iter().map(Vec::len)).max().unwrap_or(1).max(1);
        // The kernels hold these in fixed tables (per-entity requests, mutation scratch, the ledger sums
        // and the guard mask), so a world beyond them is refused here rather than run wrong.
        for (_, c) in world.variation.iter().flatten() {
            assert!(c.genes <= MAXG && c.signals <= NSIG && c.modules <= MAXMOD, "a lineage may grow to {} genes, {} signals, {} modules; the GPU runtime holds {MAXG}, {NSIG}, {MAXMOD}", c.genes, c.signals, c.modules);
        }
        assert!(ns <= 32, "the GPU runtime holds at most 32 substances ({ns})");
        if std::env::var_os("EMERGE_GPU_BREAK").is_some() {
            p.params[P_BECOMES] = 4; // upkeep counted as a different element: the audit must notice
        }
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
            // A world built without lineages has no rates: one row of zeros (no variation) keeps the buffer bindable.
            rates: gpu.storage_with(if rates_t.is_empty() { &[0u32; RATES][..] } else { &rates_t }),
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
            stats: zeros(2 * (p.reactions.len() / RX + 1)),
            // Small integers copied into `params` by the recorded commands: the stage and every reaction-law index.
            consts: gpu.storage_with(&(0..p.params[P_NRLAWS].max(64)).collect::<Vec<_>>()),
            flows: zeros(3 * ns * 2),
            sb_in: zeros(SB_MAX + 1),
            sb_out: zeros(SB_MAX + 1),
            sb_blocks: zeros(SB_MAX / 256 + 2),
            sb_cur: zeros(SB_MAX + 1),
            wgr1: zeros(p.params[P_LAWTOT] as usize + 1),
            wgr2: zeros(p.params[P_LAWTOT] as usize + 1),
            camt0: zeros(nc * ns + 1),
            ind: zeros(9),
            rec: zeros(1),
            rlist: zeros(1),
            rq: zeros(1),
            list: zeros(2),
            scr: zeros(2),
            rec_cap: 0,
            rq_cap: 0,
            list_cap: 0,
        };
        let common = format!("{}\n{}", shader::constants(), shader::contract());
        let set_m = module(gpu, &format!("{common}\n{}", include_str!("set.wgsl")), 13, &["main"]);
        let move_m = module(gpu, &format!("{common}\n{}\n{}", shader::cond("laws"), include_str!("move.wgsl")), 30, &["bucket_hist", "bucket_scatter", "bucket_sort", "sb_scan", "upkeep_fees", "gen_requests", "sums", "flag_over", "sums_world", "flag_world", "commit_world", "pad_list", "bitonic", "sort_local", "sort_merge_local", "share", "init_stage2", "commit", "movement"]);
        let world_grid = world_grid(&p, nc);
        let plan_m = module(gpu, include_str!("plan.wgsl"), 4, &["plan_buckets", "plan_requests"]);
        let tr_m = module(gpu, &crate::transform::shader_source(), 15, &["react_entities", "react_cells", "vent", "audit_totals", "clear_made"]);
        let rs_src = format!("{common}\n{}\n{}\n{}", include_str!("restructure.wgsl"), crate::scan::kernels("alive", "params[P_N]", "{name}_in[i]"), crate::scan::kernels("cand", "params[P_N]", "{name}_in[i]"));
        let rs_m = module(gpu, &rs_src, 30, &["mark", "finish_scans", "pad_list", "bitonic", "sort_local", "sort_merge_local", "dispose", "divide", "compact_ent", "compact_rest", "alive_local", "alive_scan_blocks", "alive_add", "cand_local", "cand_scan_blocks", "cand_add"]);
        let prep_src = format!("{}\n{}\n{}\n{}", shader::constants(), include_str!("prep.wgsl"), crate::scan::kernels("bkt", "params[P_NBT]", "atomicLoad(&{name}_in[i])"), crate::scan::kernels("gen", "params[P_N]", "{name}_in[i]"));
        let prep_m = module(gpu, &prep_src, 11, &["assign", "finish_nstart", "finish_prep", "scatter", "set_roff", "bkt_local", "bkt_scan_blocks", "bkt_add", "gen_local", "gen_scan_blocks", "gen_add"]);
        let mut_src = format!("{common}\n{}\n{}", include_str!("mutate.wgsl"), include_str!("mutate_children.wgsl"));
        let mut_m = module(gpu, &mut_src, 9, &["mutate_children"]);
        let gc_src = format!("{}\n{}\n{}\n{}", shader::constants(), include_str!("gc.wgsl"), crate::scan::kernels("gm", "params[P_N]", "{name}_in[i]"), crate::scan::kernels("gv", "params[P_N]", "{name}_in[i]"));
        let gc_m = module(gpu, &gc_src, 12, &["gc_sizes", "gc_copy", "gm_local", "gm_scan_blocks", "gm_add", "gv_local", "gv_scan_blocks", "gv_add"]);
        Resident { gpu, n, ns, nc, cap, nbt, tick: world.tick as u32, next_id: world.next_id, births: world.births, deaths: world.deaths, extent0: world.entity_extent.clone(), profile: std::env::var_os("EMERGE_GPU_PROFILE").map(|_| [0.0; 5]), params: p.params.clone(), sort_cache: Default::default(), fees: emerge_ref::swap::fees_of(&world.menus), parity: false, set_m, move_m, plan_m, world_grid, tr_m, rs_m, prep_m, mut_m, gc_m, pool_words, collections: 0, gc_pending: false, audit: None, comp: world.chem.substances.iter().map(|s| s.comp.clone()).collect(), max_genes, live_after_gc: 0, b }
    }

    fn write_param(&mut self, index: usize, value: u32) {
        self.params[index] = value;
        self.gpu.queue.write_buffer(&self.b.params, (index * 4) as u64, bytemuck::bytes_of(&value));
    }

    /// Caps the buckets of the request grouping sort (0: no cap but the built-in one). For tests: with
    /// few buckets, distinct keys share one and buckets outgrow the shared-memory tile.
    pub fn limit_buckets(&mut self, most: u32) {
        assert!(most == 0 || most.is_power_of_two());
        self.write_param(P_SBCAP, most);
    }

    fn ensure(&self,buf: &mut wgpu::Buffer, have: &mut usize, bytes: usize) {
        if *have < bytes {
            let want = bytes.next_power_of_two();
            *buf = self.gpu.storage(want);
            *have = want;
        }
    }

    /// One tick, in the order of spec §7: Set, Move, Transform, Restructure.
    pub fn tick(&mut self) {
        self.gpu.batching.set(true);
        self.tick_batched();
        self.gpu.flush();
        self.gpu.batching.set(false);
    }

    fn tick_batched(&mut self) {
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
                if let Some(p) = me.profile.as_mut() {
                    p[k] += (now - mark).as_secs_f64();
                }
                mark = now;
            }
        };
        // ---- preparation: buckets and record offsets ----
        // The record total is only known on the device, so the buffers are sized for the most the
        // genomes can hold (`max_genes` active genes each) and the kernels stop at the real total.
        let records = n * self.max_genes;
        let rq_slots = records + self.params[P_LAWTOT] as usize;
        let (mut rec, mut rlist, mut rq, mut list, mut scr) = (
            std::mem::replace(&mut self.b.rec, gpu.storage(4)),
            std::mem::replace(&mut self.b.rlist, gpu.storage(4)),
            std::mem::replace(&mut self.b.rq, gpu.storage(4)),
            std::mem::replace(&mut self.b.list, gpu.storage(4)),
            std::mem::replace(&mut self.b.scr, gpu.storage(4)),
        );
        let (mut rc, mut qc, mut lc) = (self.b.rec_cap, self.b.rq_cap, self.b.list_cap);
        let before = rc;
        self.ensure(&mut rec, &mut rc, records.max(1) * REC * 4);
        if rc != before {
            rlist = gpu.storage(rc / REC + 4);
        }
        self.ensure(&mut rq, &mut qc, records.max(1) * 8 * 4);
        let list_slots = rq_slots.max(n).next_power_of_two().max(2);
        self.ensure(&mut list, &mut lc, list_slots * 8);
        let mut sc = 0usize;
        // Twice the list: the upper half holds the draws of `share`.
        self.ensure(&mut scr, &mut sc, list_slots * 16);
        (self.b.rec, self.b.rlist, self.b.rq, self.b.list, self.b.scr) = (rec, rlist, rq, list, scr);
        if (self.b.rec_cap, self.b.rq_cap, self.b.list_cap) != (rc, qc, lc) {
            self.sort_cache.clear(); // the bind groups point at the old buffers
        }
        (self.b.rec_cap, self.b.rq_cap, self.b.list_cap) = (rc, qc, lc);
        gpu.clear(&self.b.bkt_in);
        gpu.clear(&self.b.cursor);
        {
            let b = &self.b;
            let g = gpu.group(&self.prep_m.bgl, &[&b.params, &b.ent, &b.epos, &b.bkt_in, &b.bkt_out, &b.bkt_blocks, &b.cursor, &b.norder, &b.gen_in, &b.gen_out, &b.gen_blocks]);
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
        self.run_move();
        lap(self, 2);

        // ---- Transform ----
        {
            let b = &self.b;
            let g = gpu.group(&self.tr_m.bgl, &[&b.params, &b.ent, &b.amounts, &b.camt, &b.ccap, &b.ecap, &b.rec, &b.gw, &b.dying, &b.reactions, &b.rlaws, &b.waste, &b.stats, &b.flows, &b.camt0]);
            let pipe = |name: &str| self.tr_m.pipe(name);
            gpu.dispatch(&[(pipe("react_entities"), &g, wg(n))]);
            for l in 0..self.params[P_NRLAWS] {
                gpu.copy_word(&self.b.consts, (l * 4) as u64, &self.b.params, (P_LAWIDX * 4) as u64);
                gpu.dispatch(&[(pipe("react_cells"), &g, wg(nc))]);
            }
            gpu.dispatch(&[(pipe("vent"), &g, wg(n))]);
        }

        lap(self, 3);
        // ---- Restructure ----
        self.run_restructure();
        lap(self, 4);
        if self.audit.is_some() {
            self.audit_tick();
        }
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
            &[&b.params, &b.ent, &b.epos, &b.amounts, &b.camt, &b.ccap, &b.rec, &b.rlist, &b.laws, &b.sinkacc, &b.nbr, &b.dying, &b.guards, &b.rq, &b.sum_lo, &b.sum_hi, &b.list, &b.counters, &b.gw, &b.ecap, &b.scr, sortp, &b.flows, &b.sb_in, &b.sb_out, &b.sb_blocks, &b.sb_cur, &b.wgr1, &b.wgr2, &b.camt0],
        )
    }

    fn run_move(&mut self) {
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
        gpu.clear(&self.b.wgr1);
        gpu.clear(&self.b.wgr2);
        gpu.copy_word(&self.b.consts, 0, &self.b.rlist, 0);     // no requests from entities yet
        gpu.copy_buffer(&self.b.camt, &self.b.camt0, (nc * ns * 4) as u64);
        let _ = keys;
        let plan = gpu.group(&self.plan_m.bgl, &[&self.b.params, &self.b.counters, &self.b.ind, &self.b.rlist]);
        {
            let m = &self.move_m;
            gpu.dispatch(&[(m.pipe("upkeep_fees"), &group, wg(n)), (m.pipe("gen_requests"), &group, wg(n))]);
        }
        gpu.dispatch(&[(self.plan_m.pipe("plan_requests"), &plan, 1)]);
        lap("upkeep+guards+gen");
        for stage in 0..2u32 {
            if stage == 1 {
                gpu.copy_word(&self.b.consts, 4, &self.b.params, (P_STAGE * 4) as u64);
                gpu.clear(&self.b.sum_lo);
                gpu.clear(&self.b.sum_hi);
                gpu.clear(&self.b.counters);
            }
            let (wx, wy) = self.world_grid;
            gpu.dispatch_sized(&[
                (self.move_m.pipe("sums"), &group, Size::Indirect(&self.b.ind, 24)),
                (self.move_m.pipe("sums_world"), &group, Size::Grid(wx, wy)),
                (self.move_m.pipe("flag_over"), &group, Size::Indirect(&self.b.ind, 24)),
                (self.move_m.pipe("flag_world"), &group, Size::Grid(wx, wy)),
            ]);
            // Group the oversubscribed requests (see `bucket_sort` in move.wgsl). The device sizes the work
            // from the count (`plan.wgsl`), so nothing waits for it; with no request the steps do nothing.
            let b = &self.b;
            gpu.dispatch(&[(self.plan_m.pipe("plan_buckets"), &plan, 1)]);
            gpu.clear(&self.b.sb_in);
            gpu.clear(&self.b.sb_cur);
            let mv = &self.move_m;
            let per_entry = Size::Indirect(&b.ind, 0);
            let per_bucket = Size::Indirect(&b.ind, 12);
            gpu.dispatch_sized(&[
                (mv.pipe("bucket_hist"), &group, per_entry),
                (mv.pipe("sb_scan"), &group, Size::Groups(1)),
                (mv.pipe("bucket_scatter"), &group, per_entry),
                (mv.pipe("bucket_sort"), &group, per_bucket),
                (mv.pipe("share"), &group, per_entry),
            ]);
            lap(if stage == 0 { "stage1 sort+share" } else { "stage2 sort+share" });
        }
        let (wx, wy) = self.world_grid;
        gpu.dispatch_sized(&[
            (self.move_m.pipe("commit"), &group, Size::Indirect(&self.b.ind, 24)),
            (self.move_m.pipe("commit_world"), &group, Size::Grid(wx, wy)),
            (self.move_m.pipe("movement"), &group, Size::Groups(wg(n))),
        ]);
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
            &[&b.params, &b.ent, &b.epos, &b.amounts, &b.ecap, &b.sig, &b.camt, &b.ccap, &b.rec, &b.dying, &b.gw, &b.reactions, &b.ent2, &b.epos2, &b.amounts2, &b.ecap2, &b.sig2, &b.alive_in, &b.alive_out, &b.alive_blocks, &b.cand_in, &b.cand_out, &b.cand_blocks, &b.list, &b.counters, sortp, &b.work, &b.prev_out, &b.prev2, &b.flows],
        )
    }

    fn run_restructure(&mut self) {
        let gpu = self.gpu;
        let n = self.n;
        let wg = |count: usize| (count.max(1) as u32).div_ceil(64);
        let wg256 = (n.max(1) as u32).div_ceil(256);
        let group = self.rs_group(&self.b.sortp0);
        gpu.clear(&self.b.counters);
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
        gpu.dispatch_sized(&[
            (self.rs_m.pipe("divide"), &group, Size::Groups(wg(n))),
            (self.rs_m.pipe("compact_ent"), &group, Size::threads(n * ENT)),
            (self.rs_m.pipe("compact_rest"), &group, Size::threads(n * (4 + 3 * self.ns + NSIG))),
        ]);
        // Children of lineages with variation get mutated genomes.
        if children > 0 {
            let b = &self.b;
            let g = gpu.group(&self.mut_m.bgl, &[&b.params, &b.menus, &b.rates, &b.mw, &b.ent, &b.ent2, &b.gw, &b.pool, &b.work]);
            gpu.dispatch_sized(&[(self.mut_m.pipe("mutate_children"), &g, Size::Grid((children as u32).min(32768), (children as u32).div_ceil(32768)))]);
            if !self.tick.is_multiple_of(8) {
                // The pools are checked every eighth tick (a read-back is a wait).
            } else {
            let tops: Vec<u32> = gpu.read(&self.b.pool, 2);
            assert!((tops[0] as usize) < self.pool_words - 4096 && (tops[1] as usize) < self.pool_words - 4096, "the genome pools are full ({} and {} of {} words)", tops[0], tops[1], self.pool_words);
            assert!(self.live_after_gc < self.pool_words / 2, "the live genomes fill half of the pools ({} of {} words): raise EMERGE_GPU_POOL_WORDS", self.live_after_gc, self.pool_words);
            let limit = self.pool_words / 2;
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
    fn read_flows(&self) -> Vec<Vec<u128>> {
        let w: Vec<u32> = self.gpu.read(&self.b.flows, 3 * self.ns * 2);
        (0..3).map(|k| (0..self.ns).map(|s| { let i = (k * self.ns + s) * 2; w[i] as u128 | (w[i + 1] as u128) << 32 }).collect()).collect()
    }

    /// Element totals from per-substance totals.
    fn elements(&self, by_substance: &[u128]) -> Vec<i128> {
        let ne = self.comp.iter().map(Vec::len).max().unwrap_or(0);
        (0..ne).map(|el| by_substance.iter().zip(&self.comp).map(|(&t, c)| t as i128 * *c.get(el).unwrap_or(&0) as i128).sum()).collect()
    }

    fn totals_now(&self) -> Vec<u128> {
        let wg = ((self.n.max(self.nc)) as u32).div_ceil(64);
        self.gpu.clear(&self.b.flows);
        let b = &self.b;
        let g = self.gpu.group(&self.tr_m.bgl, &[&b.params, &b.ent, &b.amounts, &b.camt, &b.ccap, &b.ecap, &b.rec, &b.gw, &b.dying, &b.reactions, &b.rlaws, &b.waste, &b.stats, &b.flows, &b.camt0]);
        self.gpu.dispatch(&[(self.tr_m.pipe("audit_totals"), &g, wg)]);
        self.read_flows().remove(2)
    }

    /// Starts auditing: the element totals now are the baseline, and every tick after this checks that
    /// totals = baseline + emitted − absorbed, per element.
    pub fn enable_audit(&mut self) {
        self.write_param(P_N, self.n as u32);
        let start = self.elements(&self.totals_now());
        let ne = start.len();
        self.audit = Some(Audit { start, emitted: vec![0; ne], absorbed: vec![0; ne], ticks: 0, drifted: 0, worst: 0 });
        self.gpu.clear(&self.b.flows);
    }

    pub fn audit(&self) -> Option<&Audit> {
        self.audit.as_ref()
    }

    fn audit_tick(&mut self) {
        let flows = self.read_flows();
        let emitted = self.elements(&flows[0]);
        let absorbed = self.elements(&flows[1]);
        self.write_param(P_N, self.n as u32);
        let now = self.elements(&self.totals_now());
        let a = self.audit.as_mut().unwrap();
        let mut bad = false;
        for el in 0..now.len() {
            a.emitted[el] += emitted[el];
            a.absorbed[el] += absorbed[el];
            let drift = now[el] - (a.start[el] + a.emitted[el] - a.absorbed[el]);
            a.worst = a.worst.max(drift.abs());
            bad |= drift != 0;
        }
        a.ticks += 1;
        a.drifted += u64::from(bad);
        self.gpu.clear(&self.b.flows);
    }

    /// Entity-reaction extent by reaction since the world began (waits for the GPU).
    pub fn extent(&self) -> Vec<u64> {
        let w: Vec<u32> = self.gpu.read(&self.b.stats, 2 * self.extent0.len());
        self.extent0.iter().enumerate().map(|(r, e)| e + (w[2 * r] as u64 | (w[2 * r + 1] as u64) << 32)).collect()
    }

    /// A hot-swap's commit on the GPU (spec §13.2 step 5): the configuration of `w`, a fresh compile
    /// of the new source, replaces the running one. Only configuration parameters are written (never
    /// the counts, tick, IDs or tracking of the run); the buffers sized by the laws are made anew,
    /// and the cached bind groups that point at them are dropped. Action fees go into the menu table
    /// and into every live genome (`refee`). The kernels do not change.
    pub fn apply_config(&mut self, w: &mut World) {
        let gpu = self.gpu;
        w.begin_tick();
        let mut p = pack(w);
        pack_move(w, &mut p);
        pack_transform(w, &mut p);
        pack_restructure(w, &mut p);
        assert_eq!((p.ns, w.cells.len()), (self.ns, self.nc), "a swap keeps the substances and cells");
        for i in CONFIG_PARAMS {
            self.write_param(i, p.params[i]);
        }
        let b = &mut self.b;
        b.laws = gpu.storage_with(&padded(&p.laws));
        b.sinkacc = gpu.storage_with(&padded(&p.sinkacc));
        b.rlaws = gpu.storage_with(&padded(&p.rlaws));
        b.consts = gpu.storage_with(&(0..p.params[P_NRLAWS].max(64)).collect::<Vec<_>>());
        b.wgr1 = gpu.storage(4 * (p.params[P_LAWTOT] as usize + 1));
        b.wgr2 = gpu.storage(4 * (p.params[P_LAWTOT] as usize + 1));
        gpu.queue.write_buffer(&b.ccap, 0, bytemuck::cast_slice(&p.ccap));
        let ecap = vec![w.inventory_cap; self.n * self.ns];
        if !ecap.is_empty() {
            gpu.queue.write_buffer(&b.ecap, 0, bytemuck::cast_slice(&ecap));
        }
        let (menus, rates) = pack_menus(w);
        gpu.queue.write_buffer(&b.menus, 0, bytemuck::cast_slice(&menus));
        if !rates.is_empty() {
            gpu.queue.write_buffer(&b.rates, 0, bytemuck::cast_slice(&rates));
        }
        self.world_grid = world_grid(&p, self.nc);
        self.sort_cache.clear();
        let fees = emerge_ref::swap::fees_of(&w.menus);
        if fees != self.fees {
            self.refee(&fees);
        }
    }

    /// A lowered inventory cap's excess, sent to its sink (`emerge_ref::swap::spill` on the CPU):
    /// every entity keeps at most `cap` µ of each substance, and the audit counts the rest as
    /// absorbed. Returns the µ moved, by substance.
    pub fn spill(&mut self, cap: u32) -> Vec<u64> {
        let (g, ns) = (self.gpu, self.ns);
        let mut amounts: Vec<u32> = g.read(&self.b.amounts, self.n * ns);
        let mut moved = vec![0u64; ns];
        for row in amounts.chunks_mut(ns) {
            for (s, a) in row.iter_mut().enumerate() {
                if *a > cap {
                    moved[s] += (*a - cap) as u64;
                    *a = cap;
                }
            }
        }
        if !amounts.is_empty() {
            g.queue.write_buffer(&self.b.amounts, 0, bytemuck::cast_slice(&amounts));
        }
        let absorbed = self.elements(&moved.iter().map(|&m| m as u128).collect::<Vec<_>>());
        if let Some(a) = self.audit.as_mut() {
            a.absorbed.iter_mut().zip(&absorbed).for_each(|(x, y)| *x += y);
        }
        moved
    }

    /// Every live genome's action fees set to `fees` (by action, in `emerge_ref::swap::ACTIONS`
    /// order), in the master and in the view copy; a copy shared by several entities is rewritten
    /// once. Genes copy their fee from the menu and never mutate it, so this keeps each gene's fee
    /// equal to its menu entry's, as `emerge_ref::swap::refee` does on the CPU.
    pub fn refee(&mut self, fees: &[Option<u32>; 8]) {
        self.fees = *fees;
        let g = self.gpu;
        let ent: Vec<u32> = g.read(&self.b.ent, self.n * ENT);
        let tops = g.read::<u32>(&self.b.pool, 2);
        let (mut mw, mut gw): (Vec<u32>, Vec<u32>) = (g.read(&self.b.mw, tops[0] as usize), g.read(&self.b.gw, tops[1] as usize));
        let fix = |w: &mut [u32]| {
            if (MOVE..=DIVIDE).contains(&w[0]) {
                if let Some(f) = fees[(w[0] - MOVE) as usize] {
                    w[if w[0] == MOVE { 9 } else { 7 }] = f;
                }
            }
        };
        let (mut views, mut masters) = (std::collections::HashSet::new(), std::collections::HashSet::new());
        for e in ent.chunks(ENT) {
            let (view, active, keep, master) = (e[4] as usize, e[5] as usize, e[6] as usize, e[12] as usize);
            if views.insert(view) {
                gw[view + keep..view + keep + active * SLOT].chunks_mut(SLOT).for_each(fix);
            }
            if masters.insert(master) {
                let (slots, keep) = (mw[master] as usize, mw[master + 2] as usize);
                let first = master + MASTER_HDR + keep;
                mw[first..first + slots * SLOT].chunks_mut(SLOT).for_each(fix);
            }
        }
        if !mw.is_empty() {
            g.queue.write_buffer(&self.b.mw, 0, bytemuck::cast_slice(&mw));
        }
        if !gw.is_empty() {
            g.queue.write_buffer(&self.b.gw, 0, bytemuck::cast_slice(&gw));
        }
    }

    /// The current balances, for a swap's preflight: cell amounts and entity amounts (by substance),
    /// and the entity IDs.
    pub fn balances(&self) -> (Vec<u32>, Vec<u32>, Vec<u64>) {
        let ent: Vec<u32> = self.gpu.read(&self.b.ent, self.n * ENT);
        let ids = ent.chunks(ENT).map(|w| w[0] as u64 | (w[1] as u64) << 32).collect();
        (self.gpu.read(&self.b.camt, self.nc * self.ns), self.gpu.read(&self.b.amounts, self.n * self.ns), ids)
    }

    /// Waits for all submitted work.
    pub fn sync(&self) {
        self.gpu.sync();
    }

    pub fn ns(&self) -> usize {
        self.ns
    }

    /// What each entity made of the tracked substance since the last `clear_made` (`Entity::made`).
    pub fn made(&self) -> Vec<u64> {
        let ent: Vec<u32> = self.gpu.read(&self.b.ent, self.n * ENT);
        ent.chunks(ENT).map(|w| w[14] as u64 | (w[15] as u64) << 32).collect()
    }

    /// Living entities by founding lineage (`lineages` of them), read from the device.
    pub fn lineage_counts(&self, lineages: usize) -> Vec<u64> {
        let ent: Vec<u32> = self.gpu.read(&self.b.ent, self.n * ENT);
        let mut out = vec![0u64; lineages];
        for e in ent.chunks(ENT) {
            if let Some(c) = out.get_mut(e[11] as usize) {
                *c += 1;
            }
        }
        out
    }

    /// Which substance `made` counts (`World::track`), from the next tick; `None` counts nothing.
    pub fn set_track(&mut self, track: Option<usize>) {
        self.write_param(P_TRACK, track.map_or(0, |t| t as u32 + 1));
    }

    /// Starts a window: every entity's `made` is 0.
    pub fn clear_made(&mut self) {
        self.write_param(P_N, self.n as u32);
        let b = &self.b;
        let g = self.gpu.group(&self.tr_m.bgl, &[&b.params, &b.ent, &b.amounts, &b.camt, &b.ccap, &b.ecap, &b.rec, &b.gw, &b.dying, &b.reactions, &b.rlaws, &b.waste, &b.stats, &b.flows, &b.camt0]);
        self.gpu.dispatch(&[(self.tr_m.pipe("clear_made"), &g, (self.n.max(1) as u32).div_ceil(64))]);
    }

    pub fn pool_tops(&self) -> (u32, u32) {
        let t: Vec<u32> = self.gpu.read(&self.b.pool, 2);
        (t[0], t[1])
    }

    /// The amounts of one cell (µ of each substance).
    pub fn cell_amounts(&self, cell: usize) -> Vec<u32> {
        self.gpu.read_at(&self.b.camt, cell * self.ns, self.ns)
    }

    /// The entity table, positions and amounts, without the pools: what a listing of creatures needs.
    pub fn entity_table(&self) -> (Vec<u32>, Vec<f32>, Vec<u32>) {
        let (n, ns, g) = (self.n, self.ns, self.gpu);
        (g.read(&self.b.ent, n * ENT), g.read(&self.b.epos, n * 4), g.read(&self.b.amounts, n * ns))
    }

    /// A living entity's lineage, age and genome, decoded from the master pool (`pack::decode_master`);
    /// `None` if no entity has that ID. Reads the entity table and one genome, not the pools.
    pub fn genome_of(&self, id: u64) -> Option<(usize, u64, emerge_ref::Genome)> {
        let ent: Vec<u32> = self.gpu.read(&self.b.ent, self.n * ENT);
        let row = ent.chunks(ENT).find(|e| u64::from(e[0]) | (u64::from(e[1]) << 32) == id)?;
        let off = row[12] as usize;
        let header: Vec<u32> = self.gpu.read_at(&self.b.mw, off, crate::pack::MASTER_HDR);
        let words: Vec<u32> = self.gpu.read_at(&self.b.mw, off, crate::pack::master_len(&header));
        Some((row[11] as usize, u64::from(row[10]), crate::pack::decode_master(&words, 0)))
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
