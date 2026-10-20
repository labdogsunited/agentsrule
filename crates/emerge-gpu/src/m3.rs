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
            let rec = records(world, &mut packed, &plans);
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
            let rec = records(world, &mut packed, &plans);
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
                rep.entities += world.entities.len() as u64;
                let view = StateView { ent: &out.ent, epos: &out.epos, amounts: &out.amounts, ecap: &out.ecap, sig: &out.sig, camt: &out.camt, gw: &packed.gw, mw: &packed.mw };
                let (be, bc) = compare_state(world, &view, ns, &mut note);
                rep.bad_entities += be;
                rep.bad_cells += bc;
            }
        }
    }
    rep
}

/// The device arrays of a state, to compare with a `World`.
pub struct StateView<'a> {
    pub ent: &'a [u32],
    pub epos: &'a [f32],
    pub amounts: &'a [u32],
    pub ecap: &'a [u32],
    pub sig: &'a [f32],
    pub camt: &'a [u32],
    /// The genome words that the entities' offsets point into.
    pub gw: &'a [u32],
    /// The master genome words that the entities' master offsets point into.
    pub mw: &'a [u32],
}

/// Compares every entity and cell of `world` with `view`; returns the entities and cells that differ.
pub fn compare_state(world: &World, view: &StateView, ns: usize, note: &mut impl FnMut(String)) -> (u64, u64) {
    let cpu = pack(world);
    let (mut bad_entities, mut bad_cells) = (0, 0);
    for (i, e) in world.entities.iter().enumerate() {
        let w = &view.ent[i * ENT..(i + 1) * ENT];
        let id = w[0] as u64 | (w[1] as u64) << 32;
        let mut bad = Vec::new();
        if id != e.id { bad.push(format!("id {id} vs {}", e.id)); }
        if w[2] as usize != e.cell { bad.push(format!("cell {} vs {}", w[2], e.cell)); }
        if w[10] as u64 != e.age { bad.push(format!("age {} vs {}", w[10], e.age)); }
        if w[11] as usize != e.lineage { bad.push(format!("lineage {} vs {}", w[11], e.lineage)); }
        let want = [e.pos[0], e.pos[1], e.heading[0], e.heading[1]].map(f32::to_bits);
        let got: Vec<u32> = view.epos[i * 4..i * 4 + 4].iter().map(|x| x.to_bits()).collect();
        if want.to_vec() != got { bad.push("position/heading".into()); }
        if view.amounts[i * ns..(i + 1) * ns] != e.store.amounts[..ns] { bad.push(format!("amounts {:?} vs {:?}", &view.amounts[i * ns..(i + 1) * ns], &e.store.amounts[..ns])); }
        if view.ecap[i * ns..(i + 1) * ns] != e.store.caps[..ns] { bad.push("caps".into()); }
        let sig: Vec<u32> = view.sig[i * NSIG..i * NSIG + e.signals.len()].iter().map(|x| x.to_bits()).collect();
        if sig != e.signals.iter().map(|x| x.to_bits()).collect::<Vec<_>>() { bad.push("signals".into()); }
        let (nsg, ng) = (cpu.ent[i * ENT + 6] as usize, cpu.ent[i * ENT + 5] as usize);
        let len = nsg + ng * SLOT;
        let (c0, g0) = (cpu.ent[i * ENT + 4] as usize, w[4] as usize);
        if w[5] as usize != ng || w[6] as usize != nsg || cpu.gw[c0..c0 + len] != view.gw[g0..g0 + len] { bad.push("genome".into()); }
        let master = master_words(&e.genome);
        let m0 = w[12] as usize;
        if view.mw.len() < m0 + master.len() || view.mw[m0..m0 + master.len()] != master[..] { bad.push("master genome".into()); }
        if !bad.is_empty() {
            bad_entities += 1;
            note(format!("entity {} (index {i}): {}", e.id, bad.join("; ")));
        }
    }
    for (c, cell) in world.cells.iter().enumerate() {
        if view.camt[c * ns..(c + 1) * ns] != cell.amounts[..ns] {
            bad_cells += 1;
            note(format!("cell {c} amounts cpu {:?} gpu {:?}", &cell.amounts[..ns], &view.camt[c * ns..(c + 1) * ns]));
        }
    }
    (bad_entities, bad_cells)
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

/// The whole tick on the GPU, kept there, against the CPU's `run_tick`, every `every` ticks
/// (the GPU runs every tick).
pub fn compare_full(world: &mut World, gpu: &Gpu, ticks: u64, every: u64, verbose: bool) -> (u64, u64, u64) {
    compare_full_with(world, gpu, ticks, every, verbose, |_| {})
}

/// `compare_full`, with `setup` applied to the GPU world before the first tick.
pub fn compare_full_with(world: &mut World, gpu: &Gpu, ticks: u64, every: u64, verbose: bool, setup: impl FnOnce(&mut crate::resident::Resident)) -> (u64, u64, u64) {
    use crate::resident::Resident;
    assert!(world.contract);
    let mut gpu_world = Resident::new(gpu, world);
    setup(&mut gpu_world);
    let (mut states, mut bad_entities, mut bad_cells) = (0, 0, 0);
    let mut shown = 0;
    for tick in 0..ticks {
        gpu_world.tick();
        world.run_tick();
        if tick % every == 0 || tick + 1 == ticks {
            let s = gpu_world.snapshot();
            states += 1;
            if verbose && std::env::var_os("EMERGE_GPU_POOLS").is_some() {
                println!("  tick {tick}: {} entities, pools {:?}, collected {}", s.n, gpu_world.pool_tops(), gpu_world.collections);
            }
            let mut note = |what: String| {
                if verbose && shown < 8 {
                    shown += 1;
                    println!("  tick {tick}: {what}");
                }
            };
            if s.n != world.entities.len() || s.tick as u64 != world.tick || s.next_id != world.next_id {
                bad_entities += 1;
                note(format!("{} entities, tick {}, next id {} on the GPU; {}, {}, {} on the CPU", s.n, s.tick, s.next_id, world.entities.len(), world.tick, world.next_id));
                break;
            }
            let view = StateView { ent: &s.ent, epos: &s.epos, amounts: &s.amounts, ecap: &s.ecap, sig: &s.sig, camt: &s.camt, gw: &s.gw, mw: &s.mw };
            let (mut be, bc) = compare_state(world, &view, s.ns, &mut note);
            if gpu_world.births != world.births || gpu_world.deaths != world.deaths || gpu_world.extent() != world.entity_extent {
                be += 1;
                note(format!("counts: births {} vs {}, deaths {} vs {}, extent {:?} vs {:?}", gpu_world.births, world.births, gpu_world.deaths, world.deaths, gpu_world.extent(), world.entity_extent));
            }
            bad_entities += be;
            bad_cells += bc;
            if be + bc > 0 {
                break; // later ticks only repeat the first difference
            }
        }
    }
    if verbose {
        println!("genome pools collected {} times; {:?} words in use", gpu_world.collections, gpu_world.pool_tops());
    }
    (states, bad_entities, bad_cells)
}

pub fn run_full_cmd(args: &[String]) {
    use emerge_ref::compile::{compile, Options};
    let [dish, lineages, ticks, seed, every] = args else {
        eprintln!("usage: emerge-gpu m3 DISH LINEAGES TICKS SEED EVERY (whole ticks, mutation off)");
        return;
    };
    let src = std::fs::read_to_string(dish).expect("read dish");
    let names: Vec<&str> = lineages.split(',').collect();
    let (ticks, seed, every): (u64, u64, u64) = (ticks.parse().unwrap(), seed.parse().unwrap(), every.parse().unwrap());
    let mutation = std::env::var_os("EMERGE_GPU_MUTATION").is_some();
    let mut world = compile(&src, &names, Options { seed, allow_partial: true, mutation }).unwrap_or_else(|e| panic!("{e:?}")).world;
    world.contract = true;
    let gpu = Gpu::new();
    println!("M3 (whole ticks on the GPU, mutation {}): {} ; {dish} {lineages} seed {seed}, compared every {every} ticks", if mutation { "on" } else { "off" }, gpu.name);
    let t = std::time::Instant::now();
    let (states, be, bc) = compare_full(&mut world, &gpu, ticks, every, true);
    println!("{ticks} ticks, {states} states compared, {} entities at the end; mismatches: entities {be}, cells {bc} ({:.1} s)", world.entities.len(), t.elapsed().as_secs_f64());
}

/// `emerge-gpu run DISH LINEAGES TICKS SEED [EVERY]`: the whole run on the GPU (mutation with `EMERGE_GPU_MUTATION`), with a
/// line per window: population, births and deaths in the window, entity-reaction extent by reaction.
pub fn run_gpu_cmd(args: &[String]) {
    use crate::resident::Resident;
    use emerge_ref::compile::{compile, Options};
    if args.len() < 4 {
        eprintln!("usage: emerge-gpu run DISH LINEAGES TICKS SEED [EVERY]");
        return;
    }
    let src = std::fs::read_to_string(&args[0]).expect("read dish");
    let names: Vec<&str> = args[1].split(',').collect();
    let (ticks, seed): (u64, u64) = (args[2].parse().unwrap(), args[3].parse().unwrap());
    let every: u64 = args.get(4).map_or(1000, |v| v.parse().unwrap());
    let mutation = std::env::var_os("EMERGE_GPU_MUTATION").is_some();
    let mut world = compile(&src, &names, Options { seed, allow_partial: true, mutation }).unwrap_or_else(|e| panic!("{e:?}")).world;
    world.contract = true;
    let reaction_names: Vec<String> = world.chem.reactions.iter().map(|r| world.chem.formula(&r.inputs, &r.outputs)).collect();
    let gpu = Gpu::new();
    let mut r = Resident::new(&gpu, &mut world);
    if std::env::var_os("EMERGE_GPU_AUDIT").is_some() {
        r.enable_audit();
    }
    println!("run on {}: {} {} seed {seed}, {ticks} ticks, mutation {}, contract mode", gpu.name, args[0], args[1], if mutation { "on" } else { "off" });
    println!("tick,population,births,deaths,ms_per_tick,entity-reaction extent (nonzero)");
    let (mut b0, mut d0, mut e0, mut t0) = (r.births, r.deaths, r.extent(), std::time::Instant::now());
    for tick in 1..=ticks {
        r.tick();
        if tick % every == 0 {
            let ms = t0.elapsed().as_secs_f64() * 1e3 / every as f64;
            let now = r.extent();
            let extent: Vec<String> = now.iter().zip(&e0).enumerate().filter(|(_, (a, b))| a != b).map(|(k, (a, b))| format!("[{}]={}", reaction_names[k], *a - *b)).collect();
            println!("{tick},{},{},{},{ms:.2},{}", r.n, r.births - b0, r.deaths - d0, extent.join(" "));
            (b0, d0, e0, t0) = (r.births, r.deaths, now.clone(), std::time::Instant::now());
        }
    }
    if let Some(a) = r.audit() {
        println!("ledger audit: {} ticks checked, {} with element totals off, largest difference {} micro-units; genome pools collected {} times", a.ticks, a.drifted, a.worst, r.collections);
    }
    let report = gpu.kernel_report();
    if !report.is_empty() {
        let total: f64 = report.iter().map(|r| r.1).sum();
        println!("kernel time per tick (us, calls per tick): {:.0} in all", total / ticks as f64);
        for (name, us, n) in report.iter().take(24) {
            println!("  {name:<18} {:>8.1} us  x{:.1}", us / ticks as f64, *n as f64 / ticks as f64);
        }
    }
    if let Some(p) = r.profile {
        let total: f64 = p.iter().sum();
        println!("phases (s): prep {:.2}, set {:.2}, move {:.2}, transform {:.2}, restructure {:.2}; {:.2} ms per tick", p[0], p[1], p[2], p[3], p[4], total / ticks as f64 * 1e3);
    }
}
