//! Hot-swaps on the GPU (`proposals/engine-api.md` H2): the same swaps on the CPU runner and on
//! the GPU give equal full states after every tick, in contract mode. Needs the GPU.

use emerge_gpu::gpu::Gpu;
use emerge_gpu::m3::{compare_state, StateView};
use emerge_gpu::resident::Resident;
use emerge_gpu::swap::hotswap;
use emerge_ref::compile::Options;
use emerge_ref::swap::{Live, Rejected};

const DISH: &str = include_str!("../../../examples/dish.em");

fn live(src: &str) -> Live {
    let mut l = Live::new(src, &["Drifter", "Builder"], Options { seed: 5, allow_partial: true, mutation: true }).unwrap();
    l.world.contract = true;
    l
}

/// Full state, births, deaths and reaction extent; the number of differences.
fn differences(cpu: &Live, r: &Resident, tick: u64) -> u64 {
    let s = r.snapshot();
    let w = &cpu.world;
    if s.n != w.entities.len() || s.next_id != w.next_id {
        println!("tick {tick}: {} entities on the GPU, {} on the CPU", s.n, w.entities.len());
        return 1;
    }
    let view = StateView { ent: &s.ent, epos: &s.epos, amounts: &s.amounts, ecap: &s.ecap, sig: &s.sig, camt: &s.camt, gw: &s.gw, mw: &s.mw };
    let mut note = |m: String| println!("tick {tick}: {m}");
    let (be, bc) = compare_state(w, &view, s.ns, &mut note);
    let counts = u64::from(r.births != w.births || r.deaths != w.deaths || r.extent() != w.entity_extent);
    be + bc + counts
}

#[test]
fn the_same_swaps_give_the_same_world_on_cpu_and_gpu() {
    let src = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;");
    let reacts: String = (0..70).map(|_| "react(rot, rate: 1bp); ").collect();
    let rain: String = (0..300).map(|k| format!("transfer({}, from: rock, to: self, amount: 2mu, overflow: void); ", ["carbon", "nitrate"][k % 2])).collect();
    let swaps: Vec<(u64, String)> = vec![
        (4, "patch Dish {\n    set upkeep.base = 25mu;\n    set upkeep.per_gene = 4mu;\n}\n".into()),
        (8, "patch Dish {\n    set rules.sunlight@transfer_light.amount = 60mu;\n    set rules.rotting@react_rot.rate = 80bp;\n}\n".into()),
        (12, format!("patch Dish {{\n    add after rules.spread: rule many on cells {{ {reacts}}}\n    add after rules.spread: rule rain on cells {{ {rain}}}\n}}\n")),
        (16, "patch Dish {\n    set max_age = 300;\n    set mind_steps = 2;\n    set limits.max_entities = 900;\n}\n".into()),
        (20, "patch Drifter {\n    set mutation.rate = 0.2;\n    set mutation.duplicate_gene = 0.1;\n}\n".into()),
        (24, "patch Dish {\n    set limits.inventory_cap = 150u;\n}\n".into()),
        (28, "patch Dish {\n    remove rules.rain;\n    remove rules.many;\n}\n".into()),
        // Action fees (H7): every live gene and the menu that mutation copies from.
        (34, "patch Dish {\n    set actions.take.cost = 8mu;\n    set actions.turn.cost = 0u;\n    set actions.move.cost = 30mu;\n}\n".into()),
        (70, "patch Dish {\n    set actions.steal.cost = 12mu;\n    set actions.take.cost = 3mu;\n}\n".into()),
    ];
    let gpu = Gpu::new();
    let mut cpu = live(&src);
    let mut on_gpu = live(&src);
    let mut r = Resident::new(&gpu, &mut on_gpu.world);
    let mut bad = 0;
    for tick in 0..150u64 {
        for (at, patch) in &swaps {
            if *at == tick {
                cpu.hotswap(patch, cpu.revision).unwrap_or_else(|e| panic!("CPU, tick {tick}: {e}"));
                let base = on_gpu.revision;
                let (pause, _) = hotswap(&mut on_gpu, &mut r, patch, base).unwrap_or_else(|e| panic!("GPU, tick {tick}: {e}"));
                assert!(pause < 0.2, "tick {tick}: the pause took {pause} s");
            }
        }
        // A swap that fails preflight is rejected on both, and changes neither.
        if tick == 30 {
            let low = "patch Dish {\n    set limits.inventory_cap = 6u;\n}\n";
            assert!(matches!(cpu.hotswap(low, cpu.revision), Err(Rejected::Preflight(_))));
            let base = on_gpu.revision;
            assert!(matches!(hotswap(&mut on_gpu, &mut r, low, base), Err(Rejected::Preflight(_))));
        }
        cpu.world.run_tick();
        r.tick();
        bad += differences(&cpu, &r, tick);
        assert_eq!(bad, 0, "tick {tick}: CPU and GPU differ");
    }
    assert_eq!((cpu.revision, on_gpu.revision), (9, 9));
    let w = &cpu.world;
    assert!(w.births > 0 && w.deaths > 0 && !w.entities.is_empty(), "births {}, deaths {}, alive {}", w.births, w.deaths, w.entities.len());
    cpu.world.audit().unwrap();
}

#[test]
fn a_rule_change_pauses_the_full_dish_briefly() {
    // Spec §13.2: the pause (preflight and commit) is under 200 ms for a rule change; compile time is apart.
    let gpu = Gpu::new();
    let mut l = Live::new(DISH, &["Drifter"], Options { seed: 1, allow_partial: true, mutation: true }).unwrap();
    l.world.contract = true;
    let mut r = Resident::new(&gpu, &mut l.world);
    for _ in 0..50 {
        r.tick();
    }
    let (pause, compile) = hotswap(&mut l, &mut r, "patch Dish {\n    set rules.rotting@react_rot.rate = 80bp;\n}\n", 0).unwrap();
    println!("full dish, {} entities: pause {:.1} ms, compile {:.0} ms", r.n, pause * 1e3, compile * 1e3);
    assert!(pause < 0.2, "the pause took {pause} s");
    r.tick();
    // A fee change rewrites every live genome's fee words as well (H7).
    let (pause, _) = hotswap(&mut l, &mut r, "patch Dish {\n    set actions.take.cost = 8mu;\n}\n", 1).unwrap();
    println!("full dish, {} entities: fee swap pause {:.1} ms", r.n, pause * 1e3);
    assert!(pause < 0.2, "the fee swap's pause took {pause} s");
    r.tick();
}
