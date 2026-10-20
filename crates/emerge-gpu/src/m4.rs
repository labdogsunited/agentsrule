//! M4: the GPU's mutation of a genome equals the CPU's (contract mode), word for word, on
//! parents taken from an evolving population.

use crate::gpu::Gpu;
use crate::mutation::run_batch;
use crate::pack::*;
use emerge_ref::mutate::{mutate, validate, Draws};
use emerge_ref::philox::{self, Stream};
use emerge_ref::World;

#[derive(Default)]
pub struct Report {
    pub mutations: u64,
    pub changed: u64,
    pub bad: u64,
    pub invalid: u64,
}

impl Report {
    pub fn clean(&self) -> bool {
        self.mutations > 0 && self.bad == 0 && self.invalid == 0
    }
}

/// Mutates `count` genomes drawn from `world`'s population on both sides and compares them.
pub fn compare_mutations(world: &World, gpu: &Gpu, count: usize, verbose: bool) -> Report {
    assert!(world.contract && world.mutation);
    let (menus, rates) = pack_menus(world);
    let mut masters: Vec<u32> = Vec::new();
    let mut offsets = Vec::new();
    for e in &world.entities {
        offsets.push(masters.len() as u32);
        masters.extend(master_words(&e.genome));
    }
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (rng >> 33) as u32
    };
    let mut rep = Report::default();
    let mut shown = 0;
    const BATCH: usize = 50_000;
    let mut done = 0;
    while done < count {
        let n = BATCH.min(count - done);
        let mut jobs = Vec::new();
        let mut which = Vec::new();
        for _ in 0..n {
            let i = next() as usize % world.entities.len();
            let child = next();
            which.push((i, child));
            jobs.push([offsets[i], world.entities[i].lineage as u32, child, world.tick as u32]);
        }
        let got = run_batch(gpu, &menus, &rates, world.seed, &masters, &jobs);
        for ((i, child), got) in which.iter().zip(&got) {
            let (got, gtrace) = got.split_at(got.len() - 58);
            let e = &world.entities[*i];
            let Some(Some((r, cap))) = world.variation.get(e.lineage) else { continue };
            let stream = Stream { seed: world.seed, tick: world.tick as u32, stream: *child, domain: philox::DOMAIN_ENTITY, aux: 0, pass: philox::MUTATE };
            let mut d = Draws::new(stream);
            d.contract = true;
            d.trace = Some(Vec::new());
            let want_genome = mutate(&e.genome, r, cap, &world.menus, &mut d);
            let cpu_trace = d.trace.clone().unwrap_or_default();
            let mut want = master_words(&want_genome);
            want[3] = d.next as u32; // the GPU reports the draws it consumed in the spare header word
            rep.mutations += 1;
            rep.changed += u64::from(want_genome != e.genome);
            if validate(&want_genome, cap, &world.menus).is_err() {
                rep.invalid += 1;
            }
            if want != got {
                rep.bad += 1;
                if verbose && shown < 4 {
                    shown += 1;
                    let at = want.iter().zip(got.iter()).position(|(a, b)| a != b);
                    println!("  parent {} child {}: cpu {} words ({} draws), gpu {} words ({} draws), first difference at {:?}", e.id, child, want.len(), want[3], got.len(), got[3], at);
                    println!("    parent genes: {:?}", e.genome.slots.iter().map(|s| emerge_gpu_kind(&s.gene)).collect::<Vec<_>>());
                    println!("    modules of the slots {:?}, module bits {:?}, expressed {:?}", e.genome.slots.iter().map(|s| s.module).collect::<Vec<_>>(), e.genome.modules, e.genome.slots.iter().map(|s| s.expressed as u8).collect::<Vec<_>>());
                    if std::env::var_os("EMERGE_M4_TRACE").is_some() {
                        println!("    cpu draws after each gene {:?}\n    gpu draws after each gene {:?}", cpu_trace, { let n = cpu_trace.iter().filter(|&&x| x < 1_000_000).count(); let mut g = gtrace[..n].to_vec(); g.push(gtrace[53]); g.extend_from_slice(&gtrace[50..53]); g });
                    }
                    if std::env::var_os("EMERGE_M4_DUMP").is_some() { for s in &e.genome.slots { println!("      {:?}", s.gene); } }
                    if let Some(k) = at {
                        println!("    cpu {:08x?}\n    gpu {:08x?}", &want[k.saturating_sub(2)..(k + 6).min(want.len())], &got[k.saturating_sub(2)..(k + 6).min(got.len())]);
                    }
                }
            }
        }
        done += n;
    }
    rep
}

fn emerge_gpu_kind(g: &emerge_ref::Gene) -> String {
    format!("{:?}", g).split(|c: char| !c.is_alphanumeric()).next().unwrap_or("").to_string()
}

pub fn run(args: &[String]) {
    use emerge_ref::compile::{compile, Options};
    let [dish, lineages, warm, seed, count] = args else {
        eprintln!("usage: emerge-gpu m4 DISH LINEAGES WARM_TICKS SEED COUNT");
        return;
    };
    let src = std::fs::read_to_string(dish).expect("read dish");
    let names: Vec<&str> = lineages.split(',').collect();
    let (warm, seed, count): (u64, u64, usize) = (warm.parse().unwrap(), seed.parse().unwrap(), count.parse().unwrap());
    let mut world = compile(&src, &names, Options { seed, allow_partial: true, mutation: true }).unwrap_or_else(|e| panic!("{e:?}")).world;
    world.contract = true;
    for _ in 0..warm {
        world.run_tick();
    }
    // EMERGE_M4_ONLY=name keeps one operator, at the rate in EMERGE_M4_RATE (default 0.2), to find which differs.
    if let Ok(only) = std::env::var("EMERGE_M4_ONLY") {
        let p: f32 = std::env::var("EMERGE_M4_RATE").ok().and_then(|v| v.parse().ok()).unwrap_or(0.2);
        for (r, _) in world.variation.iter_mut().flatten() {
            let keep = |name: &str, v: f32| if name == only { p } else { 0.0 * v };
            r.rate = keep("rate", r.rate);
            r.expression_flip = keep("expression_flip", r.expression_flip);
            r.rewire = keep("rewire", r.rewire);
            r.diverge = keep("diverge", r.diverge);
            r.duplicate_gene = keep("duplicate_gene", r.duplicate_gene);
            r.insert_gene = keep("insert_gene", r.insert_gene);
            r.delete_gene = keep("delete_gene", r.delete_gene);
            r.duplicate_module = keep("duplicate_module", r.duplicate_module);
            r.delete_module = keep("delete_module", r.delete_module);
            r.split_module = keep("split_module", r.split_module);
            r.merge_modules = keep("merge_modules", r.merge_modules);
        }
    }
    let gpu = Gpu::new();
    println!("M4 (mutation): {} ; {dish} {lineages} seed {seed}, {warm} ticks of warm-up, {} entities", gpu.name, world.entities.len());
    let t = std::time::Instant::now();
    let r = compare_mutations(&world, &gpu, count, true);
    println!("{} mutations compared ({} changed the genome); mismatches {}, invalid {} ({:.1} s)", r.mutations, r.changed, r.bad, r.invalid, t.elapsed().as_secs_f64());
}
