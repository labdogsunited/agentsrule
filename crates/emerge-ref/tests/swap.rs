//! Hot-swaps on the CPU runner (spec §13, `proposals/engine-api.md` H1): a committed swap takes
//! effect at the next tick, and a rejected one leaves the world exactly as it was.

use emerge_ref::compile::Options;
use emerge_ref::swap::{Live, Rejected};

const DISH: &str = include_str!("../../../examples/dish.em");

/// The dish on a 32 × 32 grid with 300 of each lineage, so a compile is quick.
fn live(mutation: bool) -> Live {
    let src = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 32, h: 32)").replace("count: 2_000;", "count: 300;");
    Live::new(&src, &["Drifter", "Builder"], Options { seed: 3, allow_partial: true, mutation }).unwrap()
}

fn run(l: &mut Live, ticks: u64) {
    for _ in 0..ticks {
        l.world.run_tick();
    }
    l.world.audit().unwrap();
}

/// Everything a swap could touch: the state, the source and the revision.
fn state(l: &Live) -> String {
    format!("{:?}\n{}\n{}", l.world, l.source, l.revision)
}

fn carbon(l: &Live) -> u64 {
    l.world.emitted[l.world.chem.element("C")]
}

#[test]
fn a_committed_rule_change_takes_effect_at_the_next_tick() {
    let mut l = live(true);
    run(&mut l, 5);
    let before = carbon(&l);
    run(&mut l, 1);
    assert!(carbon(&l) > before, "the weather rule adds carbon every tick");
    l.hotswap("patch Dish {\n    remove rules.weather;\n}\n", 0).unwrap();
    assert_eq!(l.revision, 1);
    assert!(!l.source.contains("rule weather"));
    let after = carbon(&l);
    run(&mut l, 3);
    assert_eq!(carbon(&l), after, "no carbon comes in once the rule is gone");
    // A rule added back by a second swap, against the new revision.
    let wind = "patch Dish {\n    add after rules.sunlight: rule weather on cells { transfer(carbon, from: rock, to: self, amount: 40mu, overflow: void) @transfer_carbon; }\n}\n";
    assert_eq!(l.hotswap(wind, 0), Err(Rejected::Stale { base: 0, current: 1 }));
    l.hotswap(wind, 1).unwrap();
    let back = carbon(&l);
    run(&mut l, 1);
    assert!(carbon(&l) > back);
    assert_eq!(l.revision, 2);
}

#[test]
fn rejected_swaps_change_nothing() {
    let mut l = live(true);
    run(&mut l, 20);
    let was = state(&l);
    let cases: [(&str, &str, u64); 9] = [
        ("stale", "patch Dish {\n    set upkeep.base = 25mu;\n}\n", 4),
        ("stale base in the patch", "patch Dish base 7 {\n    set upkeep.base = 25mu;\n}\n", 0),
        ("does not parse", "patch Dish {\n    set upkeep.base = ;\n}\n", 0),
        ("unknown path", "patch Dish {\n    set rules.nowhere@x.amount = 1u;\n}\n", 0),
        ("does not check", "patch Dish {\n    set rules.rotting@react_rot.rate = 20_000bp;\n}\n", 0),
        ("space", "patch Dish {\n    set space.max_interaction_radius = 3.0;\n}\n", 0),
        ("an action fee", "patch Dish {\n    set actions.steal.cost = 200mu;\n}\n", 0),
        ("a new reaction", "patch Dish {\n    add after reactions.rot: reaction leak : light -> energy;\n}\n", 0),
        ("preflight: entities hold more than the new cap", "patch Dish {\n    set limits.inventory_cap = 6u;\n}\n", 0),
    ];
    for (what, patch, base) in cases {
        let r = l.hotswap(patch, base);
        let ok = match what {
            w if w.starts_with("stale") => matches!(r, Err(Rejected::Stale { .. })),
            "does not parse" | "unknown path" | "does not check" => matches!(r, Err(Rejected::Invalid { .. })),
            w if w.starts_with("preflight") => matches!(r, Err(Rejected::Preflight(_))),
            _ => matches!(r, Err(Rejected::NotSwappable(_))),
        };
        assert!(ok, "{what}: {r:?}");
        assert!(state(&l) == was, "{what}: the world changed");
    }
    // And the world still runs and balances.
    run(&mut l, 5);
}

#[test]
fn swappable_parts_change_and_the_ledger_holds() {
    let mut l = live(true);
    run(&mut l, 10);
    let swaps = [
        "patch Dish {\n    set upkeep.base = 25mu;\n    set upkeep.per_gene = 4mu;\n}\n",
        "patch Dish {\n    set rules.sunlight@transfer_light.amount = 60mu;\n    set rules.rotting@react_rot.rate = 80bp;\n}\n",
        "patch Dish {\n    set max_age = 15_000;\n    set limits.max_entities = 150_000;\n    set mind_steps = 2;\n}\n",
        "patch Drifter {\n    set mutation.rate = 0.03;\n}\n",
        "patch Dish {\n    set limits.inventory_cap = 200u;\n}\n",
        "patch Dish {\n    add after rules.spread: rule wind on cells { diffuse_conserved([carbon], rate: 100bp) @wind; }\n}\n",
    ];
    for (k, patch) in swaps.iter().enumerate() {
        l.hotswap(patch, k as u64).unwrap_or_else(|e| panic!("swap {k}: {e}"));
        run(&mut l, 5);
    }
    let w = &l.world;
    assert_eq!((w.upkeep.base, w.upkeep.per_gene, w.max_age, w.max_entities, w.mind_steps, w.inventory_cap), (25, 4, 15_000, 150_000, 2, 200_000));
    assert_eq!(w.variation[0].as_ref().unwrap().0.rate, 0.03);
    assert!(w.entities.iter().all(|e| e.store.caps[0] == 200_000));
    assert_eq!(l.revision, 6);
    // The running world now has the configuration of a fresh compile of its own source.
    let fresh = Live::new(&l.source, &["Drifter", "Builder"], l.options).unwrap();
    assert_eq!(emerge_ref::swap::Config::of(&fresh.world), emerge_ref::swap::Config::of(&l.world));
}
