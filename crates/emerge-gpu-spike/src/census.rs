//! Census of real genomes (archive/gpu-spike.md, B): runs the CPU runner, counts the
//! shapes of the evolved minds, and packs them for the GPU benchmark.
//!
//! Packed file: per entity `[n_slots, n_signals, keep × n_signals (f32), SLOT_WORDS × n_slots]`,
//! all u32 little endian. Only active genes are packed.

use emerge_ref::compile::{compile, Options};
use emerge_ref::{Curve, Gene, Genome, Input, Sense};
use std::collections::HashMap;

pub const SLOT_WORDS: usize = 12;

/// Slot kinds shared with the benchmark shader.
pub const SENSE: u32 = 0;
pub const WEIGH_LINEAR: u32 = 1; // + curve index: linear, step, sigmoid, tanh
pub const PRODUCT: u32 = 5;
pub const READER: u32 = 6;
pub const REAL_KINDS: u32 = 7;

fn curve_index(c: Curve) -> u32 {
    match c {
        Curve::Linear => 0,
        Curve::Step => 1,
        Curve::Sigmoid => 2,
        Curve::Tanh => 3,
    }
}

fn sense_variant(s: &Sense) -> u32 {
    match s {
        Sense::Inventory(_) => 0,
        Sense::Cell(_) => 1,
        Sense::Gradient(_) => 2,
        Sense::Probe { .. } => 3,
        Sense::Crowd { .. } => 4,
        Sense::Neighbour { .. } => 5,
        Sense::Clock { .. } => 6,
        Sense::Delta(_) => 7,
    }
}

fn signal_of(i: Input) -> (f32, u32) {
    match i {
        Input::Const(c) => (c, 255),
        Input::Signal(s) => (1.0, s as u32),
    }
}

/// The word that identifies a gene's kind for shape counting: what a specialised kernel
/// would have to be compiled for.
fn gene_kind(g: &Gene) -> String {
    match g {
        Gene::Sense { sense, out2, .. } => format!("sense{}{}", sense_variant(sense), if out2.is_some() { "x2" } else { "" }),
        Gene::Weigh { curve, inputs, .. } => format!("weigh{}n{}", curve_index(*curve), inputs.len()),
        Gene::Product { .. } => "product".into(),
        Gene::React { .. } => "react".into(),
        Gene::Move { .. } => "move".into(),
        Gene::Turn { .. } => "turn".into(),
        Gene::Take { .. } => "take".into(),
        Gene::Drop { .. } => "drop".into(),
        Gene::Give { .. } => "give".into(),
        Gene::Steal { .. } => "steal".into(),
        Gene::Guard { .. } => "guard".into(),
        Gene::Divide { .. } => "divide".into(),
    }
}

fn is_mind(g: &Gene) -> bool {
    matches!(g, Gene::Sense { .. } | Gene::Weigh { .. } | Gene::Product { .. })
}

/// The packed slots of one genome's active genes.
pub fn pack(genome: &Genome) -> Vec<[u32; SLOT_WORDS]> {
    let mut out = Vec::new();
    for g in (0..genome.slots.len()).filter(|&g| genome.active(g)) {
        let mut w = [0u32; SLOT_WORDS];
        match &genome.slots[g].gene {
            Gene::Sense { sense, out: o, out2 } => {
                w[0] = SENSE | (sense_variant(sense) << 4) | ((*o as u32) << 8) | (out2.map_or(255, |x| x as u32) << 16);
                w[1] = 0x3f80_0000; // scale 1.0
            }
            Gene::Weigh { bias, curve, inputs, out: o } => {
                w[0] = (WEIGH_LINEAR + curve_index(*curve)) | ((inputs.len() as u32) << 4) | ((*o as u32) << 8);
                w[1] = bias.to_bits();
                for (k, &(wt, s)) in inputs.iter().take(4).enumerate() {
                    w[2 + 2 * k] = wt.to_bits();
                    w[3 + 2 * k] = s as u32;
                }
            }
            Gene::Product { a, b, out: o } => {
                w[0] = PRODUCT | (2 << 4) | ((*o as u32) << 8);
                w[3] = *a as u32;
                w[5] = *b as u32;
            }
            other => {
                let src = match other {
                    Gene::React { speed, .. } => *speed,
                    Gene::Move { src, .. }
                    | Gene::Turn { src, .. }
                    | Gene::Take { src, .. }
                    | Gene::Drop { src, .. }
                    | Gene::Give { src, .. }
                    | Gene::Steal { src, .. }
                    | Gene::Guard { src, .. }
                    | Gene::Divide { src, .. } => *src,
                    _ => unreachable!(),
                };
                let (c, s) = signal_of(src);
                w[0] = READER | (1 << 4);
                w[2] = c.to_bits();
                w[3] = s;
            }
        }
        out.push(w);
    }
    out
}

pub fn run(args: &[String]) {
    let [src_path, lineage, ticks, seed, out_path] = args else {
        eprintln!("usage: emerge-gpu-spike census SRC LINEAGE TICKS SEED OUT");
        return;
    };
    let src = std::fs::read_to_string(src_path).expect("read dish");
    let (ticks, seed): (u64, u64) = (ticks.parse().unwrap(), seed.parse().unwrap());
    let mut world = compile(&src, &[lineage.as_str()], Options { seed, allow_partial: true, mutation: true }).unwrap_or_else(|e| panic!("{e:?}")).world;
    let t = std::time::Instant::now();
    for k in 0..ticks {
        world.run_tick();
        if (k + 1) % 2000 == 0 {
            eprintln!("tick {} entities {} ({:.0} s)", k + 1, world.entities.len(), t.elapsed().as_secs_f64());
        }
    }
    let n = world.entities.len();
    let mut full: HashMap<String, usize> = HashMap::new();
    let mut mind: HashMap<String, usize> = HashMap::new();
    let (mut active, mut mind_genes, mut sig_max, mut active_max) = (0usize, 0usize, 0usize, 0usize);
    let mut words: Vec<u32> = Vec::new();
    for e in &world.entities {
        let genes: Vec<&Gene> = (0..e.genome.slots.len()).filter(|&g| e.genome.active(g)).map(|g| &e.genome.slots[g].gene).collect();
        *full.entry(genes.iter().map(|g| gene_kind(g)).collect::<Vec<_>>().join(",")).or_default() += 1;
        *mind.entry(genes.iter().filter(|g| is_mind(g)).map(|g| gene_kind(g)).collect::<Vec<_>>().join(",")).or_default() += 1;
        active += genes.len();
        mind_genes += genes.iter().filter(|g| is_mind(g)).count();
        active_max = active_max.max(genes.len());
        sig_max = sig_max.max(e.genome.keep.len());
        let slots = pack(&e.genome);
        words.push(slots.len() as u32);
        words.push(e.genome.keep.len() as u32);
        words.extend(e.genome.keep.iter().map(|k| k.to_bits()));
        words.extend(slots.iter().flatten());
    }
    let shared = |m: &HashMap<String, usize>| m.values().filter(|&&c| c > 1).sum::<usize>();
    println!("{n} entities after {ticks} ticks (seed {seed}, lineage {lineage})");
    println!("active genes: mean {:.1}, max {active_max}; of them mind genes (sense, weigh, product): mean {:.1}; signals: max {sig_max}", active as f64 / n as f64, mind_genes as f64 / n as f64);
    println!("distinct full shapes (kind sequence of all active genes): {}; entities sharing a shape with another: {:.1}%", full.len(), shared(&full) as f64 / n as f64 * 100.0);
    println!("distinct mind shapes (sense, weigh, product only): {}; entities sharing: {:.1}%", mind.len(), shared(&mind) as f64 / n as f64 * 100.0);
    let mut top: Vec<_> = mind.iter().collect();
    top.sort_by_key(|(_, &c)| std::cmp::Reverse(c));
    let covered: usize = top.iter().take(10).map(|(_, &c)| c).sum();
    println!("the 10 most common mind shapes cover {:.1}% of entities", covered as f64 / n as f64 * 100.0);
    std::fs::write(out_path, bytemuck::cast_slice::<u32, u8>(&words)).expect("write");
    println!("packed {} bytes to {out_path}", words.len() * 4);
}
