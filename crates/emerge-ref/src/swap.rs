//! Hot-swapping (spec §13, `proposals/engine-api.md`): a running world keeps its source and a
//! revision, and a patch changes the world between ticks, or is rejected and changes nothing.
//!
//! What may change is a fixed list (`Config`). The check is fail-closed: the current source and
//! the patched one are compiled afresh, the listed parts are blanked in both, and everything else
//! must be identical, so a part nobody listed is rejected rather than half-applied.

use crate::compile::{compile, Compiled, Options};
use crate::mutate::Rates;
use crate::world::{Law, Sink};
use crate::{Mu, World};

/// A world that runs from a source and can be changed by patches.
pub struct Live {
    pub world: World,
    /// The canonical (`emerge fmt`) source of the current revision.
    pub source: String,
    pub lineages: Vec<String>,
    pub options: Options,
    /// 0 at the start, one more for every committed swap.
    pub revision: u64,
}

/// Why a swap was rejected. A rejected swap changes nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Rejected {
    /// The patch was written against another revision.
    Stale { base: u64, current: u64 },
    /// The patch, or the world it gives, does not parse, check or compile.
    Invalid { code: String, path: String, message: String },
    /// The patch changes a part that cannot be swapped (yet).
    NotSwappable(String),
    /// The patch does not fit the current balances.
    Preflight(String),
}

impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Rejected::Stale { base, current } => write!(f, "stale: written against revision {base}, the world is at {current}"),
            Rejected::Invalid { code, path, message } => write!(f, "invalid: {code} {path}: {message}"),
            Rejected::NotSwappable(what) => write!(f, "not swappable: {what}"),
            Rejected::Preflight(why) => write!(f, "preflight: {why}"),
        }
    }
}

/// What a swap may change.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub laws: Vec<Law>,
    /// `upkeep`'s `base`, `per_gene`, `per_signal`, `per_signal_keep` and `per_carried_u`.
    pub upkeep: [Mu; 5],
    pub max_age: u64,
    pub max_entities: usize,
    pub mind_steps: u32,
    /// Each lineage's mutation rates (the capacity may not change).
    pub rates: Vec<Option<Rates>>,
    /// The caps of every cell, and of every inventory.
    pub cell_caps: Vec<[Mu; crate::world::MAX_SUBSTANCES]>,
    pub inventory_cap: Mu,
    pub sources: Vec<String>,
    pub sinks: Vec<Sink>,
}

impl Config {
    pub fn of(w: &World) -> Config {
        let u = &w.upkeep;
        Config {
            laws: w.laws.clone(),
            upkeep: [u.base, u.per_gene, u.per_signal, u.per_signal_keep, u.per_carried_u],
            max_age: w.max_age,
            max_entities: w.max_entities,
            mind_steps: w.mind_steps,
            rates: w.variation.iter().map(|v| v.as_ref().map(|(r, _)| r.clone())).collect(),
            cell_caps: w.cells.iter().map(|c| c.caps).collect(),
            inventory_cap: w.inventory_cap,
            sources: w.sources.clone(),
            sinks: w.sinks.clone(),
        }
    }
}

/// A patch that compiled and passed the diff, ready to commit at a tick boundary.
pub struct Staged {
    pub base: u64,
    pub source: String,
    pub config: Config,
    /// The patched world compiled afresh: its configuration is `config` (a GPU packs it).
    pub world: World,
    /// Time to patch, compile and diff, in seconds (spec §13.2 reports it apart from the pause).
    pub compile_s: f64,
}

/// Blanks the parts a swap may change, so that what is left can be compared. Sources and sinks
/// are cut to `keep` (new ones may be added after the old ones).
fn blank(w: &mut World, keep: (usize, usize)) {
    w.laws.clear();
    let u = &mut w.upkeep;
    (u.base, u.per_gene, u.per_signal, u.per_signal_keep, u.per_carried_u) = (0, 0, 0, 0, 0);
    (w.max_age, w.max_entities, w.mind_steps, w.inventory_cap) = (0, 0, 0, 0);
    for v in w.variation.iter_mut().flatten() {
        v.0 = Rates::default();
    }
    for c in &mut w.cells {
        c.caps.iter_mut().for_each(|x| *x = 0);
    }
    for e in &mut w.entities {
        e.store.caps.iter_mut().for_each(|x| *x = 0);
    }
    w.sources.truncate(keep.0);
    w.sinks.truncate(keep.1);
}

fn invalid(code: &str, path: &str, message: &str) -> Rejected {
    Rejected::Invalid { code: code.into(), path: path.into(), message: message.into() }
}

impl Live {
    /// Compiles `source` (put in canonical form first) and starts at revision 0.
    pub fn new(source: &str, lineages: &[&str], options: Options) -> Result<Live, Rejected> {
        let source = canonical(source)?;
        let world = compile(&source, lineages, options).map_err(|e| invalid(e.code, &e.path, &e.message))?.world;
        Ok(Live { world, source, lineages: lineages.iter().map(|s| s.to_string()).collect(), options, revision: 0 })
    }

    fn compile(&self, source: &str) -> Result<Compiled, Rejected> {
        let names: Vec<&str> = self.lineages.iter().map(String::as_str).collect();
        compile(source, &names, self.options).map_err(|e| invalid(e.code, &e.path, &e.message))
    }

    /// Steps 1 to 3 of spec §13.2: the revision, the patched source compiled, and the diff. Does
    /// not touch the running world.
    pub fn stage(&self, patch: &str, base: u64) -> Result<Staged, Rejected> {
        let t = std::time::Instant::now();
        if base != self.revision {
            return Err(Rejected::Stale { base, current: self.revision });
        }
        let file = emerge_lang::parse(&self.source).map_err(|d| invalid(d.code, "", &d.message))?;
        let patches = emerge_lang::parse(patch).map_err(|d| invalid(d.code, "", &format!("patch {}:{}: {}", d.line, d.col, d.message)))?;
        // A `base` written in the patch must agree with the one it is sent with.
        for item in &patches.items {
            if let emerge_lang::ast::Item::Patch(p) = &item.node {
                if let Some(b) = p.base.filter(|&b| b != base as i64) {
                    return Err(Rejected::Stale { base: b.max(0) as u64, current: self.revision });
                }
            }
        }
        let patched = emerge_lang::patch::apply(&file, &patches).map_err(|p| invalid(p.code, &p.path, &p.message))?;
        let source = emerge_lang::format(&patched);
        let mut old = self.compile(&self.source)?.world;
        let mut new = self.compile(&source)?.world;
        let config = Config::of(&new);
        let candidate = new.clone();
        let keep = (old.sources.len(), old.sinks.len());
        if new.sources.len() < keep.0 || new.sinks.len() < keep.1 {
            return Err(Rejected::NotSwappable("removing a source or sink".into()));
        }
        if self.world.variation.iter().zip(&new.variation).any(|(a, b)| a.is_some() != b.is_some()) {
            return Err(Rejected::NotSwappable("turning a lineage's mutation on or off".into()));
        }
        blank(&mut old, keep);
        blank(&mut new, keep);
        let (a, b) = (format!("{old:?}"), format!("{new:?}"));
        if a != b {
            let part = parts(&old).into_iter().zip(parts(&new)).find(|(x, y)| x.1 != y.1).map_or("the world", |(x, _)| x.0);
            return Err(Rejected::NotSwappable(format!("`{part}` changed; {}", first_difference(&a, &b))));
        }
        Ok(Staged { base, source, config, world: candidate, compile_s: t.elapsed().as_secs_f64() })
    }

    /// Steps 4 and 5, between ticks: the revision again, the patch against the current balances,
    /// then the commit. Returns the pause in seconds.
    pub fn commit(&mut self, staged: Staged) -> Result<f64, Rejected> {
        let t = std::time::Instant::now();
        if staged.base != self.revision {
            return Err(Rejected::Stale { base: staged.base, current: self.revision });
        }
        let w = &self.world;
        preflight(&staged.config, w, w.cells.iter().map(|c| &c.amounts[..]), w.entities.iter().map(|e| (e.id, &e.store.amounts[..])))?;
        apply(&mut self.world, &staged.config);
        self.advance(staged);
        Ok(t.elapsed().as_secs_f64())
    }

    /// Takes the staged source and revision without touching `world`: for a backend that holds
    /// the state elsewhere (the GPU), after it has preflighted and applied the swap itself.
    pub fn advance(&mut self, staged: Staged) {
        assert_eq!(staged.base, self.revision, "a staged swap is committed against the revision it was staged at");
        self.source = staged.source;
        self.revision += 1;
    }

    /// `stage` and `commit` at once, at the current tick boundary.
    pub fn hotswap(&mut self, patch: &str, base: u64) -> Result<(), Rejected> {
        let staged = self.stage(patch, base)?;
        self.commit(staged).map(|_| ())
    }
}

/// Step 4 of spec §13.2: the new caps against the current balances (amounts by substance, for
/// every cell and for every entity with its ID). `names` gives the substances.
pub fn preflight<'a>(c: &Config, names: &World, cells: impl Iterator<Item = &'a [Mu]>, entities: impl Iterator<Item = (u64, &'a [Mu])>) -> Result<(), Rejected> {
    let ns = names.chem.substances.len();
    for (i, (amounts, caps)) in cells.zip(&c.cell_caps).enumerate() {
        if let Some(s) = (0..ns).find(|&s| amounts[s] > caps[s]) {
            return Err(Rejected::Preflight(format!("cell {i} holds {}µ of {}, above the new cap {}µ (a `move_to` destination is not supported yet)", amounts[s], names.chem.substances[s].name, caps[s])));
        }
    }
    for (id, amounts) in entities {
        if amounts[..ns].iter().any(|&a| a > c.inventory_cap) {
            return Err(Rejected::Preflight(format!("entity {id} holds more than the new inventory cap {}µ", c.inventory_cap)));
        }
    }
    Ok(())
}

/// Copies a configuration into a running world.
pub fn apply(w: &mut World, c: &Config) {
    w.laws = c.laws.clone();
    let u = &mut w.upkeep;
    [u.base, u.per_gene, u.per_signal, u.per_signal_keep, u.per_carried_u] = c.upkeep;
    (w.max_age, w.max_entities, w.mind_steps) = (c.max_age, c.max_entities, c.mind_steps);
    for (v, r) in w.variation.iter_mut().zip(&c.rates) {
        if let (Some(v), Some(r)) = (v, r) {
            v.0 = r.clone();
        }
    }
    for (cell, caps) in w.cells.iter_mut().zip(&c.cell_caps) {
        cell.caps = *caps;
    }
    w.inventory_cap = c.inventory_cap;
    let ns = w.chem.substances.len();
    for e in &mut w.entities {
        e.store.caps[..ns].iter_mut().for_each(|x| *x = c.inventory_cap);
    }
    w.sources = c.sources.clone();
    w.sinks = c.sinks.clone();
}

/// The source in `emerge fmt` form.
pub fn canonical(source: &str) -> Result<String, Rejected> {
    let file = emerge_lang::parse(source).map_err(|d| invalid(d.code, "", &format!("{}:{}: {}", d.line, d.col, d.message)))?;
    Ok(emerge_lang::format(&file))
}

/// The world's parts by name, to say which one a rejected patch changed.
fn parts(w: &World) -> Vec<(&'static str, String)> {
    vec![
        ("chemistry and reactions", format!("{:?}", w.chem)),
        ("space", format!("{:?}", w.space)),
        ("cells", format!("{:?}{:?}", w.cells, w.neighbours)),
        ("menus (senses, actions, regulation, fees)", format!("{:?}", w.menus)),
        ("lineages and their genomes", format!("{:?}{:?}", w.entities, w.variation)),
        ("upkeep routes", format!("{:?}", w.upkeep)),
        ("waste, remains, decompose", format!("{:?}{:?}{:?}", w.waste, w.remains, w.decompose)),
        ("sources and sinks", format!("{:?}{:?}", w.sources, w.sinks)),
    ]
}

/// Where two descriptions of a world first differ, as a short excerpt naming the field.
fn first_difference(a: &str, b: &str) -> String {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let at = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
    // Back to the start of the field before the one that differs, so its name shows.
    let mut start = at;
    for _ in 0..2 {
        start = a[..start].iter().rposition(|&c| c == ',' || c == '{').unwrap_or(0);
    }
    let cut = |s: &[char]| s[start.min(s.len())..(at + 60).min(s.len())].iter().collect::<String>().trim_start_matches([',', '{']).trim().to_string();
    format!("near `{}`, which became `{}`", cut(&a), cut(&b))
}
