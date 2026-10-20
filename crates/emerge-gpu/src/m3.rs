//! M3: Transform (and, later in this file, Restructure) against the CPU, on states sampled
//! from a contract-mode run. The inputs are the CPU's own state after the previous phase.

use crate::gpu::Gpu;
use crate::m2::records;
use crate::pack::*;
use crate::restructure::run_restructure;
use crate::transform::run_transform;
use emerge_ref::World;

#[derive(Default)]
pub struct Report {
    pub states: u64,
    pub entities: u64,
    pub bad_amounts: u64,
    pub bad_cells: u64,
}

impl Report {
    pub fn clean(&self) -> bool {
        self.states > 0 && self.bad_amounts + self.bad_cells == 0
    }
}

/// Runs `world` (contract mode) and compares Transform on every `every`-th tick.
pub fn compare_transform(world: &mut World, gpu: &Gpu, ticks: u64, every: u64, verbose: bool) -> Report {
    assert!(world.contract);
    let mut rep = Report::default();
    let mut shown = 0;
    for tick in 0..ticks {
        world.begin_tick();
        let mut plans = world.set_phase();
        let dying = world.move_phase(&mut plans);
        let gpu_out = (tick % every == 0).then(|| {
            let mut packed = pack(world);
            pack_transform(world, &mut packed);
            let rec = records(world, &packed, &plans);
            let d: Vec<u32> = dying.iter().map(|&x| u32::from(x)).collect();
            run_transform(gpu, &packed, &rec, &d)
        });
        world.transform_phase(&plans, &dying);
        if let Some(out) = gpu_out {
            rep.states += 1;
            let ns = world.chem.substances.len();
            for (i, e) in world.entities.iter().enumerate() {
                rep.entities += 1;
                if out.amounts[i * ns..(i + 1) * ns] != e.store.amounts[..ns] {
                    rep.bad_amounts += 1;
                    if verbose && shown < 6 {
                        shown += 1;
                        println!("  tick {tick} entity {} amounts cpu {:?} gpu {:?}", e.id, &e.store.amounts[..ns], &out.amounts[i * ns..(i + 1) * ns]);
                    }
                }
            }
            for (c, cell) in world.cells.iter().enumerate() {
                if out.camt[c * ns..(c + 1) * ns] != cell.amounts[..ns] {
                    rep.bad_cells += 1;
                    if verbose && shown < 6 {
                        shown += 1;
                        println!("  tick {tick} cell {c} amounts cpu {:?} gpu {:?}", &cell.amounts[..ns], &out.camt[c * ns..(c + 1) * ns]);
                    }
                }
            }
        }
        world.restructure_phase(plans, dying);
    }
    rep
}

pub fn run(args: &[String]) {
    use emerge_ref::compile::{compile, Options};
    let [dish, lineages, ticks, seed, every] = args else {
        eprintln!("usage: emerge-gpu m3 DISH LINEAGES TICKS SEED EVERY");
        return;
    };
    let src = std::fs::read_to_string(dish).expect("read dish");
    let names: Vec<&str> = lineages.split(',').collect();
    let (ticks, seed, every): (u64, u64, u64) = (ticks.parse().unwrap(), seed.parse().unwrap(), every.parse().unwrap());
    let mut world = compile(&src, &names, Options { seed, allow_partial: true, mutation: true }).unwrap_or_else(|e| panic!("{e:?}")).world;
    world.contract = true;
    let gpu = Gpu::new();
    println!("M3 (Transform): {} ; {dish} {lineages} seed {seed}, contract mode, sampled every {every} ticks", gpu.name);
    let r = compare_transform(&mut world, &gpu, ticks, every, true);
    println!("{} states, {} entity-states; mismatches: entity amounts {}, cell amounts {}", r.states, r.entities, r.bad_amounts, r.bad_cells);
}

#[derive(Default)]
pub struct RestructureReport {
    pub states: u64,
    pub entities: u64,
    pub removed: u64,
    pub children: u64,
    pub bad_counts: u64,
    pub bad_entities: u64,
    pub bad_cells: u64,
}

impl RestructureReport {
    pub fn clean(&self) -> bool {
        self.states > 0 && self.bad_counts + self.bad_entities + self.bad_cells == 0
    }
}

/// Compares Restructure on every `every`-th tick, from the CPU's state after Transform.
pub fn compare_restructure(world: &mut World, gpu: &Gpu, ticks: u64, every: u64, verbose: bool) -> RestructureReport {
    assert!(world.contract);
    let mut rep = RestructureReport::default();
    let mut shown = 0;
    for tick in 0..ticks {
        world.begin_tick();
        let mut plans = world.set_phase();
        let dying = world.move_phase(&mut plans);
        world.transform_phase(&plans, &dying);
        let gpu_out = (tick % every == 0).then(|| {
            let mut packed = pack(world);
            pack_transform(world, &mut packed);
            pack_restructure(world, &mut packed);
            let rec = records(world, &packed, &plans);
            let d: Vec<u32> = dying.iter().map(|&x| u32::from(x)).collect();
            let out = run_restructure(gpu, &packed, &rec, &d);
            (packed, out)
        });
        world.restructure_phase(plans, dying);
        if let Some((packed, out)) = gpu_out {
            rep.states += 1;
            rep.removed += out.removed as u64;
            rep.children += out.children as u64;
            let ns = packed.ns;
            let mut note = |what: String| {
                if verbose && shown < 8 {
                    shown += 1;
                    println!("  tick {tick}: {what}");
                }
            };
            if out.alive + out.children != world.entities.len() {
                rep.bad_counts += 1;
                note(format!("{} entities on the GPU ({} survivors, {} children), {} on the CPU", out.alive + out.children, out.alive, out.children, world.entities.len()));
            } else {
                let cpu = pack(world);
                for (i, e) in world.entities.iter().enumerate() {
                    rep.entities += 1;
                    let w = &out.ent[i * ENT..(i + 1) * ENT];
                    let id = w[0] as u64 | (w[1] as u64) << 32;
                    let mut bad = Vec::new();
                    if id != e.id { bad.push(format!("id {id} vs {}", e.id)); }
                    if w[2] as usize != e.cell { bad.push(format!("cell {} vs {}", w[2], e.cell)); }
                    if w[10] as u64 != e.age { bad.push(format!("age {} vs {}", w[10], e.age)); }
                    if w[11] as usize != e.lineage { bad.push(format!("lineage {} vs {}", w[11], e.lineage)); }
                    let want = [e.pos[0], e.pos[1], e.heading[0], e.heading[1]].map(f32::to_bits);
                    let got: Vec<u32> = out.epos[i * 4..i * 4 + 4].iter().map(|x| x.to_bits()).collect();
                    if want.to_vec() != got { bad.push("position/heading".into()); }
                    if out.amounts[i * ns..(i + 1) * ns] != e.store.amounts[..ns] { bad.push(format!("amounts {:?} vs {:?}", &out.amounts[i * ns..(i + 1) * ns], &e.store.amounts[..ns])); }
                    if out.ecap[i * ns..(i + 1) * ns] != e.store.caps[..ns] { bad.push("caps".into()); }
                    let sig: Vec<u32> = out.sig[i * NSIG..i * NSIG + e.signals.len()].iter().map(|x| x.to_bits()).collect();
                    if sig != e.signals.iter().map(|x| x.to_bits()).collect::<Vec<_>>() { bad.push("signals".into()); }
                    let (nsg, ng) = (cpu.ent[i * ENT + 6] as usize, cpu.ent[i * ENT + 5] as usize);
                    let len = nsg + ng * SLOT;
                    let (c0, g0) = (cpu.ent[i * ENT + 4] as usize, w[4] as usize);
                    if w[5] as usize != ng || w[6] as usize != nsg || cpu.gw[c0..c0 + len] != packed.gw[g0..g0 + len] { bad.push("genome".into()); }
                    if !bad.is_empty() {
                        rep.bad_entities += 1;
                        note(format!("entity {} (index {i}): {}", e.id, bad.join("; ")));
                    }
                }
                for (c, cell) in world.cells.iter().enumerate() {
                    if out.camt[c * ns..(c + 1) * ns] != cell.amounts[..ns] {
                        rep.bad_cells += 1;
                        note(format!("cell {c} amounts cpu {:?} gpu {:?}", &cell.amounts[..ns], &out.camt[c * ns..(c + 1) * ns]));
                    }
                }
            }
        }
    }
    rep
}

pub fn run_restructure_cmd(args: &[String]) {
    use emerge_ref::compile::{compile, Options};
    let [dish, lineages, ticks, seed, every] = args else {
        eprintln!("usage: emerge-gpu m3r DISH LINEAGES TICKS SEED EVERY");
        return;
    };
    let src = std::fs::read_to_string(dish).expect("read dish");
    let names: Vec<&str> = lineages.split(',').collect();
    let (ticks, seed, every): (u64, u64, u64) = (ticks.parse().unwrap(), seed.parse().unwrap(), every.parse().unwrap());
    let mut world = compile(&src, &names, Options { seed, allow_partial: true, mutation: false }).unwrap_or_else(|e| panic!("{e:?}")).world;
    world.contract = true;
    let gpu = Gpu::new();
    println!("M3 (Restructure, mutation off): {} ; {dish} {lineages} seed {seed}, contract mode, sampled every {every} ticks", gpu.name);
    let r = compare_restructure(&mut world, &gpu, ticks, every, true);
    println!("{} states, {} entity-states, {} removed, {} children; mismatches: counts {}, entities {}, cells {}", r.states, r.entities, r.removed, r.children, r.bad_counts, r.bad_entities, r.bad_cells);
}
