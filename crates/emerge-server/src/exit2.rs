//! Exit test 2 (spec §16, `proposals/engine-api.md` H4): zero element drift over a long run that
//! has 20 valid and 20 rejected hot-swaps, on the CPU runner and on the GPU. The swaps come from a
//! seeded generator fixed before the run; valid ones move one value of `examples/dish.em` to within
//! ±20% of the dish's own value. The run must stay alive with births in every window of 1_000 ticks,
//! so that zero drift is not the zero drift of an empty world.

use crate::journal::Journal;
use crate::run::{Run, Spec};
use emerge_ref::swap::Rejected;

/// The values a valid swap may move: patch target, path, the dish's value, and its unit.
const VALID: [(&str, &str, f64, &str); 8] = [
    ("Dish", "rules.sunlight@transfer_light.amount", 40.0, "mu"),
    ("Dish", "rules.weather@transfer_carbon.amount", 20.0, "mu"),
    ("Dish", "rules.weather@transfer_nitrate.amount", 10.0, "mu"),
    ("Dish", "rules.rotting@react_rot.rate", 50.0, "bp"),
    ("Dish", "rules.spread@diffuse_conserved.rate", 300.0, "bp"),
    ("Dish", "upkeep.base", 20.0, "mu"),
    ("Dish", "upkeep.per_gene", 5.0, "mu"),
    ("Drifter", "mutation.rate", 0.02, ""),
];

/// The kinds of rejected swap, in turn, with the patch each sends.
const REJECTED: [(&str, &str); 5] = [
    ("stale", "patch Dish {\n    set upkeep.base = 22mu;\n}\n"),
    ("invalid", "patch Dish {\n    set rules.rotting@react_rot.rate = 20_000bp;\n}\n"),
    ("not swappable: space", "patch Dish {\n    set space.max_interaction_radius = 3.0;\n}\n"),
    ("not swappable: a menu range", "patch Dish {\n    set actions.steal.max = 0u..2u;\n}\n"),
    ("preflight: a lower inventory cap", "patch Dish {\n    set limits.inventory_cap = 6u;\n}\n"),
];

pub struct Planned {
    pub tick: u64,
    pub patch: String,
    /// `None` for a valid swap; else the kind of rejection expected.
    pub reject: Option<&'static str>,
}

/// 40 swaps, one every `ticks / 40` ticks from the first such tick, valid and rejected in turn.
pub fn plan(ticks: u64, seed: u64) -> Vec<Planned> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let every = ticks / 40;
    (0..40)
        .map(|k| {
            let tick = every * (k + 1) - every / 2;
            if k % 2 == 0 {
                let (target, path, value, unit) = VALID[(next() % VALID.len() as u64) as usize];
                let u = (next() % 4001) as f64 / 10_000.0 - 0.2;
                let v = value * (1.0 + u);
                let text = if unit.is_empty() { format!("{v:.4}") } else { format!("{}{unit}", v.round().max(1.0) as u64) };
                Planned { tick, patch: format!("patch {target} {{\n    set {path} = {text};\n}}\n"), reject: None }
            } else {
                let (kind, patch) = REJECTED[(k as usize / 2) % REJECTED.len()];
                Planned { tick, patch: patch.into(), reject: Some(kind) }
            }
        })
        .collect()
}

fn expected(kind: &str, r: &Rejected) -> bool {
    match r {
        Rejected::Stale { .. } => kind == "stale",
        Rejected::Invalid { .. } => kind == "invalid",
        Rejected::NotSwappable(_) => kind.starts_with("not swappable"),
        Rejected::Preflight(_) => kind.starts_with("preflight"),
    }
}

/// Runs exit test 2 and prints its record; returns whether it passed.
pub fn run(source: &str, gpu: bool, ticks: u64, journal: Journal) -> bool {
    let spec = Spec { source: source.into(), lineages: vec!["Drifter".into()], seed: 1, mutation: true, contract: gpu, gpu };
    let mut run = Run::create(spec, journal).expect("the dish compiles");
    run.enable_audit();
    println!("exit test 2 on {}: {ticks} ticks, 40 swaps", run.device);
    let (mut committed, mut rejected, mut wrong, mut windows_without_births) = (0, 0, Vec::new(), 0);
    let mut births = run.births();
    let started = std::time::Instant::now();
    for p in plan(ticks, 2026) {
        while run.tick() < p.tick && run.population() > 0 {
            run.step(1);
            if run.tick().is_multiple_of(1_000) {
                windows_without_births += u64::from(run.births() == births);
                births = run.births();
            }
        }
        let base = if p.reject == Some("stale") { run.revision() + 1 } else { run.revision() };
        let before = p.reject.map(|_| run.digest());
        match (p.reject, run.hotswap(&p.patch, base, "exit test 2")) {
            (None, Ok(r)) => {
                committed += 1;
                println!("tick {}: committed revision {} ({:.1} ms pause): {}", p.tick, r["revision"], r["pause_ms"].as_f64().unwrap_or(0.0), p.patch.lines().nth(1).unwrap_or("").trim());
            }
            (Some(kind), Err(e)) if expected(kind, &e) && before == Some(run.digest()) => {
                rejected += 1;
                println!("tick {}: rejected as expected ({kind}): {e}", p.tick);
            }
            (want, got) => wrong.push(format!("tick {}: expected {:?}, got {got:?}", p.tick, want.unwrap_or("a commit"))),
        }
    }
    while run.tick() < ticks && run.population() > 0 {
        run.step(1);
        if run.tick().is_multiple_of(1_000) {
            windows_without_births += u64::from(run.births() == births);
            births = run.births();
        }
    }
    run.close();
    let (audited, off) = run.audit().unwrap_or((0, 0));
    let alive = run.population() > 0 && run.tick() == ticks;
    let pass = committed == 20 && rejected == 20 && wrong.is_empty() && audited == ticks && off == 0 && alive && windows_without_births == 0;
    for w in &wrong {
        println!("WRONG {w}");
    }
    println!(
        "result: {} — {committed} committed, {rejected} rejected as expected, {audited} ticks audited, {off} with element totals off, population {} at tick {}, {windows_without_births} windows of 1_000 ticks without births ({:.0} s)",
        if pass { "PASS" } else { "FAIL" },
        run.population(),
        run.tick(),
        started.elapsed().as_secs_f64()
    );
    pass
}
