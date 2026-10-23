//! The GPU's per-statement counters (`rules.activity`) equal the CPU runner's, field for field, in
//! contract mode, for inflows, shifts, diffusion and reactions, with and without a `when`. Needs the GPU.

use emerge_gpu::gpu::Gpu;
use emerge_gpu::resident::Resident;
use emerge_ref::compile::Options;
use emerge_ref::swap::Live;

const DISH: &str = include_str!("../../../examples/dish.em");

#[test]
fn the_gpu_counts_what_each_world_statement_did_like_the_cpu() {
    let mut src = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 64, h: 64)").replace("count: 2_000;", "count: 300;")
        .replace("transfer(light, from: sun, to: self, amount: 40mu, overflow: void) @transfer_light;", "when tick >= 10 { transfer(light, from: sun, to: self, amount: 40mu, overflow: void) @transfer_light; }");
    let at = src.find("    genesis {").unwrap();
    src.insert_str(at, "    rule wind on cells {\n        when self.carbon >= 25u { transfer(carbon, from: self, to: right, amount: 30mu) @shift_c; }\n    }\n\n    rule photo on cells {\n        when self.light >= 40mu { react(photosynthesis, rate: 10000bp) @react_photo; }\n    }\n\n");
    let mk = || { let mut l = Live::new(&src, &["Drifter", "Builder"], Options { seed: 5, allow_partial: true, mutation: true }).unwrap(); l.world.contract = true; l };
    let (mut cpu, mut dev) = (mk(), mk());
    let gpu = Gpu::new();
    let mut r = Resident::new(&gpu, &mut dev.world);
    r.enable_law_stats(&dev.world);
    for _ in 0..80 {
        cpu.world.run_tick();
        r.tick();
    }
    let want = &cpu.world.law_stats;
    let got = r.law_stats().unwrap();
    assert_eq!(want.len(), got.len());
    assert!(want.len() >= 7);
    for (i, (w, g)) in want.iter().zip(&got).enumerate() {
        assert_eq!(w, g, "law {i}");
    }
    // the fixture exercised every counter
    assert!(want.iter().any(|s| s.passed < s.cells && s.passed > 0), "a condition that holds on some cells only");
    assert!(want.iter().any(|s| s.cut_by_dest > 0 || s.cut_by_origin > 0), "a request that was cut");
    assert!(want.iter().any(|s| s.ran > 0 && s.extent > 0), "a reaction that ran");
    assert!(want.iter().any(|s| s.requests > 0 && s.delivered > 0));
}

use emerge_ref::world::End;

type Key = (u64, Option<u32>, (u8, u64), (u8, u64), usize, bool, u32, u32, u32);

fn dish_with_rules() -> String {
    let mut src = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 24, h: 24)").replace("count: 2_000;", "count: 300;")
        .replace("transfer(light, from: sun, to: self, amount: 40mu, overflow: void) @transfer_light;", "when tick >= 4 { transfer(light, from: sun, to: self, amount: 40mu, overflow: void) @transfer_light; }");
    let at = src.find("    genesis {").unwrap();
    src.insert_str(at, "    rule wind on cells {\n        when self.carbon >= 25u { transfer(carbon, from: self, to: right, amount: 30mu) @shift_c; }\n    }\n\n");
    src
}

/// The transfer recording of the GPU equals the CPU's records of the same ticks, as a set.
fn compare_records(ticks: u32, laws: Option<Vec<u32>>, creatures: bool) -> usize {
    let src = dish_with_rules();
    let mk = || { let mut l = Live::new(&src, &["Drifter", "Builder"], Options { seed: 5, allow_partial: true, mutation: true }).unwrap(); l.world.contract = true; l };
    let (mut cpu, mut dev) = (mk(), mk());
    cpu.world.record_transfers = true;
    let gpu = Gpu::new();
    let mut r = Resident::new(&gpu, &mut dev.world);
    r.enable_law_stats(&dev.world);
    r.trace_start(32768, laws.as_deref(), creatures).unwrap();
    let mut want: Vec<Key> = vec![];
    for _ in 0..ticks {
        cpu.world.run_tick();
        r.tick();
        let end = |e: &End, id: Option<u64>| match e { End::Cell(c) => (0u8, *c as u64), End::Entity(_) => (1, id.unwrap()), End::Source(s) => (2, *s as u64), End::Sink(s) => (3, *s as u64) };
        for (x, m) in cpu.world.last_transfers.iter().zip(&cpu.world.last_transfer_meta) {
            let wanted = match m.law { Some(l) => laws.as_ref().is_none_or(|ls| ls.contains(&l)), None => creatures };
            if wanted { want.push((cpu.world.tick, m.law, end(&x.origin, m.origin_id), end(&x.dest, m.dest_id), x.s, m.steal, x.requested, x.delivered, x.overflowed)); }
        }
    }
    let (records, seen) = r.trace_read().unwrap();
    assert_eq!(seen as usize, want.len(), "requests wanted");
    assert!(want.len() < 32768, "the fixture must fit the records, has {}", want.len());
    let mut got: Vec<Key> = records.iter().map(|t| (t.tick, t.law.map(|l| l as u32), t.origin, t.dest, t.substance, t.steal, t.requested, t.delivered, t.overflowed)).collect();
    got.sort();
    want.sort();
    assert_eq!(got, want);
    want.len()
}

#[test]
fn the_gpu_records_the_same_transfer_requests_as_the_cpu() {
    // everything, for a few ticks
    assert!(compare_records(3, None, true) > 1000);
    // one statement over a longer window, then creatures' own actions alone
    let n = compare_records(8, Some(vec![0]), false);
    assert!(n > 0);
    assert!(compare_records(10, Some(vec![]), true) > 0, "creatures act");
}

#[test]
fn a_full_recording_counts_the_requests_it_had_no_room_for() {
    let src = dish_with_rules();
    let mut l = Live::new(&src, &["Drifter"], Options { seed: 5, allow_partial: true, mutation: true }).unwrap();
    l.world.contract = true;
    let gpu = Gpu::new();
    let mut r = Resident::new(&gpu, &mut l.world);
    r.enable_law_stats(&l.world);
    r.trace_start(100, None, true).unwrap();
    for _ in 0..3 { r.tick(); }
    let (records, seen) = r.trace_read().unwrap();
    assert_eq!(records.len(), 100);
    assert!(seen > 100, "seen {seen}");
    // a closed recording keeps what it has
    r.trace_stop();
    r.tick();
    assert_eq!(r.trace_read().unwrap().1, seen);
}
