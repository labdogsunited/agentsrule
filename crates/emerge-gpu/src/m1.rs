//! M1: the GPU's intents, reaction caps, signals and `prev` against the CPU's, on states
//! sampled from a contract-mode run, with a count of the gene kinds that were compared.

use crate::gpu::Gpu;
use crate::pack::*;
use crate::set::run_set;
use emerge_ref::compile::{compile, Options};
use emerge_ref::world::{Act, Plan};
use std::collections::BTreeMap;

/// The CPU's plan as `(gene, code, s, amount, fee, other, f32 bits)`, one per record.
fn cpu_records(world: &emerge_ref::World, i: usize, plan: &Plan) -> Vec<[u32; 7]> {
    let e = &world.entities[i];
    let mut out = Vec::new();
    for x in &plan.intents {
        let g = x.gene as u32;
        out.push(match x.act {
            Act::Move { distance } => [g, R_MOVE, 0, 0, x.fee, 0, distance.to_bits()],
            Act::Turn { angle_deg } => [g, R_TURN, 0, 0, x.fee, 0, angle_deg.to_bits()],
            Act::Take { s, amount } => [g, R_TAKE, s as u32, amount, x.fee, 0, 0],
            Act::Drop { s, amount } => [g, R_DROP, s as u32, amount, x.fee, 0, 0],
            Act::Give { s, amount, other } => [g, R_GIVE, s as u32, amount, x.fee, other as u32, 0],
            Act::Steal { s, amount, other } => [g, R_STEAL, s as u32, amount, x.fee, other as u32, 0],
            Act::Guard { s } => [g, R_GUARD, s as u32, 0, x.fee, 0, 0],
            Act::Divide => [g, R_DIVIDE, 0, 0, x.fee, 0, 0],
        });
    }
    for g in (0..e.genome.slots.len()).filter(|&g| e.genome.active(g)) {
        if matches!(e.genome.slots[g].gene, emerge_ref::Gene::React { .. }) {
            out.push([g as u32, R_REACT, 0, plan.react_caps[g], 0, 0, 0]);
        }
    }
    out.sort();
    out
}

pub struct Report {
    pub states: u64,
    pub entities: u64,
    pub bad_sig: u64,
    pub bad_rec: u64,
    pub bad_prev: u64,
    pub gpu_time: std::time::Duration,
    /// Per gene kind: genes compared and records fired.
    pub coverage: BTreeMap<String, (u64, u64)>,
}

impl Report {
    pub fn clean(&self) -> bool {
        self.states > 0 && self.bad_sig == 0 && self.bad_rec == 0 && self.bad_prev == 0
    }
}

/// Runs `world` for `ticks` ticks (it must be in contract mode) and compares the GPU's Set
/// phase with the CPU's on every `every`-th tick.
pub fn compare(world: &mut emerge_ref::World, gpu: &Gpu, ticks: u64, every: u64, verbose: bool) -> Report {
    assert!(world.contract, "the GPU matches contract mode only");
    let mut rep = Report { states: 0, entities: 0, bad_sig: 0, bad_rec: 0, bad_prev: 0, gpu_time: std::time::Duration::ZERO, coverage: BTreeMap::new() };
    let mut shown = 0;
    for tick in 0..ticks {
        world.begin_tick();
        let sample = tick % every == 0;
        let gpu_out = sample.then(|| {
            let mut packed = pack(&world);
            if std::env::var_os("EMERGE_GPU_BREAK").is_some() {
                packed.params[P_STEPS] += 1; // a deliberate error, to show that the comparison can fail
            }
            let (out, t) = run_set(&gpu, &packed);
            rep.gpu_time += t;
            (packed, out)
        });
        let plans = world.set_phase();
        if let Some((packed, out)) = gpu_out {
            rep.states += 1;
            for (i, e) in world.entities.iter().enumerate() {
                rep.entities += 1;
                // Signals.
                let want: Vec<u32> = e.signals.iter().map(|s| s.to_bits()).collect();
                let got: Vec<u32> = out.sig[i * NSIG..i * NSIG + e.signals.len()].iter().map(|s| s.to_bits()).collect();
                if want != got {
                    rep.bad_sig += 1;
                    if verbose && shown < 5 {
                        shown += 1;
                        println!("  tick {tick} entity {} signals differ: cpu {:?} gpu {:?}", e.id, e.signals, &out.sig[i * NSIG..i * NSIG + e.signals.len()]);
                    }
                }
                // prev, for entities that keep one.
                if e.shape.delta {
                    let k = packed.ns;
                    if e.prev.iter().take(k).copied().collect::<Vec<_>>() != out.prev[i * k..(i + 1) * k] {
                        rep.bad_prev += 1;
                    }
                }
                // Records.
                let plan = &plans[i];
                let want = cpu_records(&world, i, plan);
                let off = {
                    let first: usize = packed.genes[..i].iter().map(Vec::len).sum();
                    first * REC
                };
                let mut got: Vec<[u32; 7]> = Vec::new();
                for (gi, &g) in packed.genes[i].iter().enumerate() {
                    let r = &out.rec[off + gi * REC..off + (gi + 1) * REC];
                    if r[0] != 0 {
                        got.push([r[1], r[0], r[2], r[3], r[4], r[5], r[6]]);
                        debug_assert_eq!(r[1] as usize, g);
                    }
                    let label = kind_label(&{
                        // Rebuild the kind label from the packed words of this gene.
                        let goff = packed.ent[i * ENT + 4] as usize + packed.ent[i * ENT + 6] as usize + gi * SLOT;
                        let mut w = [0u32; SLOT];
                        w.copy_from_slice(&packed.gw[goff..goff + SLOT]);
                        w
                    });
                    let c = rep.coverage.entry(label).or_default();
                    c.0 += 1;
                    c.1 += u64::from(r[0] != 0);
                }
                got.sort();
                if want != got {
                    rep.bad_rec += 1;
                    if verbose && shown < 5 {
                        shown += 1;
                        println!("  tick {tick} entity {} records differ:\n    cpu {:?}\n    gpu {:?}", e.id, want, got);
                        if std::env::var_os("EMERGE_GPU_DEBUG").is_some() {
                            for r in want.iter().filter(|r| r[1] == R_STEAL || r[1] == R_GIVE) {
                                let j = r[5] as usize;
                                let (a, b) = (&world.entities[i], &world.entities[j]);
                                let sp = world.space.as_ref().unwrap();
                                let d2 = emerge_ref::near::distance2(sp, a.pos, b.pos, true);
                                println!("    other {j}: pos {:?} vs {:?}, d2 {d2}, radius {}, buckets {} and {}, packed bucket {}", a.pos, b.pos, sp.radius, world.near_index().bucket(a.pos), world.near_index().bucket(b.pos), packed.ent[i * ENT + 3]);
                            }
                        }
                    }
                }
            }
        }
        world.finish_tick(plans);
    }
    rep
}

pub fn run(args: &[String]) {
    let [dish, lineages, ticks, seed, every] = args else {
        eprintln!("usage: emerge-gpu m1 DISH LINEAGES TICKS SEED EVERY");
        return;
    };
    let src = std::fs::read_to_string(dish).expect("read dish");
    let names: Vec<&str> = lineages.split(',').collect();
    let (ticks, seed, every): (u64, u64, u64) = (ticks.parse().unwrap(), seed.parse().unwrap(), every.parse().unwrap());
    let mut world = compile(&src, &names, Options { seed, allow_partial: true, mutation: true }).unwrap_or_else(|e| panic!("{e:?}")).world;
    world.contract = true;
    world.one_per_substance = std::env::var_os("EMERGE_GPU_ONE").is_some();
    let gpu = Gpu::new();
    println!("M1: {} ; {dish} {lineages} seed {seed}, contract mode, sampled every {every} ticks", gpu.name);
    let r = compare(&mut world, &gpu, ticks, every, true);
    println!("{} states, {} entity-states compared; GPU Set time (dispatch, wall) {:.1} ms in total", r.states, r.entities, r.gpu_time.as_secs_f64() * 1e3);
    println!("mismatches: signals {}, intents/caps {}, prev {}", r.bad_sig, r.bad_rec, r.bad_prev);
    println!("coverage (compared genes / intents fired):");
    for (k, (n, f)) in &r.coverage {
        println!("  {k:<18} {n:>10} {f:>10}");
    }
}
