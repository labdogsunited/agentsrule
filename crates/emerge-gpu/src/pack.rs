//! A `World` as flat arrays for the GPU (`proposals/gpu-runtime.md`).
//!
//! Layouts (all u32 or f32 words):
//! - `ent`, `ENT` words per entity: `[id_lo, id_hi, cell, bucket, genome_off, n_genes, n_signals,
//!   flags, out_off, in_use, age, lineage, master_off, 0, 0, 0]`; flags: bit 0 needs the neighbour list, bit 1 has a `prev`, bit 2
//!   keeps a `prev` (a `delta` gene anywhere in the genome).
//! - `epos`, 4 floats per entity: position and heading.
//! - `amounts`, `prev`: `ns` words per entity; `camt`, `ccap`: `ns` words per cell.
//! - `gw`: per entity `n_signals` keep values, then `SLOT` words per active gene.
//! - `sig`: `NSIG` floats per entity.
//! - `params`: see the `P_*` indices.

use emerge_ref::mind::{Curve, Gene, Input, Sense};
use emerge_ref::World;

pub const ENT: usize = 16;
pub const SLOT: usize = 18;
pub const NSIG: usize = 32;
pub const MAXG: usize = 48;
/// Words of a slot: the gene's own (0 to 15), then the module and expression bit in a master genome.
pub const NONE: u32 = u32::MAX;
/// Records per active gene in the Set output.
pub const REC: usize = 8;

// Gene kinds. Senses 0 to 7, regulation 8 and 9, then the genes that read a signal.
pub const SENSE_INVENTORY: u32 = 0;
pub const SENSE_CELL: u32 = 1;
pub const SENSE_GRADIENT: u32 = 2;
pub const SENSE_PROBE: u32 = 3;
pub const SENSE_CROWD: u32 = 4;
pub const SENSE_NEIGHBOUR: u32 = 5;
pub const SENSE_CLOCK: u32 = 6;
pub const SENSE_DELTA: u32 = 7;
pub const WEIGH: u32 = 8;
pub const PRODUCT: u32 = 9;
pub const REACT: u32 = 10;
pub const MOVE: u32 = 11;
pub const TURN: u32 = 12;
pub const TAKE: u32 = 13;
pub const DROP: u32 = 14;
pub const GIVE: u32 = 15;
pub const STEAL: u32 = 16;
pub const GUARD: u32 = 17;
pub const DIVIDE: u32 = 18;

// Parameter words.
pub const P_N: usize = 0;
pub const P_NS: usize = 1;
pub const P_TICK: usize = 2;
pub const P_STEPS: usize = 3;
pub const P_ONE: usize = 4;
pub const P_GW: usize = 5;
pub const P_GH: usize = 6;
pub const P_WRAP: usize = 7;
pub const P_NBX: usize = 8;
pub const P_NBY: usize = 9;
pub const P_W: usize = 10;
pub const P_H: usize = 11;
pub const P_R2: usize = 12;
pub const P_RADIUS: usize = 13;
pub const P_KX: usize = 14;
pub const P_KY: usize = 15;
pub const P_CKX: usize = 16;
pub const P_CKY: usize = 17;
pub const P_UNIT: usize = 18;
pub const P_COUNT: usize = 64;

// Parameter words used by the Move phase.
pub const P_PAY: usize = 24;
pub const P_UBASE: usize = 25;
pub const P_UGENE: usize = 26;
pub const P_USIG: usize = 27;
pub const P_UKEEP: usize = 28;
pub const P_UCARRY: usize = 29;
pub const P_SEEDLO: usize = 30;
pub const P_SEEDHI: usize = 31;
pub const P_NCELLS: usize = 32;
pub const P_NLAWS: usize = 33;
pub const P_R0: usize = 34;
pub const P_RTOT: usize = 35;
pub const P_NSINK: usize = 36;
pub const P_LAWTOT: usize = 39;
pub const P_NRLAWS: usize = 37;
pub const P_NWASTE: usize = 38;
pub const P_BECOMES: usize = 55;
pub const P_STAGE: usize = 48;
pub const P_LAWIDX: usize = 49;
pub const P_MAXAGE: usize = 40;
pub const P_MAXENT: usize = 41;
pub const P_NEXTID_LO: usize = 42;
pub const P_NEXTID_HI: usize = 43;
pub const P_DECOMP: usize = 44;
pub const P_CHILDOFF: usize = 45;
pub const P_NALIVE: usize = 46;
/// The substance counted in `Entity::made` (words 14 and 15 of an entity), plus 1; 0 for none.
pub const P_TRACK: usize = 58;
pub const RX: usize = 24;
pub const LAW: usize = 28;

/// Output record codes (word 0). 0 is no intent.
pub const R_MOVE: u32 = 1;
pub const R_TURN: u32 = 2;
pub const R_TAKE: u32 = 3;
pub const R_DROP: u32 = 4;
pub const R_GIVE: u32 = 5;
pub const R_STEAL: u32 = 6;
pub const R_GUARD: u32 = 7;
pub const R_DIVIDE: u32 = 8;
pub const R_REACT: u32 = 9;

pub struct Packed {
    pub n: usize,
    pub ns: usize,
    pub ent: Vec<u32>,
    pub epos: Vec<f32>,
    pub amounts: Vec<u32>,
    pub prev: Vec<u32>,
    pub gw: Vec<u32>,
    /// The master genomes (every slot, with module and expression bits), end to end.
    pub mw: Vec<u32>,
    pub sig: Vec<f32>,
    pub camt: Vec<u32>,
    pub ccap: Vec<u32>,
    pub nstart: Vec<u32>,
    pub norder: Vec<u32>,
    pub params: Vec<u32>,
    /// Total records in the Set output (`REC` words each).
    pub records: usize,
    /// Per entity, the original slot index of each packed (active) gene.
    pub genes: Vec<Vec<usize>>,
    // Move phase.
    pub ecap: Vec<u32>,
    pub laws: Vec<u32>,
    pub sinkacc: Vec<u32>,
    pub nbr: Vec<u32>,
    /// The entity of each record.
    // Transform.
    pub reactions: Vec<u32>,
    pub rlaws: Vec<u32>,
    pub waste: Vec<u32>,
}

fn input(i: Input) -> (u32, u32) {
    match i {
        Input::Signal(s) => (s as u32, 0),
        Input::Const(c) => (NONE, c.to_bits()),
    }
}

/// The packed words of one active gene, and its kind.
fn slot(g: usize, gene: &Gene) -> [u32; SLOT] {
    let mut w = [0u32; SLOT];
    w[1] = g as u32;
    w[2] = NONE;
    w[3] = NONE;
    let action = |w: &mut [u32; SLOT], kind: u32, src: Input, threshold: f32, fee: u32| {
        w[0] = kind;
        (w[2], w[3]) = input(src);
        w[4] = threshold.to_bits();
        w[7] = fee;
    };
    match *gene {
        Gene::Sense { sense, out, out2 } => {
            w[2] = out as u32;
            w[3] = out2.map_or(NONE, |o| o as u32);
            match sense {
                Sense::Inventory(s) => (w[0], w[5]) = (SENSE_INVENTORY, s as u32),
                Sense::Cell(s) => (w[0], w[5]) = (SENSE_CELL, s as u32),
                Sense::Gradient(s) => (w[0], w[5]) = (SENSE_GRADIENT, s as u32),
                Sense::Delta(s) => (w[0], w[5]) = (SENSE_DELTA, s as u32),
                Sense::Probe { s, angle_deg, dist } => {
                    (w[0], w[5], w[4], w[6]) = (SENSE_PROBE, s as u32, angle_deg.to_bits(), dist.to_bits())
                }
                Sense::Crowd { radius } => (w[0], w[4]) = (SENSE_CROWD, radius.to_bits()),
                Sense::Neighbour { s, rank } => (w[0], w[5], w[6]) = (SENSE_NEIGHBOUR, s as u32, rank),
                Sense::Clock { period } => (w[0], w[6]) = (SENSE_CLOCK, period),
            }
        }
        Gene::Weigh { bias, curve, ref inputs, out } => {
            assert!(inputs.len() <= 4, "the GPU kernel takes at most four inputs per weigh gene");
            w[0] = WEIGH;
            w[2] = out as u32;
            w[4] = bias.to_bits();
            w[5] = match curve {
                Curve::Linear => 0,
                Curve::Step => 1,
                Curve::Sigmoid => 2,
                Curve::Tanh => 3,
            };
            w[6] = inputs.len() as u32;
            for (k, &(wt, s)) in inputs.iter().enumerate() {
                w[8 + 2 * k] = wt.to_bits();
                w[9 + 2 * k] = s as u32;
            }
        }
        Gene::Product { a, b, out } => {
            w[0] = PRODUCT;
            w[2] = out as u32;
            w[8] = a as u32;
            w[9] = b as u32;
        }
        Gene::React { reaction, rate, speed } => {
            action(&mut w, REACT, speed, 0.0, 0);
            w[5] = reaction as u32;
            w[8] = rate;
        }
        Gene::Move { speed, fee_per_distance, src, threshold } => {
            action(&mut w, MOVE, src, threshold, 0);
            w[8] = speed.to_bits();
            w[9] = fee_per_distance;
        }
        Gene::Turn { angle_deg, fee, src, threshold } => {
            action(&mut w, TURN, src, threshold, fee);
            w[8] = angle_deg.to_bits();
        }
        Gene::Take { s, max, fee, src, threshold } => {
            action(&mut w, TAKE, src, threshold, fee);
            (w[5], w[8]) = (s as u32, max);
        }
        Gene::Drop { s, max, fee, src, threshold } => {
            action(&mut w, DROP, src, threshold, fee);
            (w[5], w[8]) = (s as u32, max);
        }
        Gene::Give { s, max, fee, src, threshold } => {
            action(&mut w, GIVE, src, threshold, fee);
            (w[5], w[8]) = (s as u32, max);
        }
        Gene::Steal { s, max, fee, src, threshold } => {
            action(&mut w, STEAL, src, threshold, fee);
            (w[5], w[8]) = (s as u32, max);
        }
        Gene::Guard { s, fee, src, threshold } => {
            action(&mut w, GUARD, src, threshold, fee);
            w[5] = s as u32;
        }
        Gene::Divide { split_bp, requires: (rs, ramount), fee, src, threshold } => {
            action(&mut w, DIVIDE, src, threshold, fee);
            (w[5], w[8], w[9]) = (split_bp, rs as u32, ramount);
        }
    }
    w
}

/// A label for a packed gene kind, for coverage counts.
pub fn kind_label(w: &[u32; SLOT]) -> String {
    let names = [
        "sense inventory", "sense cell", "sense gradient", "sense probe", "sense crowd", "sense neighbour", "sense clock",
        "sense delta", "weigh", "product", "react", "move", "turn", "take", "drop", "give", "steal", "guard", "divide",
    ];
    if w[0] == WEIGH {
        return format!("weigh {}", ["linear", "step", "sigmoid", "tanh"][w[5] as usize]);
    }
    names[w[0] as usize].to_string()
}

pub fn pack(world: &World) -> Packed {
    let space = world.space.as_ref().expect("the GPU runtime needs a Continuous2D space");
    let ns = world.chem.substances.len();
    let n = world.entities.len();
    let (buckets, index_radius, start, order) = world.near_index().raw();
    let mut p = Packed {
        n,
        ns,
        ent: Vec::with_capacity(n * ENT),
        epos: Vec::with_capacity(n * 4),
        amounts: Vec::with_capacity(n * ns),
        prev: vec![0; n * ns],
        gw: Vec::new(),
        mw: Vec::new(),
        sig: vec![0.0; n * NSIG],
        camt: Vec::with_capacity(world.cells.len() * ns),
        ccap: Vec::with_capacity(world.cells.len() * ns),
        nstart: start.to_vec(),
        norder: order.to_vec(),
        params: vec![0; P_COUNT],
        records: 0,
        genes: Vec::with_capacity(n),
        ecap: Vec::with_capacity(n * ns),
        laws: Vec::new(),
        sinkacc: Vec::new(),
        nbr: Vec::new(),
        reactions: Vec::new(),
        rlaws: Vec::new(),
        waste: Vec::new(),
    };
    for (i, e) in world.entities.iter().enumerate() {
        let g = &e.genome;
        assert!(g.keep.len() <= NSIG, "more than {NSIG} signals");
        let active: Vec<usize> = (0..g.slots.len()).filter(|&k| g.active(k)).collect();
        assert!(active.len() <= MAXG, "more than {MAXG} active genes");
        let needs_near = active.iter().any(|&k| matches!(g.slots[k].gene, Gene::Sense { sense: Sense::Neighbour { .. }, .. } | Gene::Give { .. } | Gene::Steal { .. }));
        let has_delta = g.slots.iter().any(|s| matches!(s.gene, Gene::Sense { sense: Sense::Delta(_), .. }));
        let flags = u32::from(needs_near) | (u32::from(!e.prev.is_empty()) << 1) | (u32::from(has_delta) << 2);
        let goff = p.gw.len() as u32;
        let in_use = e.shape.in_use.iter().fold(0u32, |m, &s| m | (1 << s));
        p.ent.extend_from_slice(&[e.id as u32, (e.id >> 32) as u32, e.cell as u32, space_bucket(world, e.pos) as u32, goff, active.len() as u32, g.keep.len() as u32, flags, p.records as u32 * REC as u32, in_use, e.age as u32, e.lineage as u32, p.mw.len() as u32, 0, e.made as u32, (e.made >> 32) as u32]);
        p.mw.extend(master_words(&e.genome));
        p.ecap.extend_from_slice(&e.store.caps[..ns]);
        p.epos.extend_from_slice(&[e.pos[0], e.pos[1], e.heading[0], e.heading[1]]);
        p.amounts.extend_from_slice(&e.store.amounts[..ns]);
        for (k, v) in e.prev.iter().take(ns).enumerate() {
            p.prev[i * ns + k] = *v;
        }
        p.gw.extend(g.keep.iter().map(|k| k.to_bits()));
        for &k in &active {
            p.gw.extend_from_slice(&slot(k, &g.slots[k].gene));
        }
        for (k, s) in e.signals.iter().enumerate() {
            p.sig[i * NSIG + k] = *s;
        }
        p.records += active.len();
        p.genes.push(active);
    }
    for c in &world.cells {
        p.camt.extend_from_slice(&c.amounts[..ns]);
        p.ccap.extend_from_slice(&c.caps[..ns]);
    }
    let r = space.radius.min(index_radius);
    let f = |x: f32| x.to_bits();
    let set = |p: &mut Packed, i: usize, v: u32| p.params[i] = v;
    set(&mut p, P_N, n as u32);
    set(&mut p, P_NS, ns as u32);
    set(&mut p, P_TICK, u32::try_from(world.tick).expect("tick fits in u32"));
    set(&mut p, P_STEPS, world.mind_steps);
    set(&mut p, P_ONE, u32::from(world.one_per_substance));
    set(&mut p, P_TRACK, world.track.map_or(0, |t| t as u32 + 1));
    set(&mut p, P_GW, space.grid_w as u32);
    set(&mut p, P_GH, space.grid_h as u32);
    set(&mut p, P_WRAP, u32::from(space.wrap));
    set(&mut p, P_NBX, buckets[0] as u32);
    set(&mut p, P_NBY, buckets[1] as u32);
    set(&mut p, P_W, f(space.w));
    set(&mut p, P_H, f(space.h));
    set(&mut p, P_R2, f(r * r));
    set(&mut p, P_RADIUS, f(index_radius));
    set(&mut p, P_KX, f(0.001f32 * (space.grid_w as f32 / (2.0 * space.w))));
    set(&mut p, P_KY, f(0.001f32 * (space.grid_h as f32 / (2.0 * space.h))));
    set(&mut p, P_CKX, f(space.grid_w as f32 / space.w));
    set(&mut p, P_CKY, f(space.grid_h as f32 / space.h));
    set(&mut p, P_UNIT, f(0.001));
    p
}

fn space_bucket(world: &World, pos: [f32; 2]) -> usize {
    world.near_index().bucket(pos)
}

/// The Move phase's inputs beyond `pack`: upkeep, the world's rules as a table of requests,
/// the sinks and the cell neighbours.
pub fn pack_move(world: &World, p: &mut Packed) {
    use emerge_ref::Law;
    let ns = p.ns;
    let nc = world.cells.len();
    let u = &world.upkeep;
    let set = |p: &mut Packed, i: usize, v: u32| p.params[i] = v;
    set(p, P_PAY, u.pay as u32);
    set(p, P_BECOMES, u.becomes as u32);
    set(p, P_UBASE, u.base);
    set(p, P_UGENE, u.per_gene);
    set(p, P_USIG, u.per_signal);
    set(p, P_UKEEP, u.per_signal_keep);
    set(p, P_UCARRY, u.per_carried_u);
    set(p, P_SEEDLO, world.seed as u32);
    set(p, P_SEEDHI, (world.seed >> 32) as u32);
    set(p, P_NCELLS, nc as u32);
    set(p, P_NSINK, world.sinks.len() as u32);
    // Request slots: one per record, then each law's range (cells × width), in law order.
    let r0 = p.records as u32;
    let mut off = 0u32;
    p.laws.clear();
    let mut nlaws = 0;
    for law in &world.laws {
        let mut w = [0u32; LAW];
        match law {
            Law::Inflow { s, source, amount, overflow } => {
                w[0] = 0;
                w[1] = *s as u32;
                w[2] = *source as u32;
                w[3] = *amount;
                w[4] = overflow.map_or(NONE, |o| o as u32);
                w[8] = 1;
            }
            Law::Diffuse { substances, rate_bp } => {
                assert!(substances.len() <= 16);
                w[0] = 1;
                w[5] = *rate_bp;
                w[6] = substances.len() as u32;
                for (k, &s) in substances.iter().enumerate() {
                    w[9 + k] = s as u32;
                }
                w[8] = 4 * substances.len() as u32;
            }
            Law::React { .. } => continue,
        }
        w[7] = off;
        off += w[8] * nc as u32;
        p.laws.extend_from_slice(&w);
        nlaws += 1;
    }
    set(p, P_NLAWS, nlaws);
    set(p, P_R0, r0);
    set(p, P_RTOT, r0 + off);
    set(p, P_LAWTOT, off);
    p.sinkacc.clear();
    for sink in &world.sinks {
        for s in 0..ns {
            p.sinkacc.push(u32::from(sink.accepts(s)));
        }
    }
    p.nbr.clear();
    for c in 0..nc {
        let nb = &world.neighbours[c];
        assert!(nb.len() <= 4);
        for k in 0..4 {
            p.nbr.push(nb.get(k).map_or(NONE, |&x| x as u32));
        }
    }
}

/// The Transform phase's inputs: the reaction table, the world's reaction rules and the waste list.
pub fn pack_transform(world: &World, p: &mut Packed) {
    use emerge_ref::Law;
    p.reactions.clear();
    for r in &world.chem.reactions {
        assert!(r.inputs.len() <= 4 && r.outputs.len() <= 4, "at most four inputs and four outputs per reaction");
        let mut w = [0u32; RX];
        w[0] = r.inputs.len() as u32;
        w[1] = r.outputs.len() as u32;
        for (k, &(s, c)) in r.inputs.iter().enumerate() {
            (w[2 + 2 * k], w[3 + 2 * k]) = (s as u32, c);
        }
        for (k, &(s, c)) in r.outputs.iter().enumerate() {
            (w[10 + 2 * k], w[11 + 2 * k]) = (s as u32, c);
        }
        // The catalyst: substance + 1 (0 for none), `half`, and `boost − 1` in units of 2^-16.
        if let Some(k) = r.catalyst {
            (w[18], w[19], w[20]) = (k.s as u32 + 1, k.half, emerge_ref::contract::catalyst_k(k.boost));
        }
        p.reactions.extend_from_slice(&w);
    }
    p.rlaws.clear();
    for law in &world.laws {
        if let Law::React { reaction, rate_bp } = law {
            p.rlaws.extend_from_slice(&[*reaction as u32, *rate_bp]);
        }
    }
    p.waste = world.waste.iter().map(|&(s, _)| s as u32).collect();
    p.params[P_NRLAWS] = (p.rlaws.len() / 2) as u32;
    p.params[P_NWASTE] = p.waste.len() as u32;
}

/// Restructure's inputs: the age limit, the entity limit, the next ID, `decompose`, the child offset.
pub fn pack_restructure(world: &World, p: &mut Packed) {
    let space = world.space.as_ref().unwrap();
    p.params[P_MAXAGE] = world.max_age.min(u32::MAX as u64) as u32;
    p.params[P_MAXENT] = world.max_entities.min(u32::MAX as usize) as u32;
    p.params[P_NEXTID_LO] = world.next_id as u32;
    p.params[P_NEXTID_HI] = (world.next_id >> 32) as u32;
    p.params[P_DECOMP] = world.decompose.map_or(NONE, |r| r as u32);
    p.params[P_CHILDOFF] = space.child_offset.to_bits();
}

// ---------- mutation (milestone M4) ----------

/// Words before the keep values in a master genome: counts and the module expression bits.
pub const MASTER_HDR: usize = 20;
/// Words of one lineage's mutation rates and capacity.
pub const RATES: usize = 16;
pub const MAXMOD: usize = 16;

// The menu table (see `pack_menus`).
pub const MN_SUBS: usize = 0; // count, then up to 16
pub const MN_FIELD: usize = 17;
pub const MN_FLAGS: usize = 34;
pub const MN_PROBE: usize = 35; // angle lo, hi, dist lo, hi
pub const MN_CROWD: usize = 39;
pub const MN_NEIGH: usize = 41;
pub const MN_CLOCK: usize = 43;
pub const MN_WEIGH: usize = 45; // bias lo, hi, weight lo, hi, inputs min, max, n curves, curves[4]
pub const MN_ACT: usize = 56; // eight menus of ACTW words: range lo, hi, fee, n substances, up to 16
pub const ACTW: usize = 21;
pub const MN_DIV_REQ: usize = 224; // substance, amount (after the eight action menus)
pub const MN_KEEP: usize = 226; // 0.0, 1.0: the range of a keep value
pub const MN_REACT: usize = 228; // count, then (reaction, max) pairs
// Flag bits in MN_FLAGS.
pub const F_INVENTORY: u32 = 1 << 0;
pub const F_DELTA: u32 = 1 << 1;
pub const F_GRADIENT: u32 = 1 << 2;
pub const F_PRODUCT: u32 = 1 << 3;
pub const F_PROBE: u32 = 1 << 4;
pub const F_CROWD: u32 = 1 << 5;
pub const F_NEIGH: u32 = 1 << 6;
pub const F_CLOCK: u32 = 1 << 7;
pub const F_WEIGH: u32 = 1 << 8;
// Action menus, in this order from MN_ACT: move, turn, take, drop, give, steal, guard, divide.
pub const A_MOVE: usize = 0;
pub const A_TURN: usize = 1;
pub const A_TAKE: usize = 2;
pub const A_DROP: usize = 3;
pub const A_GIVE: usize = 4;
pub const A_STEAL: usize = 5;
pub const A_GUARD: usize = 6;
pub const A_DIVIDE: usize = 7;
pub const ACTION_PRESENT: usize = 20; // word of an action menu: 1 if the menu exists

/// The master words of a genome: every slot (active or not) in module order, with each slot's
/// module and expression bit, the module expression bits and the keep values.
pub fn master_words(g: &emerge_ref::Genome) -> Vec<u32> {
    assert!(g.modules.len() <= MAXMOD && g.keep.len() <= NSIG && g.slots.len() <= MAXG);
    let mut w = vec![0u32; MASTER_HDR];
    w[0] = g.slots.len() as u32;
    w[1] = g.modules.len() as u32;
    w[2] = g.keep.len() as u32;
    for (m, &e) in g.modules.iter().enumerate() {
        w[4 + m] = u32::from(e);
    }
    w.extend(g.keep.iter().map(|k| k.to_bits()));
    for s in &g.slots {
        let mut sw = slot(0, &s.gene);
        sw[16] = s.module as u32;
        sw[17] = u32::from(s.expressed);
        w.extend_from_slice(&sw);
    }
    w
}

/// The view words of a genome (what the Set, Move and Transform kernels read): the keep values and
/// the active genes only, with each gene's index in the full list.
pub fn view_words(g: &emerge_ref::Genome) -> Vec<u32> {
    let mut w: Vec<u32> = g.keep.iter().map(|k| k.to_bits()).collect();
    for k in (0..g.slots.len()).filter(|&k| g.active(k)) {
        w.extend_from_slice(&slot(k, &g.slots[k].gene));
    }
    w
}

fn action_menu(t: &mut [u32], a: Option<&emerge_ref::mutate::ActionMenu>) {
    if let Some(a) = a {
        assert!(a.substances.len() <= 16);
        t[0] = a.range.0.to_bits();
        t[1] = a.range.1.to_bits();
        t[2] = a.fee;
        t[3] = a.substances.len() as u32;
        for (k, &s) in a.substances.iter().enumerate() {
            t[4 + k] = s as u32;
        }
        t[ACTION_PRESENT] = 1;
    }
}

/// The world's menus as a flat table, and each lineage's rates and capacity.
pub fn pack_menus(world: &World) -> (Vec<u32>, Vec<u32>) {
    let m = &world.menus;
    let mut t = vec![0u32; MN_REACT + 1 + 2 * m.reactions.len() + 2];
    let list = |t: &mut [u32], base: usize, xs: &[usize]| {
        assert!(xs.len() <= 16);
        t[base] = xs.len() as u32;
        for (k, &x) in xs.iter().enumerate() {
            t[base + 1 + k] = x as u32;
        }
    };
    list(&mut t, MN_SUBS, &m.substances);
    list(&mut t, MN_FIELD, &m.field);
    let mut flags = 0u32;
    for (on, f) in [
        (m.inventory, F_INVENTORY), (m.delta, F_DELTA), (m.gradient, F_GRADIENT), (m.product, F_PRODUCT), (m.probe.is_some(), F_PROBE),
        (m.crowd.is_some(), F_CROWD), (m.neighbour.is_some(), F_NEIGH), (m.clock.is_some(), F_CLOCK), (m.weigh.is_some(), F_WEIGH),
    ] {
        if on {
            flags |= f;
        }
    }
    t[MN_FLAGS] = flags;
    let range = |t: &mut [u32], at: usize, r: (f32, f32)| {
        t[at] = r.0.to_bits();
        t[at + 1] = r.1.to_bits();
    };
    if let Some((a, d)) = m.probe {
        range(&mut t, MN_PROBE, a);
        range(&mut t, MN_PROBE + 2, d);
    }
    if let Some(r) = m.crowd { range(&mut t, MN_CROWD, r); }
    if let Some(r) = m.neighbour { range(&mut t, MN_NEIGH, r); }
    if let Some(r) = m.clock { range(&mut t, MN_CLOCK, r); }
    if let Some(w) = &m.weigh {
        range(&mut t, MN_WEIGH, w.bias);
        range(&mut t, MN_WEIGH + 2, w.weight);
        t[MN_WEIGH + 4] = w.inputs.0 as u32;
        t[MN_WEIGH + 5] = w.inputs.1 as u32;
        assert!(w.curves.len() <= 4);
        t[MN_WEIGH + 6] = w.curves.len() as u32;
        for (k, c) in w.curves.iter().enumerate() {
            t[MN_WEIGH + 7 + k] = match c {
                Curve::Linear => 0,
                Curve::Step => 1,
                Curve::Sigmoid => 2,
                Curve::Tanh => 3,
            };
        }
    }
    let acts = [m.moves.as_ref(), m.turn.as_ref(), m.take.as_ref(), m.drop.as_ref(), m.give.as_ref(), m.steal.as_ref(), m.guard.as_ref(), m.divide.as_ref().map(|d| &d.0)];
    for (i, a) in acts.into_iter().enumerate() {
        action_menu(&mut t[MN_ACT + i * ACTW..MN_ACT + (i + 1) * ACTW], a);
    }
    if let Some((_, (s, amount))) = &m.divide {
        t[MN_DIV_REQ] = *s as u32;
        t[MN_DIV_REQ + 1] = *amount;
    }
    t[MN_KEEP] = 0.0f32.to_bits();
    t[MN_KEEP + 1] = 1.0f32.to_bits();
    t[MN_REACT] = m.reactions.len() as u32;
    for (k, &(r, max)) in m.reactions.iter().enumerate() {
        t[MN_REACT + 1 + 2 * k] = r as u32;
        t[MN_REACT + 2 + 2 * k] = max;
    }
    let mut rates = Vec::new();
    for v in &world.variation {
        let mut w = [0u32; RATES];
        if let (Some((r, cap)), true) = (v, world.mutation) {
            for (k, x) in [r.rate, r.param_sigma, r.expression_flip, r.rewire, r.diverge, r.duplicate_gene, r.insert_gene, r.delete_gene, r.duplicate_module, r.delete_module, r.split_module, r.merge_modules].into_iter().enumerate() {
                w[k] = x.to_bits();
            }
            (w[12], w[13], w[14], w[15]) = (cap.genes as u32, cap.signals as u32, cap.modules as u32, 1);
        }
        rates.extend_from_slice(&w);
    }
    (t, rates)
}

// ---------- reading a genome back ----------

fn bits(w: u32) -> f32 {
    f32::from_bits(w)
}

/// What a packed gene reads: a signal, or the constant in the next word.
fn input_at(w: &[u32]) -> Input {
    if w[2] == NONE {
        Input::Const(bits(w[3]))
    } else {
        Input::Signal(w[2] as usize)
    }
}

/// The gene in a packed slot (the inverse of `slot`). `w` is the `SLOT` words of one slot.
pub fn unslot(w: &[u32]) -> Gene {
    let (src, threshold) = (input_at(w), bits(w[4]));
    match w[0] {
        SENSE_INVENTORY | SENSE_CELL | SENSE_GRADIENT | SENSE_PROBE | SENSE_CROWD | SENSE_NEIGHBOUR | SENSE_CLOCK | SENSE_DELTA => {
            let (out, out2) = (w[2] as usize, if w[3] == NONE { None } else { Some(w[3] as usize) });
            let s = w[5] as usize;
            let sense = match w[0] {
                SENSE_INVENTORY => Sense::Inventory(s),
                SENSE_CELL => Sense::Cell(s),
                SENSE_GRADIENT => Sense::Gradient(s),
                SENSE_DELTA => Sense::Delta(s),
                SENSE_PROBE => Sense::Probe { s, angle_deg: bits(w[4]), dist: bits(w[6]) },
                SENSE_CROWD => Sense::Crowd { radius: bits(w[4]) },
                SENSE_NEIGHBOUR => Sense::Neighbour { s, rank: w[6] },
                _ => Sense::Clock { period: w[6] },
            };
            Gene::Sense { sense, out, out2 }
        }
        WEIGH => Gene::Weigh {
            bias: bits(w[4]),
            curve: [Curve::Linear, Curve::Step, Curve::Sigmoid, Curve::Tanh][w[5] as usize],
            inputs: (0..w[6] as usize).map(|k| (bits(w[8 + 2 * k]), w[9 + 2 * k] as usize)).collect(),
            out: w[2] as usize,
        },
        PRODUCT => Gene::Product { a: w[8] as usize, b: w[9] as usize, out: w[2] as usize },
        REACT => Gene::React { reaction: w[5] as usize, rate: w[8], speed: src },
        MOVE => Gene::Move { speed: bits(w[8]), fee_per_distance: w[9], src, threshold },
        TURN => Gene::Turn { angle_deg: bits(w[8]), fee: w[7], src, threshold },
        TAKE => Gene::Take { s: w[5] as usize, max: w[8], fee: w[7], src, threshold },
        DROP => Gene::Drop { s: w[5] as usize, max: w[8], fee: w[7], src, threshold },
        GIVE => Gene::Give { s: w[5] as usize, max: w[8], fee: w[7], src, threshold },
        STEAL => Gene::Steal { s: w[5] as usize, max: w[8], fee: w[7], src, threshold },
        GUARD => Gene::Guard { s: w[5] as usize, fee: w[7], src, threshold },
        DIVIDE => Gene::Divide { split_bp: w[5], requires: (w[8] as usize, w[9]), fee: w[7], src, threshold },
        other => panic!("packed gene kind {other} is unknown"),
    }
}

/// The words a master genome takes from its first word: the header, the keep values and the slots.
pub fn master_len(header: &[u32]) -> usize {
    MASTER_HDR + header[2] as usize + header[0] as usize * SLOT
}

/// The genome whose master words start at `off` in `mw` (the inverse of `master_words`).
pub fn decode_master(mw: &[u32], off: usize) -> emerge_ref::Genome {
    let (slots, modules, keep) = (mw[off] as usize, mw[off + 1] as usize, mw[off + 2] as usize);
    let first = off + MASTER_HDR + keep;
    emerge_ref::Genome {
        modules: (0..modules).map(|m| mw[off + 4 + m] != 0).collect(),
        keep: (0..keep).map(|k| bits(mw[off + MASTER_HDR + k])).collect(),
        slots: (0..slots)
            .map(|i| {
                let sw = &mw[first + i * SLOT..first + (i + 1) * SLOT];
                emerge_ref::mind::Slot { gene: unslot(sw), expressed: sw[17] != 0, module: sw[16] as usize }
            })
            .collect(),
    }
}
