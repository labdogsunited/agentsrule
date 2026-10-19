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
//! `emerge run <file.em> --lineages A[,B] [--mutation off] [--partial] [--seed N] [--ticks N] [--every N] [--flux S] [--stop-when-sustained] [--stop-when-hopeless] [--features] [--cell S] [--producers F] [--census N]`
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
        Some("census") => census(&args[1..]),
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: emerge fmt [--check] <file.em>");
    eprintln!("       emerge check [--json] <file.em>");
    eprintln!("       emerge run <file.em> --lineages A[,B] [--mutation off] [--partial] [--seed N] [--ticks N] [--every N] [--flux S] [--stop-when-sustained] [--stop-when-hopeless] [--producers F] [--census N] [--inject NAME@TICK] [--blocked] [--dump] [--one-per-substance] [--ancestry N]");
    eprintln!("       emerge census <file.em> --lineage A [--flux S] [--lines N] [--generations N]");
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

/// Substances that can never appear, and reactions that can never run, as notes.
fn reach_notes(w: &World) -> Vec<Diagnostic> {
    let r = emerge_ref::reach::reach(w);
    let sub = |s: usize| w.chem.substances[s].name.clone();
    let rx = |i: usize| w.chem.formula(&w.chem.reactions[i].inputs, &w.chem.reactions[i].outputs);
    let note = |code, message| Diagnostic { code, severity: "note", path: String::new(), line: 0, col: 0, message, fix: None };
    let mut out = Vec::new();
    let never: Vec<usize> = (0..r.possible.len()).filter(|&s| !r.possible[s]).collect();
    if !never.is_empty() {
        let names = never.iter().map(|&s| format!("`{}`", sub(s))).collect::<Vec<_>>().join(", ");
        let why = r.witness.iter().map(|&(i, s)| format!("{} needs `{}`", rx(i), sub(s))).collect::<Vec<_>>().join("; ");
        let why = if why.is_empty() { "no reaction makes any of them".to_string() } else { format!("every reaction that makes one needs one: {why}") };
        out.push(note("N1101", format!("can never appear: {names}. No store starts with any of them, no inflow adds one, and {why}")));
    }
    if !r.dead.is_empty() {
        let space = w.menus.reactions.len();
        let dead_space = w.menus.reactions.iter().filter(|(i, _)| r.dead.contains(i)).count();
        let shown = r.dead.iter().take(5).map(|&i| rx(i)).collect::<Vec<_>>().join("; ");
        let more = if r.dead.len() > 5 { format!("; and {} more", r.dead.len() - 5) } else { String::new() };
        out.push(note(
            "N1102",
            format!("can never run: {} reaction(s), {dead_space} of the {space} in the reaction space, because an input can never appear: {shown}{more}", r.dead.len()),
        ));
    }
    out
}

/// `steal` of the `pay` substance as net income (`proposals/budget-check.md`).
fn budget_notes(w: &World) -> Vec<Diagnostic> {
    let Some((gain, spend)) = emerge_ref::budget::steal_pays(w) else { return Vec::new() };
    let pay = &w.chem.substances[w.upkeep.pay].name;
    let message = format!(
        "`steal({pay})` is net income: one gene takes up to {gain}µ per tick for {spend}µ (fee plus gene upkeep), whenever a neighbour holds `{pay}`"
    );
    vec![Diagnostic { code: "N1103", severity: "note", path: String::new(), line: 0, col: 0, message, fix: None }]
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
                    Ok(c) => {
                        out.extend(c.partial.into_iter().map(|e| diag(e, "warning")));
                        out.extend(reach_notes(&c.world));
                        out.extend(budget_notes(&c.world));
                    }
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
    let mut cell_name: Option<String> = None;
    let (mut seed, mut ticks, mut every) = (1u64, 50_000u64, 1_000u64);
    let (mut partial, mut mutation, mut stop, mut hopeless_stop, mut features) = (false, true, false, false, false);
    let (mut census, mut threshold) = (0, 0.02);
    let (mut blocked, mut dump, mut one_per_substance) = (false, false, false);
    let mut ancestry_every = 0u64;
    let mut inject: Option<(String, u64)> = None;
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        if flag == "--partial" || flag == "--stop-when-sustained" || flag == "--stop-when-hopeless" || flag == "--features" || flag == "--blocked" || flag == "--dump" || flag == "--one-per-substance" {
            partial |= flag == "--partial";
            features |= flag == "--features";
            blocked |= flag == "--blocked";
            dump |= flag == "--dump";
            one_per_substance |= flag == "--one-per-substance";
            stop |= flag == "--stop-when-sustained";
            hopeless_stop |= flag == "--stop-when-hopeless";
            continue;
        }
        let value = rest.next().map(String::as_str).unwrap_or("");
        let number = |v: &str| v.parse::<u64>().ok();
        match (flag.as_str(), number(value)) {
            ("--lineages", _) => lineages = value.to_string(),
            ("--inject", _) => match value.split_once('@').map(|(n, t)| (n, t.parse::<u64>())) {
                Some((n, Ok(t))) if !n.is_empty() => inject = Some((n.to_string(), t)),
                _ => return usage(),
            },
            ("--flux", _) => flux_name = Some(value.to_string()),
            ("--cell", _) => cell_name = Some(value.to_string()),
            ("--mutation", _) if value == "off" || value == "on" => mutation = value == "on",
            ("--seed", Some(n)) => seed = n,
            ("--ticks", Some(n)) => ticks = n,
            ("--every", Some(n)) if n > 0 => every = n,
            ("--census", Some(n)) => census = n as usize,
            ("--ancestry", Some(n)) if n > 0 => ancestry_every = n,
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
    // The invader is compiled alone from the same file, then added to the running world.
    let mut invader = match &inject {
        None => None,
        Some((name, _)) => match compile(&src, &[name.as_str()], Options { seed, allow_partial: partial, mutation }) {
            Ok(c) => Some(c.world),
            Err(e) => {
                eprintln!("{path}: --inject: {e}");
                return ExitCode::FAILURE;
            }
        },
    };
    let mut injected: Option<usize> = None;
    let field = compiled.field.unwrap_or_default();
    // What this run leaves out, so the result says it is partial (not only stderr).
    let left_out = if compiled.partial.is_empty() {
        String::new()
    } else {
        format!(" left_out={}", compiled.partial.iter().map(|e| e.path.as_str()).collect::<Vec<_>>().join(","))
    };
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
    let cell_sub = match cell_name.as_deref().map(|n| (n, find(n))) {
        None => None,
        Some((_, Some(s))) => Some(s),
        Some((n, None)) => {
            eprintln!("--cell: unknown substance `{n}`");
            return ExitCode::from(2);
        }
    };
    let pay = w.upkeep.pay;
    w.record_events = false;
    w.track = flux;
    w.record_transfers = false;
    w.one_per_substance = one_per_substance;
    // Ancestry needs each birth's parent; recording events changes no state.
    w.record_events = ancestry_every > 0 && flux.is_some();
    let mut ancestry = Ancestry::default();
    if w.record_events {
        ancestry.start(&w, flux.unwrap());
    }
    if blocked {
        w.blocked = Some(vec![Default::default(); w.chem.reactions.len()]);
        w.binding = Some(Default::default());
    }

    let (pay_name, flux_label) = (&w.chem.substances[pay].name, flux_name.as_deref().unwrap_or("none"));
    let mut extra = cell_name.as_deref().map_or(String::new(), |n| format!(",cell_{n}_u,held_{n}_u"));
    extra += ",producer_count";
    if let Some((n, _)) = &inject {
        extra += &format!(",share_{n}");
    }
    let guard = w.menus.guard.is_some();
    if guard {
        extra += ",guarders,guarded_mu";
    }
    // With more than one lineage, each one's share of the population, in `--lineages` order.
    if names.len() > 1 {
        for n in &names {
            extra += &format!(",share_{n}");
        }
    }
    // Run manifest: what made this file, before the column header (the summary repeats it).
    println!("# binary={} dish={} args={}", binary_digest(), digest(src.as_bytes()), std::env::args().skip(1).collect::<Vec<_>>().join(" "));
    println!("tick,population,births,deaths,{flux_label}_synthesis_mu,mean_{pay_name}_u,mean_{flux_label}_u,cell_{flux_label}_u,carriers,producers,mean_active_genes,stolen_mu,thieves,ms_per_tick{extra}");
    let (mut births, mut deaths, mut made, mut stolen, mut guarded) = (0, 0, 0, 0, 0);
    let (mut windows, mut with_births, mut with_flux) = (0, 0, 0);
    let mut last_row = 0;
    // Exit test 9: flux above zero, made by at least `threshold` of the population in every
    // window, for 10_000 ticks in a row. `sustained` is the first tick at which that holds.
    let (mut streak, mut sustained) = (0u64, None);
    let mut hopeless = false;
    let mut clock = Instant::now();
    while w.tick < ticks && !w.entities.is_empty() {
        if w.record_events && w.tick > 0 {
            let s = flux.unwrap();
            ancestry.follow(&mut w);
            if w.tick.is_multiple_of(ancestry_every) {
                ancestry.finish(&w, s);
                ancestry.start(&w, s);
            }
        }
        if let Some((_, at)) = &inject {
            if w.tick == *at {
                if let Some(other) = invader.take() {
                    injected = Some(w.inject(other, 0));
                    eprintln!("injected {} entities at tick {at}", w.entities.iter().filter(|e| Some(e.lineage) == injected).count());
                }
            }
        }
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
            let producer_count = w.entities.iter().filter(|e| e.made > 0).count();
            let producers = producer_count as f64 / pop;
            let genes = w.entities.iter().map(|e| e.genome.active_genes()).sum::<usize>() as f64 / pop;
            let thieves = w.entities.iter().filter(|e| steals(e)).count() as f64 / pop;
            let mut extra = cell_sub.map_or(String::new(), |s| {
                let cells = w.cells.iter().map(|c| c.amounts[s] as f64).sum::<f64>() / w.cells.len().max(1) as f64 / 1_000.0;
                format!(",{cells:.3},{:.3}", mean(s))
            });
            extra += &format!(",{producer_count}");
            if inject.is_some() {
                let n = w.entities.iter().filter(|e| Some(e.lineage) == injected).count();
                extra += &format!(",{:.4}", n as f64 / pop);
            }
            if guard {
                let guarders = w.entities.iter().filter(|e| guards(e)).count() as f64 / pop;
                extra += &format!(",{guarders:.4},{}", w.guarded - guarded);
            }
            if names.len() > 1 {
                let mut n = vec![0usize; names.len()];
                for e in &w.entities {
                    if let Some(k) = n.get_mut(e.lineage) {
                        *k += 1;
                    }
                }
                for k in n {
                    extra += &format!(",{:.4}", k as f64 / pop);
                }
            }
            println!(
                "{},{},{},{},{},{:.2},{:.2},{:.3},{:.4},{:.4},{:.2},{},{:.4},{:.1}{extra}",
                w.tick, w.entities.len(), w.births - births, w.deaths - deaths, now - made,
                mean(pay), mean_flux, cell_flux, carriers, producers, genes, w.stolen - stolen, thieves,
                clock.elapsed().as_secs_f64() * 1_000.0 / every as f64,
            );
            windows += 1;
            last_row = w.tick;
            with_births += (w.births > births) as u64;
            with_flux += (now > made) as u64;
            streak = if now > made && producers >= threshold { streak + every } else { 0 };
            if streak >= 10_000 && sustained.is_none() {
                sustained = Some(w.tick);
            }
            (births, deaths, made, stolen, guarded) = (w.births, w.deaths, now, w.stolen, w.guarded);
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
    // Why the run ended: `end` is the requested horizon; the others stop early.
    let stop = if w.entities.is_empty() {
        "extinct"
    } else if sustained.is_some() && w.tick < ticks {
        "sustained"
    } else if hopeless {
        "cannot_pass"
    } else {
        "end"
    };
    let sustained = sustained.map_or("never".to_string(), |t| t.to_string());
    println!(
        "# lineages={lineages} seed={seed} mutation={} ticks={} requested={ticks} stop={stop} alive={} last_row={last_row} every={every} producers_threshold={threshold} windows={windows} with_births={with_births} with_flux={with_flux} sustained_at={sustained}{}{left_out}",
        if mutation { "on" } else { "off" },
        w.tick,
        !w.entities.is_empty(),
        if hopeless { " stopped=cannot_pass" } else { "" }
    );
    println!("# binary={} dish={} args={}", binary_digest(), digest(src.as_bytes()), std::env::args().skip(1).collect::<Vec<_>>().join(" "));
    if census > 0 {
        print_census(&w, &field, census, flux);
    }
    if let Some(b) = &w.blocked {
        print_blocked(&w, b);
    }
    if let Some(b) = &w.binding {
        print_binding(&w, b);
    }
    if dump {
        print_dump(&w);
    }
    if w.record_events {
        ancestry.follow(&mut w);
        if w.tick > ancestry.t0 {
            ancestry.finish(&w, flux.unwrap());
        }
        ancestry.print(flux_label);
    }
    ExitCode::SUCCESS
}

/// FNV-1a of this executable's bytes, so a result names the build that made it.
fn binary_digest() -> String {
    digest(&std::env::current_exe().and_then(std::fs::read).unwrap_or_default())
}

/// FNV-1a, as `fnv64:<hex>`.
fn digest(bytes: &[u8]) -> String {
    let h = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
    format!("fnv64:{h:016x}")
}

/// Splits the change in the share of carriers (entities with an expressed reaction gene,
/// rate above zero, that makes the tracked substance) over a window into two exact parts
/// (the Price equation, over roots: the entities alive at the window's start):
/// `selection = Cov(w, z) / mean(w)`, from carriers leaving more or fewer representatives,
/// and `transmission = mean(w (z' - z)) / mean(w)`, from representatives gaining or losing
/// the machinery. `w`: a root's representatives alive at the end (itself included), `z`: whether
/// it carried at the start, `z'`: its representatives' share of carriers at the end.
#[derive(Default)]
struct Ancestry {
    t0: u64,
    /// Root index of each living entity, by ID.
    root: std::collections::HashMap<u64, usize>,
    /// Each root's trait at the start.
    z: Vec<bool>,
    rows: Vec<String>,
}

impl Ancestry {
    fn start(&mut self, w: &World, s: usize) {
        self.t0 = w.tick;
        self.root = w.entities.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        self.z = w.entities.iter().map(|e| carries(w, e, s)).collect();
    }

    /// Children join their parent's root; the dead leave. Clears the events.
    fn follow(&mut self, w: &mut World) {
        for ev in w.events.drain(..) {
            match ev {
                emerge_ref::Event::Born { parent, child, .. } => {
                    // An injected lineage has no root until the next window starts.
                    if let Some(&r) = self.root.get(&parent) {
                        self.root.insert(child, r);
                    }
                }
                emerge_ref::Event::Died { id, .. } => {
                    self.root.remove(&id);
                }
            }
        }
    }

    fn finish(&mut self, w: &World, s: usize) {
        let n = self.z.len();
        if n == 0 {
            return;
        }
        let (mut count, mut carriers) = (vec![0u64; n], vec![0u64; n]);
        for e in &w.entities {
            let Some(&r) = self.root.get(&e.id) else { continue };
            count[r] += 1;
            carriers[r] += carries(w, e, s) as u64;
        }
        let pop: u64 = count.iter().sum();
        let nf = n as f64;
        let z = |i: usize| self.z[i] as u8 as f64;
        let mean_w = pop as f64 / nf;
        let mean_z = (0..n).map(z).sum::<f64>() / nf;
        let cov = (0..n).map(|i| count[i] as f64 * z(i)).sum::<f64>() / nf - mean_w * mean_z;
        // w (z' - z) = carriers - w z, so a root with no representatives adds nothing.
        let trans = (0..n).map(|i| carriers[i] as f64 - count[i] as f64 * z(i)).sum::<f64>() / nf;
        let z1 = carriers.iter().sum::<u64>() as f64 / pop.max(1) as f64;
        let (selection, transmission) = if pop > 0 { (cov / mean_w, trans / mean_w) } else { (f64::NAN, f64::NAN) };
        let surviving = count.iter().filter(|&&c| c > 0).count();
        self.rows.push(format!(
            "{},{},{n},{surviving},{pop},{mean_z:.5},{z1:.5},{:.5},{selection:.5},{transmission:.5},{:.1e}",
            self.t0, w.tick, z1 - mean_z, z1 - mean_z - selection - transmission
        ));
    }

    fn print(&self, flux: &str) {
        println!("# ancestry: carriers of an expressed {flux} reaction; change = selection + transmission (Price equation over the roots alive at t0)");
        println!("# t0,t1,roots,roots_with_descendants,population_t1,carriers_t0,carriers_t1,change,selection,transmission,residual");
        for row in &self.rows {
            println!("# {row}");
        }
    }
}

/// Whether an entity has an expressed reaction gene, rate above zero, that makes `s`.
fn carries(w: &World, e: &emerge_ref::Entity, s: usize) -> bool {
    e.genome.slots.iter().enumerate().any(|(g, slot)| {
        e.genome.active(g) && matches!(slot.gene, Gene::React { reaction, rate, .. } if rate > 0 && w.chem.reactions[reaction].outputs.iter().any(|o| o.0 == s))
    })
}

/// Mutation alone, no selection: `lines` lineages start from the seed genome of `--lineage`
/// and each is mutated once per generation, as at a division. At chosen generations,
/// the share of lineages whose genome has a reaction making `--flux` (expressed, or only
/// dormant), an expressed `take` of each input it needs, and a complete route: an expressed
/// reaction making it whose every input is taken or made by another expressed reaction,
/// and (last column) an expressed `steal` of it with `max` above zero.
fn census(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else { return usage() };
    let (mut lineage, mut flux_name) = (String::new(), "body".to_string());
    let (mut lines, mut generations) = (10_000usize, 100u32);
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().map(String::as_str).unwrap_or("");
        match (flag.as_str(), value.parse::<u64>().ok()) {
            ("--lineage", _) => lineage = value.to_string(),
            ("--flux", _) => flux_name = value.to_string(),
            ("--lines", Some(n)) if n > 0 => lines = n as usize,
            ("--generations", Some(n)) if n > 0 => generations = n as u32,
            _ => return usage(),
        }
    }
    let Some(src) = read(path) else { return ExitCode::from(2) };
    let c = match compile(&src, &[lineage.as_str()], Options { seed: 1, allow_partial: true, mutation: true }) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let w = c.world;
    let Some(flux) = w.chem.substances.iter().position(|s| s.name == flux_name) else {
        eprintln!("--flux: unknown substance `{flux_name}`");
        return ExitCode::from(2);
    };
    let (Some(e), Some(Some((rates, cap)))) = (w.entities.first(), w.variation.first()) else {
        eprintln!("{path}: `{lineage}` needs entities at genesis and a `mutation` block");
        return ExitCode::FAILURE;
    };
    let mut genomes = vec![e.genome.clone(); lines];
    let report = |gen: u32, genomes: &[emerge_ref::Genome]| {
        let mut n = [0usize; 6];
        for g in genomes {
            let f = route(&w, g, flux);
            for (k, &b) in f.iter().enumerate() {
                n[k] += b as usize;
            }
            n[5] += g.slots.iter().enumerate().any(|(i, slot)| g.active(i) && matches!(slot.gene, Gene::Steal { s, max, .. } if s == flux && max > 0)) as usize;
        }
        let pct = |k: usize| n[k] as f64 / genomes.len() as f64 * 100.0;
        println!("{gen},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}", pct(0), pct(1), pct(2), pct(3), pct(4), pct(5));
    };
    println!("generation,any_{flux_name}_reaction_pct,expressed_{flux_name}_reaction_pct,takes_an_input_pct,expressed_and_takes_pct,complete_route_pct,steals_{flux_name}_pct");
    report(0, &genomes);
    let marks = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1_000, 2_000, 5_000];
    for gen in 1..=generations {
        for (i, g) in genomes.iter_mut().enumerate() {
            let stream = emerge_ref::philox::Stream {
                seed: 1,
                tick: gen,
                stream: i as u32,
                domain: emerge_ref::philox::DOMAIN_ENTITY,
                aux: 1,
                pass: emerge_ref::philox::MUTATE,
            };
            *g = emerge_ref::mutate::mutate(g, rates, cap, &w.menus, &mut emerge_ref::mutate::Draws::new(stream));
        }
        if marks.contains(&gen) || gen == generations {
            report(gen, &genomes);
        }
    }
    ExitCode::SUCCESS
}

/// For one genome and substance `s`: [some reaction gene makes `s`, an expressed one does,
/// some expressed `take` names an input of an expressed one, both of those, a complete route].
/// Expressed means active with rate above zero. A complete route: starting from the taken
/// substances, expressed reactions whose inputs are all available add their outputs, until
/// nothing changes; complete when a reaction making `s` can run that way.
fn route(w: &World, g: &emerge_ref::Genome, s: usize) -> [bool; 5] {
    let react = |active: bool| {
        g.slots.iter().enumerate().filter_map(move |(i, slot)| match slot.gene {
            Gene::React { reaction, rate, .. } if !active || (g.active(i) && rate > 0) => Some(reaction),
            _ => None,
        })
    };
    let taken: Vec<usize> = g.slots.iter().enumerate().filter_map(|(i, slot)| match slot.gene {
        Gene::Take { s, max, .. } if g.active(i) && max > 0 => Some(s),
        _ => None,
    }).collect();
    let makes = |r: usize| w.chem.reactions[r].outputs.iter().any(|o| o.0 == s);
    let any = react(false).any(makes);
    let expressed: Vec<usize> = react(true).filter(|&r| makes(r)).collect();
    let takes_input = expressed.iter().any(|&r| w.chem.reactions[r].inputs.iter().any(|i| taken.contains(&i.0)));
    let mut have = vec![false; w.chem.substances.len()];
    for &x in &taken {
        have[x] = true;
    }
    let runs: Vec<usize> = react(true).collect();
    let mut changed = true;
    while changed {
        changed = false;
        for &r in &runs {
            if w.chem.reactions[r].inputs.iter().all(|i| have[i.0]) {
                for o in &w.chem.reactions[r].outputs {
                    if !have[o.0] {
                        have[o.0] = true;
                        changed = true;
                    }
                }
            }
        }
    }
    let complete = expressed.iter().any(|&r| w.chem.reactions[r].inputs.iter().all(|i| have[i.0]));
    [any, !expressed.is_empty(), takes_input, !expressed.is_empty() && takes_input, complete]
}

/// Every entity and every cell with its nonzero amounts in µ, as `#` lines.
fn print_dump(w: &World) {
    let held = |s: &emerge_ref::Store| {
        (0..w.chem.substances.len()).filter(|&i| s.amounts[i] > 0).map(|i| format!(" {}={}", w.chem.substances[i].name, s.amounts[i])).collect::<String>()
    };
    println!("# dump at tick {}: {} entities, {} births, {} deaths (amounts in mu)", w.tick, w.entities.len(), w.births, w.deaths);
    for e in &w.entities {
        println!("#   entity {} cell {} pos ({:.3}, {:.3}) age {}:{}", e.id, e.cell, e.pos[0], e.pos[1], e.age, held(&e.store));
    }
    for (i, c) in w.cells.iter().enumerate() {
        println!("#   cell {i}:{}", held(c));
    }
}

/// Which limits cut transfers (by kind and substance) and division, as `#` lines.
fn print_binding(w: &World, b: &emerge_ref::world::Binding) {
    use emerge_ref::world::TRANSFER_KINDS;
    println!("# binding: transfers by kind and substance; share of requests cut by the origin's balance (stage 1) or the destination's room (stage 2); a request can be cut by both");
    for (&(kind, s), c) in &b.transfers {
        let pct = |x: u64| x as f64 / c.requests.max(1) as f64 * 100.0;
        let got = c.delivered as f64 / c.requested.max(1) as f64 * 100.0;
        let never = if c.by_origin == 0 && c.by_dest == 0 { "  [never cut]" } else { "" };
        println!(
            "#   {} {}: {} requests, cut by origin {:.1}%, by room {:.1}%, delivered {got:.1}% of µ asked{never}",
            TRANSFER_KINDS[kind], w.chem.substances[s].name, c.requests, pct(c.by_origin), pct(c.by_dest)
        );
    }
    let d = b.divide;
    let n = d.iter().sum::<u64>().max(1) as f64;
    println!(
        "# binding: divide intents {}: divided {:.1}%, fee unpaid {:.1}%, requires unmet {:.1}%, at max_entities {:.1}%",
        d.iter().sum::<u64>(), d[0] as f64 / n * 100.0, d[1] as f64 / n * 100.0, d[2] as f64 / n * 100.0, d[3] as f64 / n * 100.0
    );
}

/// For the 20 reactions with the most active gene-ticks over the run: the share that ran at
/// their cap, ran below it, had a cap of zero, or made nothing for lack of an input or room,
/// as `#` lines. Only active genes of living entities count.
fn print_blocked(w: &World, blocked: &[emerge_ref::world::Blocked]) {
    let total = |b: &emerge_ref::world::Blocked| b.full + b.limited + b.cap_zero + b.no_room + b.no_input.iter().sum::<u64>();
    let mut rows: Vec<(usize, u64)> = blocked.iter().enumerate().map(|(r, b)| (r, total(b))).filter(|&(_, n)| n > 0).collect();
    rows.sort_by_key(|&(r, n)| (std::cmp::Reverse(n), r));
    println!(
        "# blocked: reaction genes by outcome, share of active gene-ticks over the run ({} of {} reactions shown)",
        rows.len().min(20),
        rows.len()
    );
    for &(r, n) in rows.iter().take(20) {
        let (b, rx) = (&blocked[r], &w.chem.reactions[r]);
        let pct = |x: u64| x as f64 / n as f64 * 100.0;
        let mut why = format!("full {:.1}%, limited {:.1}%, cap 0 {:.1}%", pct(b.full), pct(b.limited), pct(b.cap_zero));
        for (s, &c) in b.no_input.iter().enumerate().filter(|&(_, &c)| c > 0) {
            why += &format!(", no {} {:.1}%", w.chem.substances[s].name, pct(c));
        }
        if b.no_room > 0 {
            why += &format!(", no room {:.1}%", pct(b.no_room));
        }
        println!("#   {} ({n} gene-ticks): {why}", w.chem.formula(&rx.inputs, &rx.outputs));
    }
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

/// Whether an entity has an active `guard` gene.
fn guards(e: &emerge_ref::Entity) -> bool {
    e.genome.slots.iter().enumerate().any(|(g, slot)| e.genome.active(g) && matches!(slot.gene, Gene::Guard { .. }))
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
                Sense::Delta(s) => format!("sense delta({})", sub(*s)),
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
            Gene::Guard { s, .. } => format!("action guard({})", sub(*s)),
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
