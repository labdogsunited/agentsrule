//! M1 as a regression test: the GPU's Set phase equals the CPU's in contract mode, on a
//! dish whose menus and mutation rates make every sense, regulation and action kind appear.
//! Needs the GPU (`cargo test -p emerge-gpu`; the crate is not a default member).

use emerge_gpu::gpu::Gpu;
use emerge_gpu::m1::compare;
use emerge_ref::compile::{build, Options};

const DISH: &str = include_str!("../../../examples/dish.em");
const PATCH: &str = include_str!("coverage.patch.em");

fn world(seed: u64, mind_steps: u32, one_per_substance: bool) -> emerge_ref::World {
    let patch = emerge_lang::parse(PATCH).unwrap();
    let file = emerge_lang::patch::apply(&emerge_lang::parse(DISH).unwrap(), &patch).unwrap();
    let mut w = build(&file, &["Drifter", "Builder"], Options { seed, allow_partial: true, mutation: true }).unwrap().world;
    w.contract = true;
    w.mind_steps = mind_steps;
    w.one_per_substance = one_per_substance;
    w
}

#[test]
fn the_set_phase_matches_the_cpu_for_every_gene_kind() {
    let gpu = Gpu::new();
    let mut seen: std::collections::BTreeMap<String, u64> = Default::default();
    for (seed, steps, one) in [(2, 1, false), (3, 3, false), (4, 1, true)] {
        let mut w = world(seed, steps, one);
        let r = compare(&mut w, &gpu, 600, 50, true);
        assert!(r.clean(), "seed {seed}, {steps} mind steps: signals {} intents {} prev {}", r.bad_sig, r.bad_rec, r.bad_prev);
        for (kind, (genes, _)) in r.coverage {
            *seen.entry(kind).or_default() += genes;
        }
    }
    // A claim covers only the kinds that were compared.
    for kind in [
        "sense probe", "sense crowd", "sense neighbour", "sense delta", "sense clock", "sense gradient", "sense inventory",
        "weigh tanh", "weigh sigmoid", "weigh step", "weigh linear", "product", "react", "move", "turn", "take", "drop",
        "give", "steal", "guard", "divide",
    ] {
        assert!(seen.get(kind).is_some_and(|&n| n > 0), "no `{kind}` gene was compared: {seen:?}");
    }
}
