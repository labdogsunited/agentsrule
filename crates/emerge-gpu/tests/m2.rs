//! M2 as a regression test: the GPU's Move phase (upkeep, fees, requests, arbitration,
//! commit, movement) equals the CPU's in contract mode, request by request.
//! Needs the GPU (`cargo test -p emerge-gpu`; the crate is not a default member).

use emerge_gpu::gpu::Gpu;
use emerge_gpu::m2::compare;
use emerge_ref::compile::{build, Options};

const DISH: &str = include_str!("../../../examples/dish.em");
const PATCH: &str = include_str!("coverage.patch.em");

#[test]
fn the_move_phase_matches_the_cpu() {
    let gpu = Gpu::new();
    let patch = emerge_lang::parse(PATCH).unwrap();
    let file = emerge_lang::patch::apply(&emerge_lang::parse(DISH).unwrap(), &patch).unwrap();
    for seed in [2, 3] {
        let mut w = build(&file, &["Drifter", "Builder"], Options { seed, allow_partial: true, mutation: true }).unwrap().world;
        w.contract = true;
        let r = compare(&mut w, &gpu, 400, 50, true);
        assert!(r.clean(), "seed {seed}: {:?}", (r.bad_amounts, r.bad_cells, r.bad_pos, r.bad_dying, r.bad_live, r.bad_requests));
        assert!(r.oversubscribed[0] > 0 && r.oversubscribed[1] > 0, "arbitration was not exercised: {:?}", r.oversubscribed);
    }
}
