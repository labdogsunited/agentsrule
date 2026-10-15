//! Compiles a parsed `.em` file into a runnable `World` (spec §15, `language.md`).
//!
//! Covers what the runner can run: the chemistry, one conserved grid field over
//! `Continuous2D`, sources and sinks, waste, upkeep, limits, transfer, reaction and
//! diffusion rules, world genesis, and lineages whose seed genomes use the senses, genes
//! and actions the runner implements.
//!
//! Every name must resolve, every gene parameter must be on the world's menu and within
//! its declared range, sources emit only what they declare, sinks accept what is sent to
//! them, and endowments fit their caps. A mistake is always an error, never a skipped item.
//!
//! What the runner cannot run is handled in two ways:
//! - declarations that do not affect a run without mutation (the generated reaction space,
//!   profiles, mutation settings, rulesets, views, charts, explore blocks, patches) are
//!   listed in `Compiled::notes`;
//! - declarations that would change what the world does (signal fields and their rules)
//!   are an error, unless the caller allows a partial run; then they are listed in
//!   `Compiled::partial`. A genome that needs a missing feature is always an error.

use std::collections::HashMap;

use std::cell::Cell;

use emerge_lang::ast::{self, BinOp, Expr, GeneBody, LDecl, Lit, MenuItem, Pos, Seg, Stmt, Value, WDecl};

use crate::chem::{Chemistry, Mu, ReactionId, SubstanceId};
use crate::mind::{Curve, Gene, Genome, Input, Sense, Slot, ONE};
use crate::philox::{self, Stream};
use crate::world::{End, Law, Sink, SinkId, SourceId, Space, Upkeep, World};

/// A diagnostic (`language.md` §14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub code: &'static str,
    /// Where the innermost item being compiled starts; line 0 if unknown.
    pub pos: Pos,
    /// The item's path, such as `Dish.upkeep` or `Drifter.seed.forage`.
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.pos.line > 0 {
            write!(f, "{}:{}: ", self.pos.line, self.pos.col)?;
        }
        write!(f, "error[{}]", self.code)?;
        if !self.path.is_empty() {
            write!(f, ": {}", self.path)?;
        }
        write!(f, ": {}", self.message)
    }
}

pub struct Compiled {
    pub world: World,
    /// Declared, but unused in a run without mutation.
    pub notes: Vec<String>,
    /// Declared and part of the world's behaviour, but not run (only with `allow_partial`),
    /// as warnings with code `W1001`.
    pub partial: Vec<Error>,
}

/// Parses and checks `src`, then builds its world seeded with `lineages`, in that order.
/// With `allow_partial`, parts of the world the runner cannot run are left out and listed
/// instead of being an error.
pub fn compile(src: &str, lineages: &[&str], seed: u64, allow_partial: bool) -> Result<Compiled, Error> {
    let file = emerge_lang::parse(src).map_err(|d| Error { code: d.code, pos: Pos { line: d.line, col: d.col }, path: String::new(), message: d.message })?;
    if let Some(p) = emerge_lang::check::check(&file).into_iter().next() {
        return Err(Error { code: p.code, pos: p.pos, path: p.path, message: p.message });
    }
    build(&file, lineages, seed, allow_partial)
}

/// The names of the lineages declared in a file, in order.
pub fn lineage_names(file: &ast::File) -> Vec<&str> {
    file.items.iter().filter_map(|i| match &i.node {
        ast::Item::Lineage(l) => Some(l.name.as_str()),
        _ => None,
    }).collect()
}

/// Builds the world of a parsed file whose chemistry `emerge_lang::check` has accepted.
pub fn build(file: &ast::File, lineages: &[&str], seed: u64, allow_partial: bool) -> Result<Compiled, Error> {
    let at = Cell::new(Pos::default());
    build_at(file, lineages, seed, allow_partial, &at).map_err(|mut e| {
        if e.pos.line == 0 {
            e.pos = at.get();
        }
        e
    })
}

fn build_at(file: &ast::File, lineages: &[&str], seed: u64, allow_partial: bool, at: &Cell<Pos>) -> Result<Compiled, Error> {
    let worlds: Vec<&ast::Node<ast::Item>> = file.items.iter().filter(|i| matches!(i.node, ast::Item::World(_))).collect();
    let [item] = worlds[..] else {
        return err("E1001", "", format!("emerge-ref runs files with one world; this one has {}", worlds.len()));
    };
    let ast::Item::World(w) = &item.node else { unreachable!() };
    at.set(item.pos);
    let mut c = Compiler::new(w, at)?;
    c.world.seed = seed;
    for item in &file.items {
        match &item.node {
            ast::Item::World(_) | ast::Item::Lineage(_) => {}
            ast::Item::Ruleset(r) => c.notes.push(format!("ruleset {}", r.name)),
            ast::Item::View(v) => c.notes.push(format!("view {}", v.name)),
            ast::Item::Chart(ch) => c.notes.push(format!("chart {}", ch.name)),
            ast::Item::Explore(e) => c.notes.push(format!("explore {}", e.name)),
            ast::Item::Patch(p) => c.notes.push(format!("patch {} base {}", p.target, p.base)),
        }
    }
    for name in lineages {
        let Some((pos, l)) = file.items.iter().find_map(|i| match &i.node {
            ast::Item::Lineage(l) if l.name == *name => Some((i.pos, l)),
            _ => None,
        }) else {
            return err("E0114", *name, "no such lineage");
        };
        at.set(pos);
        if l.world != w.name {
            return err("E0114", &l.name, format!("lineage is for `{}`, not `{}`", l.world, w.name));
        }
        c.lineage(l)?;
    }
    if !allow_partial && !c.partial.is_empty() {
        at.set(item.pos);
        return err(
            "E1002",
            &w.name,
            format!(
                "emerge-ref cannot run all of this world: {}. Allow a partial run (`--partial`) to leave them out",
                c.partial.iter().map(|e| e.message.as_str()).collect::<Vec<_>>().join("; ")
            ),
        );
    }
    Ok(Compiled { world: c.world, notes: c.notes, partial: c.partial })
}

struct ActionDef {
    params: Vec<(String, ast::Param)>,
    cost: Mu,
    per_distance: bool,
    requires: Option<(SubstanceId, Mu)>,
}

struct GeneDef {
    params: Vec<(String, ast::Param)>,
    inputs: (i64, i64),
    weight: Option<(f32, f32)>,
}

struct Compiler<'a> {
    w: &'a ast::World,
    /// The innermost node being compiled, for errors' positions.
    at: &'a Cell<Pos>,
    /// Where the world starts, for errors about what it lacks.
    world_pos: Pos,
    world: World,
    notes: Vec<String>,
    partial: Vec<Error>,
    substances: HashMap<String, SubstanceId>,
    reactions: HashMap<String, ReactionId>,
    sources: HashMap<String, SourceId>,
    /// What each source may emit.
    emits: HashMap<String, Vec<SubstanceId>>,
    sinks: HashMap<String, SinkId>,
    /// The conserved grid field, if any.
    grid: Option<String>,
    /// Signal fields: declared, not run.
    signal_fields: Vec<String>,
    senses: HashMap<String, Vec<(String, ast::Param)>>,
    actions: HashMap<String, ActionDef>,
    regulation: HashMap<String, GeneDef>,
    /// The reaction space's rate limit for gene reactions.
    max_rate: Option<Mu>,
    included: Option<Vec<ReactionId>>,
}

fn err<T>(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Result<T, Error> {
    Err(Error { code, pos: Pos::default(), path: path.into(), message: message.into() })
}

impl<'a> Compiler<'a> {
    fn new(w: &'a ast::World, at: &'a Cell<Pos>) -> Result<Self, Error> {
        let p = |what: &str| format!("{}.{what}", w.name);
        let world_pos = at.get();

        // Chemistry, in declaration order. `check` has already validated it.
        let elements: Vec<&str> = w.decls.iter().filter_map(|d| match &d.node {
            WDecl::Element(e) => Some(e.as_str()),
            _ => None,
        }).collect();
        let mut chem = Chemistry::new(&elements);
        let mut substances = HashMap::new();
        let mut reactions = HashMap::new();
        for d in &w.decls {
            at.set(d.pos);
            match &d.node {
                WDecl::Config { name, entries } if name == "second_law" => {
                    for e in entries {
                        at.set(e.pos);
                        match (e.node.key.as_str(), &e.node.value) {
                            ("min_drop", Value::Expr(Expr::Lit(Lit::Int(m)))) => chem.min_drop = *m,
                            (k, _) => return err("E0115", p("second_law"), format!("unknown setting `{k}`")),
                        }
                    }
                }
                WDecl::Substance { name, comp, potential } => {
                    let terms: Vec<(&str, u32)> = comp.iter().map(|t| (t.name.as_str(), t.coeff.unwrap_or(1) as u32)).collect();
                    substances.insert(name.clone(), chem.substance(name, &terms, *potential));
                }
                WDecl::Reaction { name, formula, catalyze } => {
                    if catalyze.is_some() {
                        return err("E1001", p(&format!("reactions.{name}")), "catalysts are not supported by emerge-ref yet");
                    }
                    let side = |terms: &[ast::Term]| -> Vec<(SubstanceId, u32)> {
                        terms.iter().map(|t| (substances[&t.name], t.coeff.unwrap_or(1) as u32)).collect()
                    };
                    let (i, o) = (side(&formula.inputs), side(&formula.outputs));
                    let r = chem.reaction(name, &i, &o).map_err(|e| Error { code: "E0401", pos: Pos::default(), path: p(&format!("reactions.{name}")), message: format!("{e:?}") })?;
                    reactions.insert(name.clone(), r);
                }
                _ => {}
            }
        }

        // Sinks first: the world is built around its payment route and remains.
        let mut sinks = Vec::new();
        let mut sink_ids = HashMap::new();
        for d in &w.decls {
            at.set(d.pos);
            if let WDecl::Sink { name, accepts } = &d.node {
                let accepts = match accepts {
                    ast::SinkAccepts::Any => None,
                    ast::SinkAccepts::List(names) => Some(
                        names.iter().map(|n| lookup(&substances, n, &p("sinks"), "substance")).collect::<Result<_, _>>()?,
                    ),
                };
                sink_ids.insert(name.clone(), sinks.len());
                sinks.push(Sink { name: name.clone(), accepts });
            }
        }
        let setting = |key: &str| w.decls.iter().find_map(|d| match &d.node {
            WDecl::Setting(e) if e.key == key => Some(&e.value),
            _ => None,
        });
        let remains = match setting("remains") {
            Some(v) => lookup(&sink_ids, name_of(expr(v)?, &p("remains"))?, &p("remains"), "sink")?,
            None => {
                at.set(world_pos);
                return err("E0116", p("remains"), "a world must declare `remains`");
            }
        };
        let config = |block: &str| w.decls.iter().find_map(|d| match &d.node {
            WDecl::Config { name, entries } if name == block => Some(entries),
            _ => None,
        });
        let Some(up) = config("upkeep") else {
            at.set(world_pos);
            return err("E0116", p("upkeep"), "a world must declare `upkeep`");
        };
        let mut upkeep = Upkeep { pay: 0, becomes: 0, vent: 0, base: 0, per_gene: 0, per_signal: 0, per_signal_keep: 0, per_carried_u: 0 };
        let (mut pay, mut becomes, mut vent) = (None, None, None);
        for e in up {
            at.set(e.pos);
            let path = p("upkeep");
            let v = expr(&e.node.value)?;
            match e.node.key.as_str() {
                "pay" => pay = Some(lookup(&substances, name_of(v, &path)?, &path, "substance")?),
                "becomes" => becomes = Some(lookup(&substances, name_of(v, &path)?, &path, "substance")?),
                "vent" => vent = Some(lookup(&sink_ids, name_of(v, &path)?, &path, "sink")?),
                "base" => upkeep.base = amount(v, &path)?,
                "per_gene" => upkeep.per_gene = amount(v, &path)?,
                "per_signal" => upkeep.per_signal = amount(v, &path)?,
                "per_signal_keep" => upkeep.per_signal_keep = amount(v, &path)?,
                "per_carried_u" => upkeep.per_carried_u = amount(v, &path)?,
                k => return err("E0115", path, format!("unknown setting `{k}`")),
            }
        }
        match (pay, becomes, vent) {
            (Some(a), Some(b), Some(c)) => (upkeep.pay, upkeep.becomes, upkeep.vent) = (a, b, c),
            _ => return err("E0116", p("upkeep"), "upkeep needs `pay`, `becomes` and `vent`"),
        }
        if let Some(s) = (0..chem.substances.len()).find(|&s| !sinks[remains].accepts(s)) {
            return err("E0704", p("remains"), format!("`{}` must accept every substance; it does not accept `{}`", sinks[remains].name, chem.substances[s].name));
        }
        let world = World::new(chem, upkeep, sinks, remains).map_err(|e| Error { code: "E0701", pos: Pos::default(), path: p("upkeep"), message: format!("{e:?}") })?;

        let mut c = Compiler {
            w,
            at,
            world_pos,
            world,
            notes: Vec::new(),
            partial: Vec::new(),
            substances,
            reactions,
            sources: HashMap::new(),
            emits: HashMap::new(),
            sinks: sink_ids,
            grid: None,
            signal_fields: Vec::new(),
            senses: HashMap::new(),
            actions: HashMap::new(),
            regulation: HashMap::new(),
            max_rate: None,
            included: None,
        };
        c.declarations()?;
        Ok(c)
    }

    fn left_out(&mut self, path: String, message: String) {
        self.partial.push(Error { code: "W1001", pos: self.at.get(), path, message });
    }

    fn path(&self, what: &str) -> String {
        format!("{}.{what}", self.w.name)
    }

    fn substance(&self, n: &str, path: &str) -> Result<SubstanceId, Error> {
        lookup(&self.substances, n, path, "substance")
    }

    /// Everything after the chemistry and the payment route, in declaration order.
    fn declarations(&mut self) -> Result<(), Error> {
        let w = self.w;
        let mut space = None;
        let mut child_offset = 0.5;
        for d in &w.decls {
            self.at.set(d.pos);
            match &d.node {
                WDecl::Setting(e) => {
                    let path = self.path(&e.key);
                    let v = expr(&e.value)?;
                    match e.key.as_str() {
                        "space" => space = Some(call(v, &path)?),
                        "remains" => {}
                        "max_age" => self.world.max_age = int(v, &path)? as u64,
                        "child_offset" => child_offset = float(v, &path)?,
                        "mind_steps" => self.world.mind_steps = int(v, &path)? as u32,
                        k => return err("E0115", path, format!("unknown setting `{k}`")),
                    }
                }
                WDecl::Config { name, entries } if name == "limits" => {
                    for e in entries {
                        self.at.set(e.pos);
                        let path = self.path("limits");
                        let v = expr(&e.node.value)?;
                        match e.node.key.as_str() {
                            "max_entities" => self.world.max_entities = int(v, &path)? as usize,
                            "inventory_cap" => self.world.inventory_cap = amount(v, &path)?,
                            "max_reaction_space" => {}
                            k => return err("E0115", path, format!("unknown setting `{k}`")),
                        }
                    }
                }
                WDecl::Config { name, entries } if name == "reaction_space" => self.reaction_space(entries)?,
                WDecl::Config { name, .. } if name != "second_law" && name != "upkeep" => {
                    return err("E0115", self.path(name), format!("unknown block `{name}`"));
                }
                WDecl::Source { name, emits } => {
                    let path = self.path(&format!("sources.{name}"));
                    let ids = emits.iter().map(|s| self.substance(s, &path)).collect::<Result<_, _>>()?;
                    let id = self.world.add_source(name);
                    self.sources.insert(name.clone(), id);
                    self.emits.insert(name.clone(), ids);
                }
                WDecl::Waste { substance, sink } => {
                    let path = self.path("waste");
                    let s = self.substance(substance, &path)?;
                    let k = self.accepting_sink(sink, s, &path)?;
                    self.world.waste.push((s, k));
                }
                WDecl::Menu { kind, items } => self.menu(kind, items)?,
                _ => {}
            }
        }

        // Fields: one conserved grid over the continuous space.
        let Some(space) = space else {
            self.at.set(self.world_pos);
            return err("E0116", self.path("space"), "a world must declare `space`");
        };
        if space.name != "Continuous2D" {
            return err("E1001", self.path("space"), format!("`{}` is not supported by emerge-ref yet", space.name));
        }
        let sp = self.path("space");
        let (sw, sh) = (float(arg(&space, "w", &sp)?, &sp)?, float(arg(&space, "h", &sp)?, &sp)?);
        let wrap = boolean(arg(&space, "wrap", &sp)?, &sp)?;
        for d in &w.decls {
            self.at.set(d.pos);
            let WDecl::Field { name, kind, holds } = &d.node else { continue };
            let path = self.path(&format!("fields.{name}"));
            let Some(holds) = holds else {
                self.left_out(path.clone(), format!("signal field `{name}` is not run"));
                self.signal_fields.push(name.clone());
                continue;
            };
            if self.grid.is_some() {
                return err("E1001", path, "emerge-ref supports one conserved field");
            }
            if kind.name != "Grid" {
                return err("E1001", path, format!("`{}` is not supported by emerge-ref yet", kind.name));
            }
            let (gw, gh) = (int(arg(kind, "w", &path)?, &path)?, int(arg(kind, "h", &path)?, &path)?);
            let mut caps = vec![0; self.world.chem.substances.len()];
            for (s, cap) in holds {
                let Lit::Amount(m) = cap else { return err("E0205", path, format!("the cap of `{s}` must be an amount")) };
                caps[self.substance(s, &path)?] = mu(*m, &path)?;
            }
            let space = Space { w: sw, h: sh, wrap, grid_w: gw as usize, grid_h: gh as usize, child_offset };
            self.world.grid(space, caps);
            self.grid = Some(name.clone());
        }

        // World rules, in declaration order, then world genesis.
        for d in &w.decls {
            self.at.set(d.pos);
            match &d.node {
                WDecl::Rule { name, on, body } => self.rule(name, on, body)?,
                WDecl::Genesis(body) => self.world_genesis(body)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn menu(&mut self, kind: &str, items: &[ast::Node<MenuItem>]) -> Result<(), Error> {
        for item in items {
            self.at.set(item.pos);
            match &item.node {
                MenuItem::Sense { name, params } if kind == "senses" => {
                    float_exact(params, None, &self.path(&format!("senses.{name}")))?;
                    self.senses.insert(name.clone(), params.clone());
                }
                MenuItem::Gene { name, params, inputs, weight } if kind == "regulation" => {
                    let path = self.path(&format!("regulation.{name}"));
                    let inputs = match inputs {
                        Lit::Range(a, b) => match (a.as_ref(), b.as_ref()) {
                            (Lit::Int(a), Lit::Int(b)) => (*a, *b),
                            _ => return err("E0205", path, "`inputs` must be an integer range such as `1..4`"),
                        },
                        Lit::Int(n) => (*n, *n),
                        _ => return err("E0205", path, "`inputs` must be an integer or an integer range"),
                    };
                    let weight = match weight {
                        None => None,
                        Some(l) => Some(float_range(l, &path)?),
                    };
                    self.regulation.insert(name.clone(), GeneDef { params: params.clone(), inputs, weight });
                }
                MenuItem::Action { name, params, cost, per, requires } if kind == "actions" => {
                    let path = self.path(&format!("actions.{name}"));
                    float_exact(params, Some(cost), &path)?;
                    let Lit::Amount(cost) = cost else { return err("E0205", path, "a cost must be an amount") };
                    let per_distance = match per.as_deref() {
                        None => false,
                        Some("distance") => true,
                        Some(other) => return err("E1001", path, format!("cost per `{other}` is not supported")),
                    };
                    let requires = match requires {
                        None => None,
                        Some(Expr::Binary(BinOp::Ge, s, a)) => Some((self.substance(name_of(s, &path)?, &path)?, amount(a, &path)?)),
                        Some(_) => return err("E1001", path, "emerge-ref supports `requires <substance> >= <amount>` only"),
                    };
                    let def = ActionDef { params: params.clone(), cost: mu(*cost, &path)?, per_distance, requires };
                    self.actions.insert(name.clone(), def);
                }
                _ => return err("E0501", self.path(kind), "menu item does not match its menu"),
            }
        }
        Ok(())
    }

    fn reaction_space(&mut self, entries: &[ast::Node<ast::Entry>]) -> Result<(), Error> {
        let path = self.path("reaction_space");
        for e in entries {
            self.at.set(e.pos);
            match e.node.key.as_str() {
                "include" => {
                    let Value::Expr(Expr::List(items)) = &e.node.value else { return err("E0205", path, "`include` is a list of reactions") };
                    let ids = items.iter().map(|v| lookup(&self.reactions, name_of(expr(v)?, &path)?, &path, "reaction")).collect::<Result<_, _>>()?;
                    self.included = Some(ids);
                }
                "max_rate" => {
                    let m = amount(expr(&e.node.value)?, &path)?;
                    if m > F32_EXACT {
                        return err("E0204", path, "`max_rate` is above 2^24 µ, so gene rates would not convert to f32 exactly (spec §3.5)");
                    }
                    self.max_rate = Some(m);
                }
                "generate" | "generated_max_rate" | "exclude" => {}
                k => return err("E0115", path, format!("unknown setting `{k}`")),
            }
        }
        self.notes.push(format!("{} (generated reactions are reached only by mutation)", path));
        Ok(())
    }

    /// A sink that accepts `s`.
    fn accepting_sink(&self, sink: &str, s: SubstanceId, path: &str) -> Result<SinkId, Error> {
        let k = lookup(&self.sinks, sink, path, "sink")?;
        if !self.world.sinks[k].accepts(s) {
            return err("E0704", path, format!("sink `{sink}` does not accept `{}`", self.world.chem.substances[s].name));
        }
        Ok(k)
    }

    /// A source that declares `s` among what it emits.
    fn emitting_source(&self, source: &str, s: SubstanceId, path: &str) -> Result<SourceId, Error> {
        let id = lookup(&self.sources, source, path, "source")?;
        if !self.emits[source].contains(&s) {
            return err("E0703", path, format!("source `{source}` does not emit `{}`", self.world.chem.substances[s].name));
        }
        Ok(id)
    }

    /// Every argument of a gene call names a parameter on its menu item and lies within the
    /// parameter's declared range, choices or kind (spec §5.4: the world restricts).
    fn check_args(&self, c: &ast::Call, params: &[(String, ast::Param)], path: &str) -> Result<(), Error> {
        for a in &c.args {
            let Some(key) = &a.name else { return err("E0502", path, format!("`{}` takes named arguments", c.name)) };
            let Some((_, param)) = params.iter().find(|(n, _)| n == key) else {
                return err("E0502", path, format!("`{}` has no parameter `{key}` on the world's menu", c.name));
            };
            let v = expr(&a.value)?;
            let outside = |shown: String, lo: &Lit, hi: &Lit| {
                err("E0601", path, format!("`{key}: {shown}` is outside the menu's range {}..{}", lit_text(lo), lit_text(hi)))
            };
            match param {
                ast::Param::Range(Lit::Range(lo, hi)) => match (lo.as_ref(), hi.as_ref()) {
                    (Lit::Float(a), Lit::Float(b)) => {
                        let x = float(v, path)?;
                        if !(parse_f32(a, path)?..=parse_f32(b, path)?).contains(&x) {
                            return outside(format!("{x:?}"), lo, hi);
                        }
                    }
                    (Lit::Int(a), Lit::Int(b)) => {
                        let x = int(v, path)?;
                        if !(*a..=*b).contains(&x) {
                            return outside(x.to_string(), lo, hi);
                        }
                    }
                    (Lit::Amount(a), Lit::Amount(b)) => {
                        let x = amount(v, path)? as u64;
                        if !(*a..=*b).contains(&x) {
                            return outside(format!("{x}mu"), lo, hi);
                        }
                    }
                    _ => return err("E0205", path, format!("the menu's range for `{key}` mixes kinds")),
                },
                ast::Param::Range(_) => return err("E0205", path, format!("the menu's range for `{key}` is not a range")),
                ast::Param::Any(kind) => match kind.as_str() {
                    "substance" => {
                        self.substance(name_of(v, path)?, path)?;
                    }
                    "quantity" => match v {
                        Expr::Path(ast::Path { root, segs }) if Some(root.as_str()) == self.grid.as_deref() => {
                            let [Seg::Field(s)] = &segs[..] else { return err("E0601", path, format!("`{key}` must be `{root}.<substance>`")) };
                            self.held(self.substance(s, path)?, path)?;
                        }
                        Expr::Path(ast::Path { root, segs }) if segs.is_empty() && self.signal_fields.contains(root) => {}
                        _ => return err("E0601", path, format!("`{key}` must be a field quantity such as `cells.body`")),
                    },
                    "signal" => {
                        let n = name_of(v, path)?;
                        if !self.signal_fields.iter().any(|f| f == n) {
                            return err("E0114", path, format!("`{n}` is not a signal field"));
                        }
                    }
                    other => return err("E1001", path, format!("parameters of kind `any {other}` are not supported by emerge-ref yet")),
                },
                ast::Param::Choices(choices) => {
                    let n = name_of(v, path)?;
                    if !choices.iter().any(|c| c == n) {
                        return err("E0601", path, format!("`{key}: {n}` is not one of the menu's choices [{}]", choices.join(", ")));
                    }
                }
                ast::Param::Name(want) => {
                    if name_of(v, path)? != want {
                        return err("E0601", path, format!("`{key}` must be `{want}`"));
                    }
                }
            }
        }
        Ok(())
    }

    /// The conserved field can hold `s`.
    fn held(&self, s: SubstanceId, path: &str) -> Result<SubstanceId, Error> {
        if self.world.cells.first().is_some_and(|c| c.caps[s] == 0) {
            let field = self.grid.as_deref().unwrap_or("");
            return err("E0706", path, format!("field `{field}` cannot hold `{}`", self.world.chem.substances[s].name));
        }
        Ok(s)
    }

    fn rule(&mut self, name: &str, on: &str, body: &ast::Block) -> Result<(), Error> {
        let path = self.path(&format!("rules.{name}"));
        if self.signal_fields.iter().any(|f| f == on) {
            self.left_out(path, format!("rule `{name}` on signal field `{on}` is not run"));
            return Ok(());
        }
        if self.grid.as_deref() != Some(on) {
            return err("E0114", path, format!("unknown field `{on}`"));
        }
        for stmt in body {
            self.at.set(stmt.pos);
            let Stmt::Call { call: c, .. } = &stmt.node else {
                return err("E1001", path, "emerge-ref runs only `transfer`, `react` and `diffuse_conserved` statements in rules");
            };
            let law = match c.name.as_str() {
                "transfer" => {
                    only_args(c, &["from", "to", "amount", "overflow"], &path)?;
                    let s = self.held(self.substance(name_of(subject(c, &path)?, &path)?, &path)?, &path)?;
                    let from = name_of(arg(c, "from", &path)?, &path)?;
                    if !self.sources.contains_key(from) {
                        return err("E1001", path, format!("`{from}` is not a source; emerge-ref supports rule transfers from sources only"));
                    }
                    let source = self.emitting_source(from, s, &path)?;
                    if name_of(arg(c, "to", &path)?, &path)? != "self" {
                        return err("E0301", path, "a rule's transfer must go `to: self`");
                    }
                    let overflow = match opt_arg(c, "overflow") {
                        Some(o) => Some(self.accepting_sink(name_of(o, &path)?, s, &path)?),
                        None => None,
                    };
                    Law::Inflow { s, source, amount: amount(arg(c, "amount", &path)?, &path)?, overflow }
                }
                "react" => {
                    only_args(c, &["rate"], &path)?;
                    let r = lookup(&self.reactions, name_of(subject(c, &path)?, &path)?, &path, "reaction")?;
                    let rx = &self.world.chem.reactions[r];
                    for &(s, _) in rx.inputs.iter().chain(&rx.outputs) {
                        self.held(s, &path)?;
                    }
                    Law::React { reaction: r, rate_bp: fraction(arg(c, "rate", &path)?, &path)? }
                }
                "diffuse_conserved" => {
                    only_args(c, &["rate"], &path)?;
                    let Expr::List(items) = subject(c, &path)? else { return err("E0205", path, "expected a list of substances") };
                    let substances = items
                        .iter()
                        .map(|v| self.held(self.substance(name_of(expr(v)?, &path)?, &path)?, &path))
                        .collect::<Result<_, _>>()?;
                    Law::Diffuse { substances, rate_bp: fraction(arg(c, "rate", &path)?, &path)? }
                }
                other => return err("E1001", path, format!("`{other}` is not supported by emerge-ref yet")),
            };
            self.world.laws.push(law);
        }
        Ok(())
    }

    fn world_genesis(&mut self, body: &ast::Block) -> Result<(), Error> {
        let path = self.path("genesis");
        for stmt in body {
            self.at.set(stmt.pos);
            let Stmt::For { var, field, body } = &stmt.node else {
                return err("E1001", path, "emerge-ref supports `for cell in <field> { transfer(...) }` only");
            };
            if self.grid.as_deref() != Some(field.as_str()) {
                return err("E1001", path, format!("`{field}` is not the conserved field"));
            }
            let mut endow = Vec::new();
            for s in body {
                self.at.set(s.pos);
                let Stmt::Call { call: c, .. } = &s.node else { return err("E0301", path, "expected `transfer(...)`") };
                if c.name != "transfer" || name_of(arg(c, "to", &path)?, &path)? != var {
                    return err("E0301", path, format!("expected `transfer(..., to: {var}, ...)`"));
                }
                only_args(c, &["from", "to", "amount"], &path)?;
                let s = self.substance(name_of(subject(c, &path)?, &path)?, &path)?;
                self.emitting_source(name_of(arg(c, "from", &path)?, &path)?, s, &path)?;
                endow.push((s, amount(arg(c, "amount", &path)?, &path)?));
            }
            for cell in 0..self.world.cells.len() {
                for &(s, a) in &endow {
                    if a > self.world.cells[cell].headroom(s) {
                        return err("E0707", path, format!("{a}µ of `{}` does not fit in a cell", self.world.chem.substances[s].name));
                    }
                    self.world.endow(End::Cell(cell), s, a);
                }
            }
        }
        Ok(())
    }

    fn lineage(&mut self, l: &ast::Lineage) -> Result<(), Error> {
        let mut genome = None;
        for d in &l.decls {
            self.at.set(d.pos);
            match &d.node {
                LDecl::Profile { name, .. } => self.notes.push(format!("{}.profile.{name} (capacity applies to mutation only)", l.name)),
                LDecl::Mutation(_) => self.notes.push(format!("{}.mutation (the run has mutation off)", l.name)),
                LDecl::Seed { signals, modules } => genome = Some(self.genome(&l.name, signals, modules)?),
                LDecl::Genesis(_) => {}
            }
        }
        let Some(genome) = genome else { return err("E0116", &l.name, "a lineage needs a seed genome") };
        for d in &l.decls {
            self.at.set(d.pos);
            if let LDecl::Genesis(body) = &d.node {
                self.lineage_genesis(&l.name, body, &genome)?;
            }
        }
        Ok(())
    }

    fn genome(&self, lineage: &str, signals: &[ast::Node<(String, String)>], modules: &[ast::Node<ast::Module>]) -> Result<Genome, Error> {
        // Signals: declared ones first, then in order of first use.
        let mut names: Vec<String> = Vec::new();
        let mut keep = Vec::new();
        for s in signals {
            self.at.set(s.pos);
            let (n, k) = &s.node;
            names.push(n.clone());
            keep.push(k.parse::<f32>().map_err(|_| Error { code: "E0205", pos: Pos::default(), path: format!("{lineage}.seed.{n}"), message: "bad keep".into() })?);
        }
        let sig = |n: &str, names: &mut Vec<String>, keep: &mut Vec<f32>| -> usize {
            names.iter().position(|x| x == n).unwrap_or_else(|| {
                names.push(n.to_string());
                keep.push(0.0);
                names.len() - 1
            })
        };
        let mut slots = Vec::new();
        let mut expressed = Vec::new();
        for (m, module) in modules.iter().enumerate() {
            let module = &module.node;
            expressed.push(!module.dormant);
            for g in &module.genes {
                self.at.set(g.pos);
                let path = format!("{lineage}.seed.{}{}", module.name, g.node.label.as_ref().map(|l| format!("@{l}")).unwrap_or_default());
                let source = |s: &Option<ast::Source>, names: &mut Vec<String>, keep: &mut Vec<f32>| -> Result<Input, Error> {
                    Ok(match s {
                        None => ONE,
                        Some(ast::Source::Signal(n)) => Input::Signal(sig(n, names, keep)),
                        Some(ast::Source::Const(v)) => Input::Const(parse_f32(v, &path)?),
                    })
                };
                let gene = match &g.node.body {
                    GeneBody::Sense { call: c, outs } => {
                        let Some(params) = self.senses.get(&c.name) else {
                            return err("E0501", path, format!("sense `{}` is not on the world's menu", c.name));
                        };
                        self.check_args(c, params, &path)?;
                        let sense = match c.name.as_str() {
                            "inventory" => Sense::Inventory(self.substance(name_of(arg(c, "s", &path)?, &path)?, &path)?),
                            "gradient" => {
                                let q = arg(c, "q", &path)?;
                                match q {
                                    Expr::Path(ast::Path { root, segs }) if Some(root.as_str()) == self.grid.as_deref() && segs.len() == 1 => {
                                        let Seg::Field(s) = &segs[0] else { return err("E0205", path, "expected `<field>.<substance>`") };
                                        Sense::Gradient(self.substance(s, &path)?)
                                    }
                                    _ => return err("E1001", path, "emerge-ref supports `gradient` of the conserved field only"),
                                }
                            }
                            other => return err("E1001", path, format!("sense `{other}` is not supported by emerge-ref yet")),
                        };
                        let want = if matches!(sense, Sense::Gradient(_)) { 2 } else { 1 };
                        if outs.len() != want {
                            return err("E0601", path, format!("`{}` writes {want} signal(s)", c.name));
                        }
                        let out = sig(&outs[0], &mut names, &mut keep);
                        let out2 = outs.get(1).map(|o| sig(o, &mut names, &mut keep));
                        Gene::Sense { sense, out, out2 }
                    }
                    GeneBody::Regulate { call: c, inputs, out } => {
                        let Some(def) = self.regulation.get(&c.name) else {
                            return err("E0501", path, format!("regulation gene `{}` is not on the world's menu", c.name));
                        };
                        self.check_args(c, &def.params, &path)?;
                        let (lo, hi) = def.inputs;
                        if !(lo..=hi).contains(&(inputs.len() as i64)) {
                            return err("E0601", path, format!("`{}` takes {lo} to {hi} inputs, not {}", c.name, inputs.len()));
                        }
                        for i in inputs {
                            let wt = match &i.weight {
                                Some(wt) => parse_f32(wt, &path)?,
                                None => 1.0,
                            };
                            match def.weight {
                                Some((a, b)) if !(a..=b).contains(&wt) => {
                                    return err("E0601", path, format!("weight {wt:?} on `{}` is outside the menu's range {a:?}..{b:?}", i.signal));
                                }
                                None if i.weight.is_some() => return err("E0601", path, format!("`{}` inputs take no weights", c.name)),
                                _ => {}
                            }
                        }
                        match c.name.as_str() {
                            "weigh" => {
                                let bias = match opt_arg(c, "bias") {
                                    Some(b) => float(b, &path)?,
                                    None => 0.0,
                                };
                                let curve = match opt_arg(c, "curve").map(|v| name_of(v, &path)).transpose()? {
                                    None | Some("linear") => Curve::Linear,
                                    Some("step") => Curve::Step,
                                    Some("sigmoid") => Curve::Sigmoid,
                                    Some("tanh") => Curve::Tanh,
                                    Some(other) => return err("E0601", path, format!("unknown curve `{other}`")),
                                };
                                let mut ins = Vec::new();
                                for i in inputs {
                                    let wt = match &i.weight {
                                        Some(wt) => parse_f32(wt, &path)?,
                                        None => 1.0,
                                    };
                                    ins.push((wt, sig(&i.signal, &mut names, &mut keep)));
                                }
                                Gene::Weigh { bias, curve, inputs: ins, out: sig(out, &mut names, &mut keep) }
                            }
                            "product" => {
                                let [a, b] = &inputs[..] else { return err("E0601", path, "`product` reads two signals") };
                                if a.weight.is_some() || b.weight.is_some() {
                                    return err("E0601", path, "`product` inputs take no weights");
                                }
                                let (a, b) = (sig(&a.signal, &mut names, &mut keep), sig(&b.signal, &mut names, &mut keep));
                                Gene::Product { a, b, out: sig(out, &mut names, &mut keep) }
                            }
                            other => return err("E1001", path, format!("regulation gene `{other}` is not supported by emerge-ref yet")),
                        }
                    }
                    GeneBody::Reaction { name: r, rate: rt, source: s } => {
                        let reaction = lookup(&self.reactions, r, &path, "reaction")?;
                        if self.included.as_ref().is_some_and(|inc| !inc.contains(&reaction)) {
                            return err("E0602", path, format!("reaction `{r}` is not in the reaction space's `include` list"));
                        }
                        let Lit::Amount(rt) = rt else { return err("E0205", path, "a reaction rate is an amount") };
                        let rate = mu(*rt, &path)?;
                        if let Some(max) = self.max_rate.filter(|&m| rate > m) {
                            return err("E0601", path, format!("rate {rate}µ is above the reaction space's `max_rate` of {max}µ"));
                        }
                        Gene::React { reaction, rate, speed: source(s, &mut names, &mut keep)? }
                    }
                    GeneBody::Action { call: c, source: s, threshold } => {
                        let Some(def) = self.actions.get(&c.name) else {
                            return err("E0501", path, format!("action `{}` is not on the world's menu", c.name));
                        };
                        self.check_args(c, &def.params, &path)?;
                        let src = source(s, &mut names, &mut keep)?;
                        let threshold = match threshold {
                            Some(t) => parse_f32(t, &path)?,
                            None => 0.0,
                        };
                        let fee = def.cost;
                        if def.per_distance != (c.name == "move") {
                            return err("E1001", path, format!("emerge-ref charges per distance for `move` only, not `{}`", c.name));
                        }
                        match c.name.as_str() {
                            "move" => Gene::Move { speed: float(arg(c, "speed", &path)?, &path)?, fee_per_distance: fee, src, threshold },
                            "turn" => Gene::Turn { angle_deg: float(arg(c, "angle_deg", &path)?, &path)?, fee, src, threshold },
                            "take" | "drop" => {
                                let s = self.substance(name_of(arg(c, "s", &path)?, &path)?, &path)?;
                                let max = amount(arg(c, "max", &path)?, &path)?;
                                if c.name == "take" {
                                    Gene::Take { s, max, fee, src, threshold }
                                } else {
                                    Gene::Drop { s, max, fee, src, threshold }
                                }
                            }
                            "divide" => {
                                let Some(requires) = def.requires else { return err("E0116", path, "`divide` needs a `requires` condition") };
                                let split = float(arg(c, "split", &path)?, &path)?;
                                if !(0.0..=1.0).contains(&split) {
                                    return err("E0601", path, "`split` must be in 0.0..1.0");
                                }
                                // The child's share in whole basis points (DISCREPANCIES.md).
                                let split_bp = (split * 10_000.0).round() as u32;
                                Gene::Divide { split_bp, requires, fee, src, threshold }
                            }
                            other => return err("E1001", path, format!("action `{other}` is not supported by emerge-ref yet")),
                        }
                    }
                };
                slots.push(Slot { gene, expressed: !g.node.dormant, module: m });
            }
        }
        Ok(Genome { slots, modules: expressed, keep })
    }

    fn lineage_genesis(&mut self, lineage: &str, body: &ast::Block, genome: &Genome) -> Result<(), Error> {
        let path = format!("{lineage}.genesis");
        let mut count = None;
        let mut ranges = None;
        let mut endow = Vec::new();
        for stmt in body {
            self.at.set(stmt.pos);
            match &stmt.node {
                Stmt::Setting { key, value } if key == "count" => count = Some(int(value, &path)?),
                Stmt::Assign { target, value } if target.root == "position" && target.segs.is_empty() => {
                    let c = call(value, &path)?;
                    if c.name != "uniform" {
                        return err("E1001", path, "emerge-ref supports `position = uniform(x: a..b, y: c..d)` only");
                    }
                    ranges = Some((range(arg(&c, "x", &path)?, &path)?, range(arg(&c, "y", &path)?, &path)?));
                }
                Stmt::Call { call: c, .. } if c.name == "transfer" => {
                    only_args(c, &["from", "to", "amount"], &path)?;
                    if name_of(arg(c, "to", &path)?, &path)? != "self" {
                        return err("E0301", path, "a lineage's genesis transfer must go `to: self`");
                    }
                    let s = self.substance(name_of(subject(c, &path)?, &path)?, &path)?;
                    self.emitting_source(name_of(arg(c, "from", &path)?, &path)?, s, &path)?;
                    let a = amount(arg(c, "amount", &path)?, &path)?;
                    if a > self.world.inventory_cap {
                        return err("E0707", path, format!("{a}µ of `{}` is above the inventory cap", self.world.chem.substances[s].name));
                    }
                    endow.push((s, a));
                }
                _ => return err("E0301", path, "unsupported statement in a lineage's genesis"),
            }
        }
        let Some(count) = count else { return err("E0116", path, "genesis needs `count`") };
        let Some(((x0, x1), (y0, y1))) = ranges else { return err("E0116", path, "genesis needs `position`") };
        let space = self.world.space.as_ref().expect("the space is built before lineages");
        if !(0.0 <= x0 && x0 <= x1 && x1 <= space.w && 0.0 <= y0 && y0 <= y1 && y1 <= space.h) {
            return err("E0803", path, format!("`position` must lie within the space, 0.0..{:?} by 0.0..{:?}", space.w, space.h));
        }
        if count < 0 || self.world.entities.len() as i64 + count > self.world.max_entities as i64 {
            return err("E0802", path, "`count` must be at least 0 and keep the population within `max_entities`");
        }
        // Spec §10.2: pass `genesis`, stream = the new entity's ID, draws 0, 1, 2.
        for _ in 0..count {
            let id = self.world.next_id;
            let draws = Stream { seed: self.world.seed, tick: 0, stream: id as u32, domain: philox::DOMAIN_ENTITY, aux: 0, pass: philox::GENESIS };
            let unit = |i: u64| (draws.draw(i) >> 8) as f32 * (1.0 / 16_777_216.0);
            let pos = [x0 + unit(0) * (x1 - x0), y0 + unit(1) * (y1 - y0)];
            let (s, c) = (unit(2) * std::f32::consts::TAU).sin_cos();
            let e = self.world.spawn_at(pos, [c, s], genome.clone());
            for &(s, a) in &endow {
                self.world.endow(End::Entity(e), s, a);
            }
        }
        Ok(())
    }
}

// ---------- reading values ----------

fn lookup<T: Copy>(map: &HashMap<String, T>, n: &str, path: &str, what: &str) -> Result<T, Error> {
    map.get(n).copied().ok_or_else(|| Error { code: "E0114", pos: Pos::default(), path: path.into(), message: format!("unknown {what} `{n}`") })
}

fn expr(v: &Value) -> Result<&Expr, Error> {
    match v {
        Value::Expr(e) => Ok(e),
        _ => err("E0205", "value", "expected an expression"),
    }
}

fn name_of<'e>(e: &'e Expr, path: &str) -> Result<&'e str, Error> {
    match e {
        Expr::Path(ast::Path { root, segs }) if segs.is_empty() => Ok(root),
        _ => err("E0205", path, "expected a name"),
    }
}

fn call(e: &Expr, path: &str) -> Result<ast::Call, Error> {
    match e {
        Expr::Call(c) => Ok(c.clone()),
        _ => err("E0205", path, "expected a call"),
    }
}

fn opt_arg<'c>(c: &'c ast::Call, key: &str) -> Option<&'c Expr> {
    c.args.iter().find(|a| a.name.as_deref() == Some(key)).and_then(|a| match &a.value {
        Value::Expr(e) => Some(e),
        _ => None,
    })
}

fn arg<'c>(c: &'c ast::Call, key: &str, path: &str) -> Result<&'c Expr, Error> {
    opt_arg(c, key).ok_or_else(|| Error { code: "E0116", pos: Pos::default(), path: path.into(), message: format!("`{}` needs `{key}`", c.name) })
}

fn subject<'c>(c: &'c ast::Call, path: &str) -> Result<&'c Expr, Error> {
    match c.args.first() {
        Some(ast::Arg { name: None, value: Value::Expr(e) }) => Ok(e),
        _ => err("E0116", path, format!("`{}` needs a subject", c.name)),
    }
}

/// A literal, looking through a leading minus.
fn lit(e: &Expr) -> Option<(bool, &Lit)> {
    match e {
        Expr::Lit(l) => Some((false, l)),
        Expr::Neg(inner) => match inner.as_ref() {
            Expr::Lit(l) => Some((true, l)),
            _ => None,
        },
        _ => None,
    }
}

/// Amounts that enter a product with a float must convert to `f32` exactly (spec §3.5).
const F32_EXACT: Mu = 1 << 24;

/// Checks a menu item's amount ranges and cost against `F32_EXACT`.
fn float_exact(params: &[(String, ast::Param)], cost: Option<&Lit>, path: &str) -> Result<(), Error> {
    for (key, p) in params {
        if let ast::Param::Range(Lit::Range(_, hi)) = p {
            if matches!(hi.as_ref(), Lit::Amount(m) if *m > F32_EXACT as u64) {
                return err("E0204", path, format!("the range of `{key}` goes above 2^24 µ, so it would not convert to f32 exactly (spec §3.5)"));
            }
        }
    }
    if matches!(cost, Some(Lit::Amount(m)) if *m > F32_EXACT as u64) {
        return err("E0204", path, "the cost is above 2^24 µ, so it would not convert to f32 exactly (spec §3.5)");
    }
    Ok(())
}

/// Rejects arguments a call does not take, so a misspelled name is never ignored.
fn only_args(c: &ast::Call, allowed: &[&str], path: &str) -> Result<(), Error> {
    for a in &c.args {
        if let Some(n) = &a.name {
            if !allowed.contains(&n.as_str()) {
                return err("E0502", path, format!("`{}` takes no argument `{n}`", c.name));
            }
        }
    }
    Ok(())
}

fn lit_text(l: &Lit) -> String {
    match l {
        Lit::Float(s) => s.clone(),
        Lit::Int(i) => i.to_string(),
        Lit::Amount(m) => format!("{m}mu"),
        Lit::Rate(b) => format!("{b}bp"),
        Lit::Bool(b) => b.to_string(),
        Lit::Range(a, b) => format!("{}..{}", lit_text(a), lit_text(b)),
    }
}

fn float_range(l: &Lit, path: &str) -> Result<(f32, f32), Error> {
    match l {
        Lit::Range(a, b) => match (a.as_ref(), b.as_ref()) {
            (Lit::Float(a), Lit::Float(b)) => Ok((parse_f32(a, path)?, parse_f32(b, path)?)),
            _ => err("E0205", path, "expected a float range such as `-16.0..16.0`"),
        },
        _ => err("E0205", path, "expected a float range"),
    }
}

/// A per-tick fraction of an amount, at most `10_000bp`.
fn fraction(e: &Expr, path: &str) -> Result<u32, Error> {
    let r = rate(e, path)?;
    if r > 10_000 {
        return err("E0204", path, format!("{r}bp is more than the whole amount (10_000bp)"));
    }
    Ok(r)
}

fn parse_f32(s: &str, path: &str) -> Result<f32, Error> {
    s.parse().map_err(|_| Error { code: "E0205", pos: Pos::default(), path: path.into(), message: format!("bad number `{s}`") })
}

fn float(e: &Expr, path: &str) -> Result<f32, Error> {
    match lit(e) {
        Some((neg, Lit::Float(s))) => parse_f32(s, path).map(|v| if neg { -v } else { v }),
        _ => err("E0205", path, "expected a float literal such as `1.0`"),
    }
}

fn int(e: &Expr, path: &str) -> Result<i64, Error> {
    match lit(e) {
        Some((neg, Lit::Int(i))) => Ok(if neg { -i } else { *i }),
        _ => err("E0205", path, "expected an integer"),
    }
}

fn boolean(e: &Expr, path: &str) -> Result<bool, Error> {
    match lit(e) {
        Some((false, Lit::Bool(b))) => Ok(*b),
        _ => err("E0205", path, "expected `true` or `false`"),
    }
}

fn mu(m: u64, path: &str) -> Result<Mu, Error> {
    Mu::try_from(m).map_err(|_| Error { code: "E0204", pos: Pos::default(), path: path.into(), message: "amount too large".into() })
}

fn amount(e: &Expr, path: &str) -> Result<Mu, Error> {
    match lit(e) {
        Some((false, Lit::Amount(m))) => mu(*m, path),
        _ => err("E0205", path, "expected an amount such as `5u` or `250mu`"),
    }
}

fn rate(e: &Expr, path: &str) -> Result<u32, Error> {
    match lit(e) {
        Some((false, Lit::Rate(b))) => u32::try_from(*b).map_err(|_| Error { code: "E0204", pos: Pos::default(), path: path.into(), message: "rate too large".into() }),
        _ => err("E0205", path, "expected a rate such as `50bp`"),
    }
}

fn range(e: &Expr, path: &str) -> Result<(f32, f32), Error> {
    match lit(e) {
        Some((false, Lit::Range(a, b))) => {
            let f = |l: &Lit| match l {
                Lit::Float(s) => parse_f32(s, path),
                _ => err("E0205", path, "expected a float range such as `0.0..512.0`"),
            };
            Ok((f(a)?, f(b)?))
        }
        _ => err("E0205", path, "expected a range"),
    }
}
