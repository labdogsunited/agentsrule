//! The `emerge` command.
//!
//! `emerge fmt [--check] <file.em>` prints the canonical form of a file. With `--check`,
//! it prints nothing and fails if the file is not already canonical.
//!
//! `emerge check [--json] <file.em>` reports every problem it can find without running the
//! world: syntax, the language's static rules (chemistry, names, seeds, upkeep), and
//! compiling the world with all its lineages. Each diagnostic has a code, a position and the
//! item's path (`language.md` §14); `--json` prints one JSON object per line. It exits with
//! 1 if there is an error.
//!
//! `emerge run <file.em> --lineages A[,B] [--mutation off] [--partial] [--seed N] [--ticks N] [--every N] [--flux S] [--stop-when-sustained]`
//! compiles the world, seeds it with the lineages and runs it on the CPU reference runner,
//! auditing the ledger every `--every` ticks. It prints a CSV row per window and a final
//! `#` summary line (spec §16, exit criteria 7–9). Children mutate (spec §8.1) unless
//! `--mutation off`. With `--flux S`, `carriers` is the fraction of entities with an active
//! reaction gene that makes `S`, and the summary's `sustained_at` is the first tick at which
//! flux has been above zero with at least 10% carriers for 10_000 ticks (exit test 9);
//! `--stop-when-sustained` ends the run there. A world with parts the runner cannot
//! run yet (signal fields and their rules) is refused unless `--partial` is given. `--flux S` reports the µ of `S`
//! produced by entity reactions.

use std::process::ExitCode;
use std::time::Instant;

use emerge_ref::compile::{compile, Options};
use emerge_ref::Gene;
use emerge_ref::World;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fmt") => fmt(&args[1..]),
        Some("check") => check(&args[1..]),
        Some("run") => run(&args[1..]),
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: emerge fmt [--check] <file.em>");
    eprintln!("       emerge check [--json] <file.em>");
    eprintln!("       emerge run <file.em> --lineages A[,B] [--mutation off] [--partial] [--seed N] [--ticks N] [--every N] [--flux S] [--stop-when-sustained]");
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

/// A diagnostic as `language.md` §14 describes it.
struct Diagnostic {
    code: &'static str,
    severity: &'static str,
    path: String,
    line: usize,
    col: usize,
    message: String,
    /// A replacement for the item at `path`.
    fix: Option<String>,
}

fn check(args: &[String]) -> ExitCode {
    let (json, files): (Vec<&String>, Vec<&String>) = args.iter().partition(|a| *a == "--json");
    let [file_path] = files[..] else { return usage() };
    let Some(src) = read(file_path) else { return ExitCode::from(2) };
    let mut out = Vec::new();
    match emerge_lang::parse(&src) {
        Err(d) => out.push(Diagnostic { code: d.code, severity: "error", path: String::new(), line: d.line, col: d.col, message: d.message, fix: None }),
        Ok(file) => {
            for p in emerge_lang::check::check(&file) {
                let (line, col) = (p.pos.line, p.pos.col);
                out.push(Diagnostic { code: p.code, severity: "error", path: p.path, line, col, message: p.message, fix: p.fix });
            }
            // Compiling needs a valid chemistry; it stops at its first error.
            if out.is_empty() {
                let lineages = emerge_ref::compile::lineage_names(&file);
                let diag = |e: emerge_ref::compile::Error, severity| Diagnostic {
                    code: e.code, severity, path: e.path, line: e.pos.line, col: e.pos.col, message: e.message, fix: None,
                };
                let options = Options { seed: 1, allow_partial: true, mutation: true };
                match emerge_ref::compile::build(&file, &lineages, options) {
                    Err(e) => out.push(diag(e, "error")),
                    Ok(c) => out.extend(c.partial.into_iter().map(|e| diag(e, "warning"))),
                }
            }
        }
    }
    for d in &out {
        if json.is_empty() {
            let at = if d.line > 0 { format!("{}:{}:", d.line, d.col) } else { String::new() };
            let path = if d.path.is_empty() { String::new() } else { format!(" {}:", d.path) };
            println!("{file_path}:{at} {}[{}]:{path} {}", d.severity, d.code, d.message);
            if let Some(fix) = &d.fix {
                println!("    fix: write it as `{fix}`");
            }
        } else {
            let fixes = match &d.fix {
                Some(f) => format!(
                    r#"[{{"title":"write it in canonical form","patch":{{"ops":[{{"op":"replace","path":{},"value":{}}}]}}}}]"#,
                    quote(&d.path),
                    quote(f)
                ),
                None => "[]".into(),
            };
            println!(
                r#"{{"code":{},"severity":{},"path":{},"span":{{"file":{},"line":{},"col":{}}},"message":{},"fixes":{fixes}}}"#,
                quote(d.code), quote(d.severity), quote(&d.path), quote(file_path), d.line, d.col, quote(&d.message)
            );
        }
    }
    if out.iter().any(|d| d.severity == "error") {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// A JSON string literal.
fn quote(s: &str) -> String {
    let mut q = String::from('"');
    for c in s.chars() {
        match c {
            '"' => q.push_str("\\\""),
            '\\' => q.push_str("\\\\"),
            '\n' => q.push_str("\\n"),
            c if (c as u32) < 0x20 => q.push_str(&format!("\\u{:04x}", c as u32)),
            c => q.push(c),
        }
    }
    q.push('"');
    q
}

fn run(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else { return usage() };
    let (mut lineages, mut flux_name) = (String::new(), None);
    let (mut seed, mut ticks, mut every) = (1u64, 50_000u64, 1_000u64);
    let (mut partial, mut mutation, mut stop) = (false, true, false);
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        if flag == "--partial" || flag == "--stop-when-sustained" {
            partial |= flag == "--partial";
            stop |= flag == "--stop-when-sustained";
            continue;
        }
        let value = rest.next().map(String::as_str).unwrap_or("");
        let number = |v: &str| v.parse::<u64>().ok();
        match (flag.as_str(), number(value)) {
            ("--lineages", _) => lineages = value.to_string(),
            ("--flux", _) => flux_name = Some(value.to_string()),
            ("--mutation", _) if value == "off" || value == "on" => mutation = value == "on",
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
    let compiled = match compile(&src, &names, Options { seed, allow_partial: partial, mutation }) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    for s in &compiled.notes {
        eprintln!("note: {s}");
    }
    for s in &compiled.partial {
        eprintln!("left out of this partial run: {s}");
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
    println!("tick,population,births,deaths,{flux_label}_synthesis_mu,mean_{pay_name}_u,mean_{flux_label}_u,cell_{flux_label}_u,carriers,mean_active_genes,ms_per_tick");
    let (mut births, mut deaths, mut made) = (0, 0, 0);
    let (mut windows, mut with_births, mut with_flux) = (0, 0, 0);
    // Exit test 9: flux above zero, carried by at least 10% of the population, for 10_000
    // ticks in a row. `sustained` is the first tick at which that holds.
    let (mut streak, mut sustained) = (0u64, None);
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
            let carriers = flux.map_or(0.0, |s| w.entities.iter().filter(|e| makes(&w, e, s)).count() as f64 / pop);
            let genes = w.entities.iter().map(|e| e.genome.active_genes()).sum::<usize>() as f64 / pop;
            println!(
                "{},{},{},{},{},{:.2},{:.2},{:.3},{:.4},{:.2},{:.1}",
                w.tick, w.entities.len(), w.births - births, w.deaths - deaths, now - made,
                mean(pay), mean_flux, cell_flux, carriers, genes,
                clock.elapsed().as_secs_f64() * 1_000.0 / every as f64,
            );
            windows += 1;
            with_births += (w.births > births) as u64;
            with_flux += (now > made) as u64;
            streak = if now > made && carriers >= 0.10 { streak + every } else { 0 };
            if streak >= 10_000 && sustained.is_none() {
                sustained = Some(w.tick);
            }
            (births, deaths, made) = (w.births, w.deaths, now);
            clock = Instant::now();
            if stop && sustained.is_some() {
                break;
            }
        }
    }
    let survived = !w.entities.is_empty() && (w.tick >= ticks || sustained.is_some());
    let sustained = sustained.map_or("never".to_string(), |t| t.to_string());
    println!(
        "# lineages={lineages} seed={seed} mutation={} ticks={} survived={survived} windows={windows} with_births={with_births} with_flux={with_flux} sustained_at={sustained}",
        if mutation { "on" } else { "off" },
        w.tick
    );
    ExitCode::SUCCESS
}

/// Whether an entity has an active reaction gene that produces `s`.
fn makes(w: &World, e: &emerge_ref::Entity, s: usize) -> bool {
    e.genome.slots.iter().enumerate().any(|(g, slot)| {
        e.genome.active(g) && matches!(slot.gene, Gene::React { reaction, .. } if w.chem.reactions[reaction].outputs.iter().any(|o| o.0 == s))
    })
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
