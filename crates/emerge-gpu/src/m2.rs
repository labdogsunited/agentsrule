//! M2: the GPU's Move phase against the CPU's, on states sampled from a contract-mode run.
//! The inputs are the CPU's own intents (so M2 is tested apart from M1).

use crate::gpu::Gpu;
use crate::movephase::run_move;
use crate::pack::*;
use emerge_ref::world::{Act, End, Plan};
use emerge_ref::{Gene, World};

pub struct Report {
    pub states: u64,
    pub entities: u64,
    pub requests: u64,
    pub oversubscribed: [u64; 2],
    pub bad_amounts: u64,
    pub bad_cells: u64,
    pub bad_pos: u64,
    pub bad_dying: u64,
    pub bad_live: u64,
    pub bad_requests: u64,
    pub gpu_time: std::time::Duration,
}

impl Report {
    pub fn clean(&self) -> bool {
        self.states > 0 && self.bad_amounts + self.bad_cells + self.bad_pos + self.bad_dying + self.bad_live + self.bad_requests == 0
    }
}

/// The Set records for the CPU's plans, in the layout the GPU's Set phase writes.
pub fn records(world: &World, packed: &Packed, plans: &[Plan]) -> Vec<u32> {
    let mut rec = vec![0u32; packed.records * REC];
    let mut at = 0;
    for (i, e) in world.entities.iter().enumerate() {
        for (gi, &g) in packed.genes[i].iter().enumerate() {
            let base = (at + gi) * REC;
            let plan = &plans[i];
            if let Gene::React { .. } = e.genome.slots[g].gene {
                rec[base..base + 4].copy_from_slice(&[R_REACT, g as u32, 0, plan.react_caps[g]]);
                continue;
            }
            if let Some(x) = plan.intents.iter().find(|x| x.gene == g) {
                let r: [u32; 7] = match x.act {
                    Act::Move { distance } => [R_MOVE, g as u32, 0, 0, x.fee, 0, distance.to_bits()],
                    Act::Turn { angle_deg } => [R_TURN, g as u32, 0, 0, x.fee, 0, angle_deg.to_bits()],
                    Act::Take { s, amount } => [R_TAKE, g as u32, s as u32, amount, x.fee, 0, 0],
                    Act::Drop { s, amount } => [R_DROP, g as u32, s as u32, amount, x.fee, 0, 0],
                    Act::Give { s, amount, other } => [R_GIVE, g as u32, s as u32, amount, x.fee, other as u32, 0],
                    Act::Steal { s, amount, other } => [R_STEAL, g as u32, s as u32, amount, x.fee, other as u32, 0],
                    Act::Guard { s } => [R_GUARD, g as u32, s as u32, 0, x.fee, 0, 0],
                    Act::Divide => [R_DIVIDE, g as u32, 0, 0, x.fee, 0, 0],
                };
                rec[base..base + 7].copy_from_slice(&r);
                rec[base + 7] = u32::from(x.live);
            }
        }
        at += packed.genes[i].len();
    }
    rec
}

fn pack_end(e: End) -> u32 {
    match e {
        End::Cell(i) => i as u32,
        End::Entity(i) => (1 << 30) | i as u32,
        End::Source(i) => (2 << 30) | i as u32,
        End::Sink(i) => (3 << 30) | i as u32,
    }
}

/// Runs `world` (contract mode) and compares the Move phase on every `every`-th tick.
pub fn compare(world: &mut World, gpu: &Gpu, ticks: u64, every: u64, verbose: bool) -> Report {
    assert!(world.contract);
    let mut rep = Report {
        states: 0, entities: 0, requests: 0, oversubscribed: [0; 2], bad_amounts: 0, bad_cells: 0, bad_pos: 0, bad_dying: 0, bad_live: 0, bad_requests: 0,
        gpu_time: std::time::Duration::ZERO,
    };
    let mut shown = 0;
    for tick in 0..ticks {
        world.begin_tick();
        let mut plans = world.set_phase();
        let sample = tick % every == 0;
        let gpu_out = sample.then(|| {
            let mut packed = pack(world);
            pack_move(world, &mut packed);
            if std::env::var_os("EMERGE_GPU_BREAK").is_some() {
                packed.params[P_SEEDLO] ^= 1; // a deliberate error, to show that the comparison can fail
            }
            let rec = records(world, &packed, &plans);
            let t = std::time::Instant::now();
            let out = run_move(gpu, &packed, &rec);
            rep.gpu_time += t.elapsed();
            (packed, out)
        });
        world.record_transfers = sample;
        let dying = world.move_phase(&mut plans);
        if let Some((packed, out)) = gpu_out {
            rep.states += 1;
            rep.requests += out.rq.chunks(8).filter(|r| r[4] > 0).count() as u64;
            rep.oversubscribed[0] += out.listed[0] as u64;
            rep.oversubscribed[1] += out.listed[1] as u64;
            let ns = packed.ns;
            let mut note = |what: &str| {
                if verbose && shown < 8 {
                    shown += 1;
                    println!("  tick {tick}: {what}");
                }
            };
            let mut at = 0;
            for (i, e) in world.entities.iter().enumerate() {
                rep.entities += 1;
                if out.amounts[i * ns..(i + 1) * ns] != e.store.amounts[..ns] {
                    rep.bad_amounts += 1;
                    note(&format!("entity {} amounts cpu {:?} gpu {:?}", e.id, &e.store.amounts[..ns], &out.amounts[i * ns..(i + 1) * ns]));
                }
                let want = [e.pos[0], e.pos[1], e.heading[0], e.heading[1]].map(f32::to_bits);
                let got: Vec<u32> = out.epos[i * 4..i * 4 + 4].iter().map(|x| x.to_bits()).collect();
                if want.to_vec() != got {
                    rep.bad_pos += 1;
                    note(&format!("entity {} position/heading cpu {:?} gpu {:?}", e.id, [e.pos[0], e.pos[1], e.heading[0], e.heading[1]], &out.epos[i * 4..i * 4 + 4]));
                }
                if (out.dying[i] == 1) != dying[i] {
                    rep.bad_dying += 1;
                    note(&format!("entity {} dying cpu {} gpu {}", e.id, dying[i], out.dying[i]));
                }
                for x in &plans[i].intents {
                    let gi = packed.genes[i].iter().position(|&g| g == x.gene).unwrap();
                    let live = out.rec[(at + gi) * REC + 7] == 1;
                    if live != x.live {
                        rep.bad_live += 1;
                        note(&format!("entity {} gene {} live cpu {} gpu {}", e.id, x.gene, x.live, live));
                    }
                }
                at += packed.genes[i].len();
            }
            let nc = world.cells.len();
            for c in 0..nc {
                if out.camt[c * ns..(c + 1) * ns] != world.cells[c].amounts[..ns] {
                    rep.bad_cells += 1;
                    note(&format!("cell {c} amounts cpu {:?} gpu {:?}", &world.cells[c].amounts[..ns], &out.camt[c * ns..(c + 1) * ns]));
                }
            }
            // Per request: the same requests, in the same order, with the same grants.
            let want: Vec<[u32; 6]> = world
                .last_transfers
                .iter()
                .map(|t| [pack_end(t.origin), pack_end(t.dest), t.s as u32, t.requested, t.stage1, t.stage2])
                .collect();
            let got: Vec<[u32; 6]> = out.rq.chunks(8).filter(|r| r[4] > 0).map(|r| [r[0], r[1], r[2], r[4], r[6], r[7]]).collect();
            if want != got {
                rep.bad_requests += 1;
                let first = want.iter().zip(&got).position(|(a, b)| a != b);
                note(&format!("requests differ: cpu {} gpu {}, first difference at {:?}: cpu {:?} gpu {:?}", want.len(), got.len(), first, first.map(|k| want[k]), first.map(|k| got[k])));
            }
        }
        world.record_transfers = false;
        world.after_move(plans, dying);
    }
    rep
}

pub fn run(args: &[String]) {
    use emerge_ref::compile::{compile, Options};
    let [dish, lineages, ticks, seed, every] = args else {
        eprintln!("usage: emerge-gpu m2 DISH LINEAGES TICKS SEED EVERY");
        return;
    };
    let src = std::fs::read_to_string(dish).expect("read dish");
    let names: Vec<&str> = lineages.split(',').collect();
    let (ticks, seed, every): (u64, u64, u64) = (ticks.parse().unwrap(), seed.parse().unwrap(), every.parse().unwrap());
    let mut world = compile(&src, &names, Options { seed, allow_partial: true, mutation: true }).unwrap_or_else(|e| panic!("{e:?}")).world;
    world.contract = true;
    let gpu = Gpu::new();
    println!("M2: {} ; {dish} {lineages} seed {seed}, contract mode, sampled every {every} ticks", gpu.name);
    let r = compare(&mut world, &gpu, ticks, every, true);
    println!("{} states, {} entity-states, {} requests; oversubscribed requests listed (stage 1, stage 2): {:?}; GPU time {:.0} ms", r.states, r.entities, r.requests, r.oversubscribed, r.gpu_time.as_secs_f64() * 1e3);
    println!("mismatches: entity amounts {}, cell amounts {}, positions {}, dying {}, live flags {}, request lists {}", r.bad_amounts, r.bad_cells, r.bad_pos, r.bad_dying, r.bad_live, r.bad_requests);
}
