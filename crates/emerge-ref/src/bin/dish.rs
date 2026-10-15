//! Runs the reference world and reports the Phase 0 baseline and control checks
//! (spec §16, exit criteria 7 and 8). Mutation is not implemented, so every run is
//! `--mutation off`.
//!
//! Usage: dish [--lineage drifter|builder] [--seed N] [--ticks N] [--every N]

use std::time::Instant;

use emerge_ref::dish::{self, ids, Lineage};

fn main() {
    let mut lineage = Lineage::Drifter;
    let (mut seed, mut ticks, mut every) = (1u64, 50_000u64, 1_000u64);
    let args: Vec<String> = std::env::args().skip(1).collect();
    for pair in args.chunks(2) {
        let value = pair.get(1).map(String::as_str).unwrap_or("");
        match pair[0].as_str() {
            "--lineage" => lineage = if value == "builder" { Lineage::Builder } else { Lineage::Drifter },
            "--seed" => seed = value.parse().expect("--seed"),
            "--ticks" => ticks = value.parse().expect("--ticks"),
            "--every" => every = value.parse().expect("--every"),
            other => panic!("unknown argument {other}"),
        }
    }

    let mut w = dish::world(lineage, seed);
    w.record_events = false;
    w.record_transfers = false;
    println!("tick,population,births,deaths,body_synthesis_mu,mean_energy_u,mean_body_u,cell_body_u,ms_per_tick");
    let (mut births, mut deaths, mut body) = (0, 0, 0);
    let (mut windows, mut with_births, mut with_flux) = (0, 0, 0);
    let mut clock = Instant::now();
    while w.tick < ticks && !w.entities.is_empty() {
        w.run_tick();
        if w.tick.is_multiple_of(every) || w.entities.is_empty() {
            if let Err(e) = w.audit() {
                panic!("ledger drift: {e:?}");
            }
            let pop = w.entities.len().max(1) as f64;
            let mean = |s: usize| w.entities.iter().map(|e| e.store.amounts[s] as f64).sum::<f64>() / pop / 1_000.0;
            let cell_body = w.cells.iter().map(|c| c.amounts[ids::BODY] as f64).sum::<f64>() / w.cells.len() as f64 / 1_000.0;
            let flux = dish::body_synthesis(&w) - body;
            println!(
                "{},{},{},{},{},{:.2},{:.2},{:.3},{:.1}",
                w.tick, w.entities.len(), w.births - births, w.deaths - deaths, flux,
                mean(ids::ENERGY), mean(ids::BODY), cell_body,
                clock.elapsed().as_secs_f64() * 1_000.0 / every as f64,
            );
            windows += 1;
            with_births += (w.births > births) as u64;
            with_flux += (flux > 0) as u64;
            (births, deaths, body) = (w.births, w.deaths, dish::body_synthesis(&w));
            clock = Instant::now();
        }
    }
    let survived = !w.entities.is_empty() && w.tick >= ticks;
    println!(
        "# lineage={lineage:?} seed={seed} ticks={} survived={survived} windows={windows} with_births={with_births} with_flux={with_flux}",
        w.tick
    );
}
