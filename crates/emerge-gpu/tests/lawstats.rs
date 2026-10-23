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
