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
//! `emerge run <file.em> --lineages A[,B] [--mutation off] [--partial] [--seed N] [--ticks N] [--every N] [--flux S] [--stop-when-sustained] [--stop-when-hopeless] [--features] [--producers F] [--census N]`
//! compiles the world, seeds it with the lineages and runs it on the CPU reference runner,
//! auditing the ledger every `--every` ticks. It prints a CSV row per window and a final
//! `#` summary line (spec §16, exit criteria 7–9). Children mutate (spec §8.1) unless
//! `--mutation off`. With `--flux S`, `carriers` is the fraction of entities with an active
//! reaction gene that makes `S`, and `producers` the fraction whose reactions made some `S`
//! in the window. The summary's `sustained_at` is the first tick at which flux has been
//! above zero with at least `--producers F` producers (default `0.02`) in every window
//! for 10_000 ticks (exit test 9);
//! `--stop-when-sustained` ends the run there. `--stop-when-hopeless` ends a run that can no longer
//! pass (the ticks left plus the current streak are under 10_000); the summary then ends with
//! `stopped=cannot_pass`. `--census N` prints the `N` most common
//! genomes at the end, and the most common one that makes the `--flux` substance, in
//! `.em` syntax on `#` lines. `stolen_mu` is the µ moved by `steal` in the
//! window, and `thieves` the fraction of entities with an active `steal` gene. A world with parts the runner cannot
//! run yet (signal fields and their rules, menu items it does not implement) is refused unless `--partial` is given. `--flux S` reports the µ of `S`
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
    eprintln!("       emerge run <file.em> --lineages A[,B] [--mutation off] [--partial] [--seed N] [--ticks N] [--every N] [--flux S] [--stop-when-sustained] [--stop-when-hopeless] [--producers F] [--census N]");
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
    let (mut partial, mut mutation, mut stop, mut hopeless_stop, mut features) = (false, true, false, false, false);
    let (mut census, mut threshold) = (0, 0.02);
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        if flag == "--partial" || flag == "--stop-when-sustained" || flag == "--stop-when-hopeless" || flag == "--features" {
            partial |= flag == "--partial";
            features |= flag == "--features";
            stop |= flag == "--stop-when-sustained";
            hopeless_stop |= flag == "--stop-when-hopeless";
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
            ("--census", Some(n)) => census = n as usize,
            ("--producers", _) if value.parse::<f64>().is_ok_and(|f| (0.0..=1.0).contains(&f)) => threshold = value.parse().unwrap(),
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
    let field = compiled.field.unwrap_or_default();
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
    w.track = flux;
    w.record_transfers = false;

    let (pay_name, flux_label) = (&w.chem.substances[pay].name, flux_name.as_deref().unwrap_or("none"));
    println!("tick,population,births,deaths,{flux_label}_synthesis_mu,mean_{pay_name}_u,mean_{flux_label}_u,cell_{flux_label}_u,carriers,producers,mean_active_genes,stolen_mu,thieves,ms_per_tick");
    let (mut births, mut deaths, mut made, mut stolen) = (0, 0, 0, 0);
    let (mut windows, mut with_births, mut with_flux) = (0, 0, 0);
    // Exit test 9: flux above zero, made by at least `threshold` of the population in every
    // window, for 10_000 ticks in a row. `sustained` is the first tick at which that holds.
    let (mut streak, mut sustained) = (0u64, None);
    let mut hopeless = false;
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
            // Producers: entities alive now whose reactions made some of `S` in this window.
            let producers = w.entities.iter().filter(|e| e.made > 0).count() as f64 / pop;
            let genes = w.entities.iter().map(|e| e.genome.active_genes()).sum::<usize>() as f64 / pop;
            let thieves = w.entities.iter().filter(|e| steals(e)).count() as f64 / pop;
            println!(
                "{},{},{},{},{},{:.2},{:.2},{:.3},{:.4},{:.4},{:.2},{},{:.4},{:.1}",
                w.tick, w.entities.len(), w.births - births, w.deaths - deaths, now - made,
                mean(pay), mean_flux, cell_flux, carriers, producers, genes, w.stolen - stolen, thieves,
                clock.elapsed().as_secs_f64() * 1_000.0 / every as f64,
            );
            windows += 1;
            with_births += (w.births > births) as u64;
            with_flux += (now > made) as u64;
            streak = if now > made && producers >= threshold { streak + every } else { 0 };
            if streak >= 10_000 && sustained.is_none() {
                sustained = Some(w.tick);
            }
            (births, deaths, made, stolen) = (w.births, w.deaths, now, w.stolen);
            // Before the reset below, so `made` still holds this window.
            if features && (w.tick >= ticks || w.entities.is_empty()) {
                print_features(&w);
            }
            w.entities.iter_mut().for_each(|e| e.made = 0);
            clock = Instant::now();
            if stop && sustained.is_some() {
                break;
            }
            // The run cannot pass any more: the ticks left, plus the streak so far, are under 10_000.
            if hopeless_stop && sustained.is_none() && streak + (ticks - w.tick) < 10_000 {
                hopeless = true;
                break;
            }
        }
    }
    let survived = !w.entities.is_empty() && (w.tick >= ticks || sustained.is_some() || hopeless);
    let sustained = sustained.map_or("never".to_string(), |t| t.to_string());
    println!(
        "# lineages={lineages} seed={seed} mutation={} ticks={} survived={survived} windows={windows} with_births={with_births} with_flux={with_flux} sustained_at={sustained}{}",
        if mutation { "on" } else { "off" },
        w.tick,
        if hopeless { " stopped=cannot_pass" } else { "" }
    );
    if census > 0 {
        print_census(&w, &field, census, flux);
    }
    ExitCode::SUCCESS
}

/// The `n` most common genomes, and the most common one that makes `flux`, as `#` lines.
fn print_census(w: &World, field: &str, n: usize, flux: Option<usize>) {
    let mut count: std::collections::HashMap<String, (usize, usize)> = std::collections::HashMap::new();
    for (i, e) in w.entities.iter().enumerate() {
        count.entry(format!("{:?}", e.genome)).or_insert((0, i)).0 += 1;
    }
    let mut ranked: Vec<(usize, usize)> = count.into_values().collect();
    ranked.sort_by_key(|&(c, i)| (std::cmp::Reverse(c), w.entities[i].id));
    let pop = w.entities.len().max(1) as f64;
    let show = |title: String, i: usize| {
        println!("# {title}");
        for line in emerge_ref::print::genome(w, &w.entities[i].genome, field).lines() {
            println!("#   {line}");
        }
    };
    println!("# census: {} distinct genomes among {} entities", ranked.len(), w.entities.len());
    for (rank, &(c, i)) in ranked.iter().take(n).enumerate() {
        show(format!("#{} of {}: {c} entities ({:.1}%)", rank + 1, ranked.len(), c as f64 / pop * 100.0), i);
    }
    if let Some(s) = flux {
        let name = &w.chem.substances[s].name;
        match ranked.iter().find(|&&(_, i)| makes(w, &w.entities[i], s)) {
            Some(&(c, i)) => show(format!("most common genome that makes {name}: {c} entities ({:.1}%)", c as f64 / pop * 100.0), i),
            None => println!("# no genome makes {name}"),
        }
    }
}

/// Whether an entity has an active reaction gene that produces `s`.
fn makes(w: &World, e: &emerge_ref::Entity, s: usize) -> bool {
    e.genome.slots.iter().enumerate().any(|(g, slot)| {
        e.genome.active(g) && matches!(slot.gene, Gene::React { reaction, .. } if w.chem.reactions[reaction].outputs.iter().any(|o| o.0 == s))
    })
}

/// Whether an entity has an active `steal` gene.
fn steals(e: &emerge_ref::Entity) -> bool {
    e.genome.slots.iter().enumerate().any(|(g, slot)| e.genome.active(g) && matches!(slot.gene, Gene::Steal { .. }))
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

/// What an active gene is, as a short label: its kind and its substance or reaction.
fn feature_labels(w: &World, e: &emerge_ref::Entity) -> std::collections::BTreeSet<String> {
    use emerge_ref::mind::Sense;
    let sub = |s: usize| w.chem.substances[s].name.clone();
    let side = |t: &[(usize, u32)]| t.iter().map(|&(s, c)| if c == 1 { sub(s) } else { format!("{c} {}", sub(s)) }).collect::<Vec<_>>().join(" + ");
    let mut out = std::collections::BTreeSet::new();
    for (g, slot) in e.genome.slots.iter().enumerate().filter(|(g, _)| e.genome.active(*g)) {
        let _ = g;
        out.insert(match &slot.gene {
            Gene::Sense { sense, .. } => match sense {
                Sense::Inventory(s) => format!("sense inventory({})", sub(*s)),
                Sense::Cell(s) => format!("sense cell({})", sub(*s)),
                Sense::Gradient(s) => format!("sense gradient({})", sub(*s)),
                Sense::Probe { s, .. } => format!("sense probe({})", sub(*s)),
                Sense::Crowd { .. } => "sense crowd".to_string(),
                Sense::Neighbour { s, .. } => format!("sense neighbour({})", sub(*s)),
                Sense::Clock { .. } => "sense clock".to_string(),
            },
            Gene::Weigh { inputs, .. } => format!("regulate weigh/{}", inputs.len()),
            Gene::Product { .. } => "regulate product".to_string(),
            Gene::React { reaction, .. } => {
                let r = &w.chem.reactions[*reaction];
                format!("reaction {} -> {}", side(&r.inputs), side(&r.outputs))
            }
            Gene::Move { .. } => "action move".to_string(),
            Gene::Turn { .. } => "action turn".to_string(),
            Gene::Take { s, .. } => format!("action take({})", sub(*s)),
            Gene::Drop { s, .. } => format!("action drop({})", sub(*s)),
            Gene::Give { s, .. } => format!("action give({})", sub(*s)),
            Gene::Steal { s, .. } => format!("action steal({})", sub(*s)),
            Gene::Divide { .. } => "action divide".to_string(),
        });
    }
    out
}

/// Share of producers (`made > 0`) and of the other entities that have each kind of active gene.
fn print_features(w: &World) {
    let mut count: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
    let (mut p, mut o) = (0usize, 0usize);
    for e in &w.entities {
        let producer = e.made > 0;
        if producer { p += 1 } else { o += 1 }
        for l in feature_labels(w, e) {
            let c = count.entry(l).or_default();
            if producer { c.0 += 1 } else { c.1 += 1 }
        }
    }
    println!("# features: {p} producers, {o} others (share of each group with an active gene of this kind)");
    let mut rows: Vec<(String, f64, f64)> = count
        .into_iter()
        .map(|(l, (a, b))| (l, a as f64 / p.max(1) as f64, b as f64 / o.max(1) as f64))
        .collect();
    rows.sort_by(|x, y| (y.1 - y.2).abs().total_cmp(&(x.1 - x.2).abs()));
    for (l, a, b) in rows.iter().take(30) {
        println!("#   {a:6.1}% vs {b:6.1}%  {l}", a = a * 100.0, b = b * 100.0);
    }
}
