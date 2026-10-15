//! The `emerge` command.
//!
//! `emerge fmt [--check] <file.em>` prints the canonical form of a file. With `--check`,
//! it prints nothing and fails if the file is not already canonical.
//!
//! `emerge run <file.em> --lineages A[,B] [--mutation off] [--seed N] [--ticks N] [--every N] [--flux S]`
//! compiles the world, seeds it with the lineages and runs it on the CPU reference runner,
//! auditing the ledger every `--every` ticks. It prints a CSV row per window and a final
//! `#` summary line (spec §16, exit criteria 7 and 8). `--flux S` reports the µ of `S`
//! produced by entity reactions. Mutation is not implemented, so `--mutation on` is refused.

use std::process::ExitCode;
use std::time::Instant;

use emerge_ref::compile::compile;
use emerge_ref::World;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fmt") => fmt(&args[1..]),
        Some("run") => run(&args[1..]),
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: emerge fmt [--check] <file.em>");
    eprintln!("       emerge run <file.em> --lineages A[,B] [--mutation off] [--seed N] [--ticks N] [--every N] [--flux S]");
    ExitCode::from(2)
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).map_err(|e| eprintln!("{path}: {e}")).ok()
}

fn fmt(args: &[String]) -> ExitCode {
    let (check, files): (Vec<&String>, Vec<&String>) = args.iter().partition(|a| *a == "--check");
    let [path] = files[..] else { return usage() };
    let Some(src) = read(path) else { return ExitCode::from(2) };
    let file = match emerge_lang::parse(&src) {
        Ok(f) => f,
        Err(d) => {
            eprintln!("{path}: {d}");
            return ExitCode::FAILURE;
        }
    };
    let out = emerge_lang::format(&file);
    if check.is_empty() {
        print!("{out}");
        ExitCode::SUCCESS
    } else if out == src {
        ExitCode::SUCCESS
    } else {
        eprintln!("{path}: not in canonical form (run `emerge fmt`)");
        ExitCode::FAILURE
    }
}

fn run(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else { return usage() };
    let (mut lineages, mut flux_name) = (String::new(), None);
    let (mut seed, mut ticks, mut every) = (1u64, 50_000u64, 1_000u64);
    for pair in args[1..].chunks(2) {
        let value = pair.get(1).map(String::as_str).unwrap_or("");
        let number = |v: &str| v.parse::<u64>().ok();
        match (pair[0].as_str(), number(value)) {
            ("--lineages", _) => lineages = value.to_string(),
            ("--flux", _) => flux_name = Some(value.to_string()),
            ("--mutation", _) if value == "off" => {}
            ("--mutation", _) => {
                eprintln!("--mutation {value}: mutation is not implemented; every run is `--mutation off`");
                return ExitCode::from(2);
            }
            ("--seed", Some(n)) => seed = n,
            ("--ticks", Some(n)) => ticks = n,
            ("--every", Some(n)) if n > 0 => every = n,
            _ => return usage(),
        }
    }
    let Some(src) = read(path) else { return ExitCode::from(2) };
    let names: Vec<&str> = lineages.split(',').filter(|s| !s.is_empty()).collect();
    if names.is_empty() {
        return usage();
    }
    let compiled = match compile(&src, &names, seed) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    for s in &compiled.skipped {
        eprintln!("skipped: {s}");
    }
    let mut w = compiled.world;
    let find = |n: &str| w.chem.substances.iter().position(|s| s.name == n);
    let flux = match flux_name.as_deref().map(|n| (n, find(n))) {
        None => None,
        Some((_, Some(s))) => Some(s),
        Some((n, None)) => {
            eprintln!("--flux: unknown substance `{n}`");
            return ExitCode::from(2);
        }
    };
    let pay = w.upkeep.pay;
    w.record_events = false;
    w.record_transfers = false;

    let (pay_name, flux_label) = (&w.chem.substances[pay].name, flux_name.as_deref().unwrap_or("none"));
    println!("tick,population,births,deaths,{flux_label}_synthesis_mu,mean_{pay_name}_u,mean_{flux_label}_u,cell_{flux_label}_u,ms_per_tick");
    let (mut births, mut deaths, mut made) = (0, 0, 0);
    let (mut windows, mut with_births, mut with_flux) = (0, 0, 0);
    let mut clock = Instant::now();
    while w.tick < ticks && !w.entities.is_empty() {
        w.run_tick();
        if w.tick.is_multiple_of(every) || w.entities.is_empty() {
            if let Err(e) = w.audit() {
                eprintln!("ledger drift: {e:?}");
                return ExitCode::FAILURE;
            }
            let pop = w.entities.len().max(1) as f64;
            let mean = |s: usize| w.entities.iter().map(|e| e.store.amounts[s] as f64).sum::<f64>() / pop / 1_000.0;
            let (now, mean_flux, cell_flux) = match flux {
                Some(s) => (
                    produced(&w, s),
                    mean(s),
                    w.cells.iter().map(|c| c.amounts[s] as f64).sum::<f64>() / w.cells.len().max(1) as f64 / 1_000.0,
                ),
                None => (0, 0.0, 0.0),
            };
            println!(
                "{},{},{},{},{},{:.2},{:.2},{:.3},{:.1}",
                w.tick, w.entities.len(), w.births - births, w.deaths - deaths, now - made,
                mean(pay), mean_flux, cell_flux,
                clock.elapsed().as_secs_f64() * 1_000.0 / every as f64,
            );
            windows += 1;
            with_births += (w.births > births) as u64;
            with_flux += (now > made) as u64;
            (births, deaths, made) = (w.births, w.deaths, now);
            clock = Instant::now();
        }
    }
    let survived = !w.entities.is_empty() && w.tick >= ticks;
    println!(
        "# lineages={lineages} seed={seed} ticks={} survived={survived} windows={windows} with_births={with_births} with_flux={with_flux}",
        w.tick
    );
    ExitCode::SUCCESS
}

/// µ of `s` produced by entity reactions so far.
fn produced(w: &World, s: usize) -> u64 {
    w.chem
        .reactions
        .iter()
        .enumerate()
        .map(|(r, rx)| rx.outputs.iter().filter(|o| o.0 == s).map(|o| o.1 as u64).sum::<u64>() * w.entity_extent[r])
        .sum()
}
