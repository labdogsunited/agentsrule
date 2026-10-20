//! A view's frame is the same from either backend (`proposals/visual-language.md` V2): in contract
//! mode, the GPU run's frame equals the CPU run's tick for tick, so they draw the same picture. Needs the GPU.

use emerge_server::journal::Journal;
use emerge_server::run::{Run, Spec};

const DISH: &str = include_str!("../../../examples/dish.em");

#[test]
fn the_gpu_and_the_cpu_give_the_same_frame() {
    let source = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;");
    let spec = |gpu| Spec { source: source.clone(), lineages: vec!["Drifter".into(), "Builder".into()], seed: 2, mutation: true, contract: true, gpu };
    let (mut cpu, mut gpu) = (Run::create(spec(false), Journal::memory()).unwrap(), Run::create(spec(true), Journal::memory()).unwrap());
    for _ in 0..4 {
        cpu.step(25);
        gpu.step(25);
        let (a, b) = (cpu.frame(), gpu.frame());
        assert!(!a.agents.is_empty() && a.tick == b.tick);
        assert!(a == b, "frames differ at tick {}", a.tick);
    }
}
