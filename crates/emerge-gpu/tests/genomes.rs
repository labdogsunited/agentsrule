//! Genomes read back from the GPU (`pack::decode_master`, `Resident::genome_of`): the words a genome is
//! packed into decode to the same genome, for genomes that mutation has changed, and a GPU run's
//! genomes equal the CPU's entity by entity (contract mode). The second test needs the GPU.

use emerge_gpu::gpu::Gpu;
use emerge_gpu::pack::{decode_master, master_words};
use emerge_gpu::resident::Resident;
use emerge_ref::compile::Options;
use emerge_ref::swap::Live;

const DISH: &str = include_str!("../../../examples/dish.em");

fn small() -> String {
    DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;")
}

fn live(seed: u64) -> Live {
    let mut l = Live::new(&small(), &["Drifter", "Builder"], Options { seed, allow_partial: true, mutation: true }).unwrap();
    l.world.contract = true;
    l
}

#[test]
fn packed_genomes_decode_to_themselves() {
    let mut l = live(9);
    let mut distinct = std::collections::HashSet::new();
    for tick in 0..400 {
        l.world.run_tick();
        if tick % 100 == 99 {
            for e in &l.world.entities {
                let w = master_words(&e.genome);
                assert_eq!(decode_master(&w, 0), e.genome, "tick {tick}, entity {}", e.id);
                // At an offset, among other genomes.
                let mut padded = vec![0xdead_beef; 7];
                padded.extend(&w);
                assert_eq!(decode_master(&padded, 7), e.genome);
                distinct.insert(format!("{:?}", e.genome));
            }
        }
    }
    assert!(distinct.len() > 4, "mutation changed no genomes ({} distinct)", distinct.len());
}

#[test]
fn a_gpu_runs_genomes_read_back_equal_the_cpus() {
    let gpu = Gpu::new();
    let mut cpu = live(5);
    let mut on_gpu = live(5);
    let mut r = Resident::new(&gpu, &mut on_gpu.world);
    let mut compared = 0;
    let mut distinct = std::collections::HashSet::new();
    for tick in 0..300 {
        cpu.world.run_tick();
        r.tick();
        if tick % 100 == 99 {
            assert_eq!(r.n, cpu.world.entities.len(), "tick {tick}");
            for e in &cpu.world.entities {
                let (lineage, age, genome) = r.genome_of(e.id).unwrap_or_else(|| panic!("tick {tick}: the GPU has no entity {}", e.id));
                assert_eq!((lineage, age, &genome), (e.lineage, e.age, &e.genome), "tick {tick}, entity {}", e.id);
                distinct.insert(format!("{genome:?}"));
                compared += 1;
            }
            assert!(r.genome_of(u64::MAX).is_none());
        }
    }
    assert!(compared > 100, "only {compared} genomes compared");
    assert!(distinct.len() > 4, "mutation changed no genomes on the GPU run ({} distinct)", distinct.len());
}
