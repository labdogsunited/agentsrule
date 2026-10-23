//! The GPU's birth and death log (ancestry) equals the CPU runner's events, and keeping it changes
//! nothing in the run. Contract mode. Needs the GPU.

use emerge_gpu::gpu::Gpu;
use emerge_gpu::m3::{compare_state, StateView};
use emerge_gpu::resident::Resident;
use emerge_ref::compile::Options;
use emerge_ref::swap::Live;
use emerge_ref::world::Event;

const DISH: &str = include_str!("../../../examples/dish.em");

fn live(src: &str) -> Live {
    let mut l = Live::new(src, &["Drifter", "Builder"], Options { seed: 5, allow_partial: true, mutation: true }).unwrap();
    l.world.contract = true;
    l
}

fn key(e: &Event) -> (u64, u8, u64, u64, u8) {
    match e {
        Event::Born { tick, parent, child } => (*tick, 1, *child, *parent, 0),
        Event::Died { tick, id, cause } => (*tick, 0, *id, 0, u8::from(*cause == emerge_ref::Cause::Starved)),
    }
}

#[test]
fn the_gpu_log_of_births_and_deaths_equals_the_cpu_events() {
    let src = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;");
    let gpu = Gpu::new();
    let mut cpu = live(&src);
    let mut a = live(&src);
    let mut with = Resident::new(&gpu, &mut a.world);
    with.enable_events(1 << 20);
    for _ in 0..200 {
        cpu.world.run_tick();
        with.tick();
    }
    // keeping the log changes nothing: the logged run still equals the CPU's state
    let s = with.snapshot();
    assert_eq!((s.n, s.next_id), (cpu.world.entities.len(), cpu.world.next_id));
    let view = StateView { ent: &s.ent, epos: &s.epos, amounts: &s.amounts, ecap: &s.ecap, sig: &s.sig, camt: &s.camt, gw: &s.gw, mw: &s.mw };
    let (be, bc) = compare_state(&cpu.world, &view, s.ns, &mut |m| println!("{m}"));
    assert_eq!(be + bc, 0, "the logged run differs from the CPU");
    let (events, dropped) = with.events().unwrap();
    assert_eq!(dropped, 0);
    let mut want: Vec<_> = cpu.world.events.iter().map(key).collect();
    want.sort();
    let mut got: Vec<_> = events.iter().map(key).collect();
    got.sort();
    assert!(want.len() > 100, "the fixture should have many events, has {}", want.len());
    assert_eq!(got, want);
}

#[test]
fn a_full_log_counts_what_it_dropped() {
    let src = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;");
    let gpu = Gpu::new();
    let mut a = live(&src);
    let mut r = Resident::new(&gpu, &mut a.world);
    r.enable_events(50);
    for _ in 0..100 { r.tick(); }
    let (events, dropped) = r.events().unwrap();
    assert_eq!(events.len(), 50);
    assert!(dropped > 0);
}
