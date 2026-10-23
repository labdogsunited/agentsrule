//! The GPU's per-lineage resource accounts equal the CPU runner's, column for column, in contract
//! mode, and keeping them changes nothing in the run. Needs the GPU.

use emerge_gpu::gpu::Gpu;
use emerge_gpu::m3::{compare_state, StateView};
use emerge_gpu::resident::Resident;
use emerge_ref::compile::Options;
use emerge_ref::swap::Live;
use emerge_ref::world::{Accounts, ACCOUNT_COLUMNS};

const DISH: &str = include_str!("../../../examples/dish.em");

fn live(src: &str) -> Live {
    let mut l = Live::new(src, &["Drifter", "Builder"], Options { seed: 5, allow_partial: true, mutation: true }).unwrap();
    l.world.contract = true;
    l
}

#[test]
fn the_gpu_accounts_equal_the_cpu_accounts_per_lineage() {
    let src = DISH.replace("Grid(w: 96, h: 96)", "Grid(w: 96, h: 96)").replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;");
    let gpu = Gpu::new();
    let mut cpu = live(&src);
    let mut dev = live(&src);
    let mut r = Resident::new(&gpu, &mut dev.world);
    for _ in 0..5 {
        cpu.world.run_tick();
        r.tick();
    }
    cpu.world.accounts = Some(Accounts::default());
    r.enable_accounts(2);
    for _ in 0..150 {
        cpu.world.run_tick();
        r.tick();
    }
    let want = &cpu.world.accounts.as_ref().unwrap().totals;
    let got = r.accounts().unwrap();
    let mut bad = vec![];
    for g in 0..2 {
        for (c, name) in ACCOUNT_COLUMNS.iter().enumerate() {
            let w = want.get(g).map_or(0, |t| t[c]);
            if got[g][c] != w { bad.push(format!("lineage {g} {name}: gpu {} cpu {w}", got[g][c])); }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    for (g, row) in got.iter().enumerate() { println!("{g}: {:?}", ACCOUNT_COLUMNS.iter().zip(row).filter(|(_, v)| **v > 0).collect::<Vec<_>>()); }
    // the columns were not all empty
    for name in ["births", "starved", "upkeep_mu", "fees_mu", "pay_made_mu", "pay_used_mu"] {
        let c = ACCOUNT_COLUMNS.iter().position(|x| *x == name).unwrap();
        assert!(got.iter().any(|row| row[c] > 0), "{name} should be counted in this fixture");
    }
    // and the run is still the CPU's
    let s = r.snapshot();
    let view = StateView { ent: &s.ent, epos: &s.epos, amounts: &s.amounts, ecap: &s.ecap, sig: &s.sig, camt: &s.camt, gw: &s.gw, mw: &s.mw };
    let (be, bc) = compare_state(&cpu.world, &view, s.ns, &mut |m| println!("{m}"));
    assert_eq!(be + bc, 0);
}

#[path = "../../emerge-ref/tests/common/mod.rs"]
mod common;

/// Taking, giving, stealing, dropping, venting and dying with the upkeep substance in hand, on a laid
/// grid: the columns the dish does not reach.
#[test]
fn transfers_waste_and_deaths_of_the_upkeep_substance_are_counted_like_the_cpu() {
    use emerge_ref::world::Space;
    use emerge_ref::{End, Gene, Genome, ONE, U};
    for vent in [false, true] {
        let (mut w, d) = common::dish(20, 0);
        let caps = vec![2_000 * U; w.chem.substances.len()];
        w.grid(Space { w: 8.0, h: 8.0, wrap: true, grid_w: 4, grid_h: 4, child_offset: 0.5, radius: 2.0 }, caps);
        w.contract = true;
        let give = Gene::Give { s: d.energy, max: U, fee: 5, src: ONE, threshold: 0.0 };
        let steal = Gene::Steal { s: d.energy, max: U, fee: 30, src: ONE, threshold: 0.0 };
        let a = w.spawn_at([1.0, 1.0], [1.0, 0.0], Genome::simple(vec![Gene::take(d.energy, U, 0), Gene::drop(d.energy, 500, 0)]));
        let b = w.spawn_at([2.5, 1.0], [1.0, 0.0], Genome::simple(vec![give]));
        let c = w.spawn_at([1.0, 5.0], [1.0, 0.0], Genome::simple(vec![steal]));
        let v = w.spawn_at([2.0, 5.0], [1.0, 0.0], Genome::simple(vec![]));
        let dead = w.spawn_at([6.0, 6.0], [1.0, 0.0], Genome::simple(vec![]));
        w.endow(End::Cell(0), d.energy, 5 * U);
        for (e, x) in [(a, 3 * U), (b, 4 * U), (c, 3 * U), (v, 5 * U), (dead, 15)] {
            w.endow(End::Entity(e), d.energy, x);
        }
        if vent { w.waste.push((d.energy, d.void)); }
        w.accounts = Some(Accounts::default());
        let mut cpu = w.clone();
        cpu.record_transfers = true;
        let gpu = Gpu::new();
        let mut r = Resident::new(&gpu, &mut w);
        r.enable_accounts(1);
        r.enable_law_stats(&w);
        r.trace_start(1000, Some(&[]), true).unwrap();
        let mut want = vec![];
        for _ in 0..6 {
            cpu.run_tick();
            r.tick();
            for (x, m) in cpu.last_transfers.iter().zip(&cpu.last_transfer_meta) {
                let end = |e: &End, id: Option<u64>| match e { End::Cell(c) => (0u8, *c as u64), End::Entity(_) => (1, id.unwrap()), End::Source(s) => (2, *s as u64), End::Sink(s) => (3, *s as u64) };
                want.push((cpu.tick, end(&x.origin, m.origin_id), end(&x.dest, m.dest_id), x.s, m.steal, x.requested, x.delivered, x.overflowed));
            }
        }
        let mut got: Vec<_> = r.trace_read().unwrap().0.iter().map(|t| (t.tick, t.origin, t.dest, t.substance, t.steal, t.requested, t.delivered, t.overflowed)).collect();
        got.sort();
        want.sort();
        assert!(want.iter().any(|t| t.4) && want.iter().any(|t| !t.4 && t.1.0 == 1 && t.2.0 == 1), "the fixture has steals and gifts");
        assert_eq!(got, want, "vent {vent}: the recorded creature transfers");
        let want = &cpu.accounts.as_ref().unwrap().totals[0];
        let got = r.accounts().unwrap()[0];
        let bad: Vec<String> = ACCOUNT_COLUMNS.iter().enumerate().filter(|(c, _)| got[*c] != want[*c]).map(|(c, n)| format!("{n}: gpu {} cpu {}", got[c], want[c])).collect();
        assert!(bad.is_empty(), "vent {vent}: {}", bad.join("; "));
        for name in ["pay_taken_mu", "pay_gift_in_mu", "pay_gift_out_mu", "pay_stolen_in_mu", "pay_stolen_out_mu", "pay_dropped_mu", "starved"] {
            let c = ACCOUNT_COLUMNS.iter().position(|x| *x == name).unwrap();
            assert!(got[c] > 0, "vent {vent}: {name} should be counted in this fixture");
        }
        if vent {
            let c = ACCOUNT_COLUMNS.iter().position(|x| *x == "pay_wasted_mu").unwrap();
            assert!(got[c] > 0, "wasted should be counted");
        }
    }
}
