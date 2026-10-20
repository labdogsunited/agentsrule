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

use crate::chem::{Chemistry, Mu, ReactionId, Side, Side2, SubstanceId};
use crate::mind::{Curve, Gene, Genome, Input, Sense, Slot, ONE};
use crate::mutate::{ActionMenu, Capacity, Menus, Rates, WeighMenu};
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
        let severity = if self.code.starts_with('W') { "warning" } else { "error" };
        write!(f, "{severity}[{}]", self.code)?;
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
    /// The conserved field's name, if any.
    pub field: Option<String>,
}

/// How to build a world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub seed: u64,
    /// Leave out, and list, parts of the world the runner cannot run, instead of failing.
    pub allow_partial: bool,
    /// Vary children at division (spec §8.1). Off is `--mutation off`.
    pub mutation: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { seed: 1, allow_partial: false, mutation: false }
    }
}

/// Parses and checks `src`, then builds its world seeded with `lineages`, in that order.
pub fn compile(src: &str, lineages: &[&str], options: Options) -> Result<Compiled, Error> {
    let file = emerge_lang::parse(src).map_err(|d| Error { code: d.code, pos: Pos { line: d.line, col: d.col }, path: String::new(), message: d.message })?;
    if let Some(p) = emerge_lang::check::check(&file).into_iter().next() {
        return Err(Error { code: p.code, pos: p.pos, path: p.path, message: p.message });
    }
    build(&file, lineages, options)
}

/// The names of the lineages declared in a file, in order.
pub fn lineage_names(file: &ast::File) -> Vec<&str> {
    file.items.iter().filter_map(|i| match &i.node {
        ast::Item::Lineage(l) => Some(l.name.as_str()),
        _ => None,
    }).collect()
}

/// Builds the world of a parsed file whose chemistry `emerge_lang::check` has accepted.
pub fn build(file: &ast::File, lineages: &[&str], options: Options) -> Result<Compiled, Error> {
    // Lineages that `extends` another are built in full, and constants are replaced by their values.
    let file = &emerge_lang::prepare(file).map_err(|p| Error { code: p.code, pos: p.pos, path: p.path, message: p.message })?;
    let at = Cell::new(Pos::default());
    build_at(file, lineages, options, &at).map_err(|mut e| {
        if e.pos.line == 0 {
            e.pos = at.get();
        }
        e
    })
}

fn build_at(file: &ast::File, lineages: &[&str], options: Options, at: &Cell<Pos>) -> Result<Compiled, Error> {
    let worlds: Vec<&ast::Node<ast::Item>> = file.items.iter().filter(|i| matches!(i.node, ast::Item::World(_))).collect();
    let [item] = worlds[..] else {
        return err("E1001", "", format!("emerge-ref runs files with one world; this one has {}", worlds.len()));
    };
    let ast::Item::World(w) = &item.node else { unreachable!() };
    at.set(item.pos);
    let mut c = Compiler::new(w, at)?;
    c.world.seed = options.seed;
    c.world.mutation = options.mutation;
    c.mutation = options.mutation;
    c.rulesets = file.items.iter().filter_map(|i| match &i.node {
        ast::Item::Ruleset(r) => Some(r.name.clone()),
        _ => None,
    }).collect();
    for item in &file.items {
        match &item.node {
            ast::Item::World(_) | ast::Item::Lineage(_) => {}
            ast::Item::Ruleset(_) => {}
            ast::Item::View(v) => c.notes.push(format!("view {}: not used by a run", v.name)),
            ast::Item::Chart(ch) => c.notes.push(format!("chart {}: not used by a run", ch.name)),
            ast::Item::Explore(e) => c.notes.push(format!("explore {}: not used by a run", e.name)),
            ast::Item::Patch(p) => c.notes.push(format!("patch {} base {}: not applied by a run", p.target, p.base)),
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
    c.partial.sort_by_key(|e| (e.pos.line, e.pos.col));
    if !options.allow_partial && !c.partial.is_empty() {
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
    Ok(Compiled { world: c.world, notes: c.notes, partial: c.partial, field: c.grid })
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
    /// The reaction space in mutation order, with each reaction's maximum gene rate.
    space: Vec<(ReactionId, Mu)>,
    mutation: bool,
    rulesets: Vec<String>,
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
        // `reaction_space` below adds generated reactions.
        let mut substances = HashMap::new();
        let mut reactions = HashMap::new();
        for d in &w.decls {
            at.set(d.pos);
            match &d.node {
                WDecl::Config { name, entries } if name == "second_law" => {
                    for e in entries {
                        at.set(e.pos);
                        match (e.node.key.as_str(), &e.node.value) {
                            ("min_drop", Value::Expr(Expr::Lit(Lit::Int(m)))) if *m >= 1 => chem.min_drop = *m,
                            ("min_drop", _) => return err("E0203", p("second_law.min_drop"), "`min_drop` must be a whole number of at least 1"),
                            (k, _) => return err("E0115", p("second_law"), format!("unknown setting `{k}`")),
                        }
                    }
                }
                WDecl::Substance { name, comp, potential } => {
                    let terms: Vec<(&str, u32)> = comp.iter().map(|t| (t.name.as_str(), t.coeff.unwrap_or(1) as u32)).collect();
                    substances.insert(name.clone(), chem.substance(name, &terms, *potential));
                }
                WDecl::Reaction { name, formula, catalyze } => {
                    let side = |terms: &[ast::Term]| -> Vec<(SubstanceId, u32)> {
                        terms.iter().map(|t| (substances[&t.name], t.coeff.unwrap_or(1) as u32)).collect()
                    };
                    let (i, o) = (side(&formula.inputs), side(&formula.outputs));
                    let r = chem.reaction(name, &i, &o).map_err(|e| Error { code: "E0401", pos: Pos::default(), path: p(&format!("reactions.{name}")), message: format!("{e:?}") })?;
                    if let Some(c) = catalyze {
                        chem.reactions[r].catalyst = Some(catalyst(c, &substances, &p(&format!("reactions.{name}.catalyze")))?);
                    }
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
        let reactions_declared = chem.reactions.len();
        let space = reaction_space(w, &mut chem, &reactions, at)?;
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
            max_rate: space.max_rate,
            included: Some(space.included),
            space: space.entries.clone(),
            mutation: false,
            rulesets: Vec::new(),
        };
        let generated = space.entries.iter().filter(|(r, _)| *r >= reactions_declared).count();
        c.notes.push(format!(
            "{}.reaction_space: {} reactions for genes, {generated} of them generated; only mutation reaches the generated ones",
            w.name,
            space.entries.len()
        ));
        c.declarations()?;
        Ok(c)
    }

    /// `q` as a substance of the conserved field: `<field>.<substance>`.
    fn field_substance(&self, q: &Expr, sense: &str, path: &str) -> Result<SubstanceId, Error> {
        match q {
            Expr::Path(ast::Path { root, segs }) if Some(root.as_str()) == self.grid.as_deref() && segs.len() == 1 => {
                let Seg::Field(s) = &segs[0] else { return err("E0205", path, "expected `<field>.<substance>`") };
                self.substance(s, path)
            }
            _ => err("E1001", path, format!("emerge-ref supports `{sense}` of the conserved field only")),
        }
    }

    /// Contact needs a space with an interaction radius.
    fn reach(&self, what: &str, path: &str) -> Result<(), Error> {
        if self.world.space.as_ref().is_some_and(|s| s.radius > 0.0) {
            return Ok(());
        }
        err("E0116", path, format!("`{what}` needs a space with `max_interaction_radius`"))
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
                        "decompose" => self.world.decompose = Some(lookup(&self.reactions, name_of(v, &path)?, &path, "reaction")?),
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
                WDecl::Config { name, .. } if name == "reaction_space" => {}
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
        let radius = match opt_arg(&space, "max_interaction_radius") {
            Some(r) => float(r, &sp)?,
            None => 0.0,
        };
        for d in &w.decls {
            self.at.set(d.pos);
            let WDecl::Field { name, kind, holds, .. } = &d.node else { continue };
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
            let space = Space { w: sw, h: sh, wrap, grid_w: gw as usize, grid_h: gh as usize, child_offset, radius };
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
        self.world.menus = self.menus()?;
        Ok(())
    }

    /// What mutation may produce: the menu items the runner supports, with their ranges.
    fn menus(&self) -> Result<Menus, Error> {
        let path = self.path("menus");
        let range = |params: &[(String, ast::Param)], key: &str| -> Result<(f32, f32), Error> {
            match params.iter().find(|(k, _)| k == key) {
                Some((_, ast::Param::Range(Lit::Range(a, b)))) => {
                    let f = |l: &Lit| -> Result<f32, Error> {
                        Ok(match l {
                            Lit::Float(x) => parse_f32(x, &path)?,
                            Lit::Amount(m) => *m as f32,
                            Lit::Int(i) => *i as f32,
                            _ => return err("E0205", path.clone(), format!("`{key}` must be a numeric range")),
                        })
                    };
                    Ok((f(a)?, f(b)?))
                }
                _ => err("E0116", path.clone(), format!("the menu gives no range for `{key}`")),
            }
        };
        // The substances an action's `s` may name: all for `any substance`, else the listed ones.
        let n = self.world.chem.substances.len();
        let subs = |params: &[(String, ast::Param)]| -> Result<Vec<SubstanceId>, Error> {
            match params.iter().find(|(k, _)| k == "s").map(|(_, p)| p) {
                Some(ast::Param::Any(kind)) if kind == "substance" => Ok((0..n).collect()),
                Some(ast::Param::Name(name)) => Ok(vec![self.substance(name, &path)?]),
                Some(ast::Param::Choices(names)) => {
                    let mut ids = names.iter().map(|x| self.substance(x, &path)).collect::<Result<Vec<_>, _>>()?;
                    ids.sort_unstable();
                    ids.dedup();
                    Ok(ids)
                }
                _ => Ok(Vec::new()),
            }
        };
        let action = |name: &str, key: &str| -> Result<Option<ActionMenu>, Error> {
            match self.actions.get(name) {
                Some(a) => Ok(Some(ActionMenu { range: range(&a.params, key)?, fee: a.cost, substances: subs(&a.params)? })),
                None => Ok(None),
            }
        };
        let weigh = match self.regulation.get("weigh") {
            None => None,
            Some(g) => {
                let curves = match g.params.iter().find(|(k, _)| k == "curve") {
                    Some((_, ast::Param::Choices(cs))) => cs
                        .iter()
                        .filter_map(|c| match c.as_str() {
                            "linear" => Some(Curve::Linear),
                            "step" => Some(Curve::Step),
                            "sigmoid" => Some(Curve::Sigmoid),
                            "tanh" => Some(Curve::Tanh),
                            _ => None,
                        })
                        .collect(),
                    _ => vec![Curve::Linear],
                };
                let Some(weight) = g.weight else { return err("E0116", path, "`weigh` needs a weight range") };
                let inputs = (g.inputs.0.max(1) as usize, g.inputs.1.max(1) as usize);
                Some(WeighMenu { bias: range(&g.params, "bias")?, curves, inputs, weight })
            }
        };
        let divide = match self.actions.get("divide") {
            Some(a) => match a.requires {
                Some(r) => Some((ActionMenu { range: range(&a.params, "split")?, fee: a.cost, substances: Vec::new() }, r)),
                None => None,
            },
            None => None,
        };
        let caps = self.world.cells.first().map(|c| c.caps).unwrap_or_default();
        let n = self.world.chem.substances.len();
        let sense = |name: &str, keys: &[&str]| -> Result<Option<Vec<(f32, f32)>>, Error> {
            match self.senses.get(name) {
                Some(params) => Ok(Some(keys.iter().map(|k| range(params, k)).collect::<Result<_, _>>()?)),
                None => Ok(None),
            }
        };
        let contact = self.world.space.as_ref().is_some_and(|s| s.radius > 0.0);
        let field = self.grid.is_some();
        Ok(Menus {
            substances: (0..n).collect(),
            field: (0..n).filter(|&s| caps[s] > 0).collect(),
            inventory: self.senses.contains_key("inventory"),
            delta: self.senses.contains_key("delta"),
            gradient: self.senses.contains_key("gradient") && self.grid.is_some(),
            probe: sense("probe", &["angle_deg", "dist"])?.filter(|_| field).map(|r| (r[0], r[1])),
            crowd: sense("crowd", &["radius"])?.filter(|_| contact).map(|r| r[0]),
            neighbour: sense("neighbour", &["rank"])?.filter(|_| contact).map(|r| r[0]),
            clock: sense("clock", &["period"])?.map(|r| r[0]),
            weigh,
            product: self.regulation.contains_key("product"),
            moves: action("move", "speed")?,
            turn: action("turn", "angle_deg")?,
            take: action("take", "max")?,
            drop: action("drop", "max")?,
            give: action("give", "max")?.filter(|_| contact),
            steal: action("steal", "max")?.filter(|_| contact),
            guard: match self.actions.get("guard") {
                Some(a) if contact => Some(ActionMenu { range: (0.0, 0.0), fee: a.cost, substances: subs(&a.params)? }),
                _ => None,
            },
            divide,
            reactions: self.space.clone(),
        })
    }

    fn menu(&mut self, kind: &str, items: &[ast::Node<MenuItem>]) -> Result<(), Error> {
        for item in items {
            self.at.set(item.pos);
            match &item.node {
                MenuItem::Sense { name, params } if kind == "senses" => {
                    let path = self.path(&format!("senses.{name}"));
                    float_exact(params, None, &path)?;
                    if !SENSES.contains(&name.as_str()) {
                        self.left_out(path, format!("sense `{name}` is not run: no gene can use it, and mutation does not produce it"));
                    }
                    self.senses.insert(name.clone(), params.clone());
                }
                MenuItem::Gene { name, params, inputs, weight } if kind == "regulation" => {
                    let path = self.path(&format!("regulation.{name}"));
                    if !REGULATION.contains(&name.as_str()) {
                        self.left_out(path.clone(), format!("regulation gene `{name}` is not run: no gene can use it, and mutation does not produce it"));
                    }
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
                    if !ACTIONS.contains(&name.as_str()) {
                        self.left_out(path.clone(), format!("action `{name}` is not run: no gene can use it, and mutation does not produce it"));
                    }
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
        let (mut cap, mut rates) = (None, None);
        let lineage_pos = self.at.get();
        for d in &l.decls {
            self.at.set(d.pos);
            match &d.node {
                LDecl::Profile { name, entries } => {
                    if cap.is_some() {
                        self.notes.push(format!("{}.profile.{name}: not used; emerge-ref uses the first profile", l.name));
                    } else {
                        cap = Some(self.capacity(&l.name, entries)?);
                    }
                }
                LDecl::Mutation(entries) => rates = Some(self.rates(&l.name, entries)?),
                LDecl::Seed { signals, modules } => genome = Some(self.genome(&l.name, signals, modules)?),
                LDecl::Genesis(_) => {}
            }
        }
        let Some(genome) = genome else { return err("E0116", &l.name, "a lineage needs a seed genome") };
        let variation = match (self.mutation, rates, cap) {
            (false, Some(_), _) => {
                self.notes.push(format!("{}.mutation: not used, because mutation is off", l.name));
                None
            }
            (true, Some(rates), Some(cap)) => {
                self.at.set(lineage_pos);
                if let Err(e) = crate::mutate::validate(&genome, &cap, &self.world.menus) {
                    return err("E0601", format!("{}.seed", l.name), format!("the seed is not a genome mutation could keep: {e}"));
                }
                Some((rates, cap))
            }
            (true, Some(_), None) => {
                self.at.set(lineage_pos);
                return err("E0116", &l.name, "a lineage with `mutation` needs a `profile` for its capacity");
            }
            (_, None, _) => None,
        };
        let lineage = self.world.variation.len();
        self.world.variation.push(variation);
        let first = self.world.entities.len();
        for d in &l.decls {
            self.at.set(d.pos);
            if let LDecl::Genesis(body) = &d.node {
                self.lineage_genesis(&l.name, body, &genome)?;
            }
        }
        for e in &mut self.world.entities[first..] {
            e.lineage = lineage;
        }
        Ok(())
    }

    fn capacity(&self, lineage: &str, entries: &[ast::Node<ast::Entry>]) -> Result<Capacity, Error> {
        let path = format!("{lineage}.profile");
        let mut cap = Capacity { genes: 0, signals: 0, modules: 0 };
        for e in entries {
            self.at.set(e.pos);
            let n = int(expr(&e.node.value)?, &path)?.max(0) as usize;
            match e.node.key.as_str() {
                "genes" => cap.genes = n,
                "signals" => cap.signals = n,
                "modules" => cap.modules = n,
                k => return err("E0115", path, format!("unknown setting `{k}`")),
            }
        }
        Ok(cap)
    }

    fn rates(&mut self, lineage: &str, entries: &[ast::Node<ast::Entry>]) -> Result<Rates, Error> {
        let path = format!("{lineage}.mutation");
        let mut r = Rates::default();
        for e in entries {
            self.at.set(e.pos);
            let v = expr(&e.node.value)?;
            if e.node.key == "rules" {
                let name = name_of(v, &path)?;
                if !self.rulesets.iter().any(|r| r == name) {
                    return err("E0114", path, format!("unknown ruleset `{name}`"));
                }
                if self.mutation {
                    self.left_out(path.clone(), format!("ruleset `{name}` is not applied (rewrite rules are not implemented)"));
                } else {
                    self.notes.push(format!("ruleset {name}: not used, because mutation is off"));
                }
                continue;
            }
            if matches!(v, Expr::Path(p) if p.root == "evolvable") {
                return err("E1001", path, format!("`{}: evolvable` is not supported by emerge-ref yet", e.node.key));
            }
            let x = float(v, &path)?;
            if !(0.0..=1.0).contains(&x) {
                return err("E0204", path, format!("`{}: {x:?}` must be in 0.0..1.0", e.node.key));
            }
            let slot = match e.node.key.as_str() {
                "rate" => &mut r.rate,
                "param_sigma" => &mut r.param_sigma,
                "expression_flip" => &mut r.expression_flip,
                "rewire" => &mut r.rewire,
                "diverge" => &mut r.diverge,
                "duplicate_gene" => &mut r.duplicate_gene,
                "insert_gene" => &mut r.insert_gene,
                "delete_gene" => &mut r.delete_gene,
                "duplicate_module" => &mut r.duplicate_module,
                "delete_module" => &mut r.delete_module,
                "split_module" => &mut r.split_module,
                "merge_modules" => &mut r.merge_modules,
                k => return err("E0115", path, format!("unknown setting `{k}`")),
            };
            *slot = x;
        }
        Ok(r)
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
                            "gradient" => Sense::Gradient(self.field_substance(arg(c, "q", &path)?, "gradient", &path)?),
                            "probe" => Sense::Probe {
                                s: self.field_substance(arg(c, "q", &path)?, "probe", &path)?,
                                angle_deg: float(arg(c, "angle_deg", &path)?, &path)?,
                                dist: float(arg(c, "dist", &path)?, &path)?,
                            },
                            "crowd" => Sense::Crowd { radius: float(arg(c, "radius", &path)?, &path)? },
                            "neighbour" => Sense::Neighbour {
                                s: self.substance(name_of(arg(c, "s", &path)?, &path)?, &path)?,
                                rank: int(arg(c, "rank", &path)?, &path)? as u32,
                            },
                            "clock" => Sense::Clock { period: int(arg(c, "period", &path)?, &path)?.max(1) as u32 },
                            "delta" => Sense::Delta(self.substance(name_of(arg(c, "s", &path)?, &path)?, &path)?),
                            other => return err("E1001", path, format!("sense `{other}` is not supported by emerge-ref yet")),
                        };
                        if matches!(sense, Sense::Crowd { .. } | Sense::Neighbour { .. }) {
                            self.reach(&c.name, &path)?;
                        }
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
                            "take" | "drop" | "give" | "steal" => {
                                if matches!(c.name.as_str(), "give" | "steal") {
                                    self.reach(&c.name, &path)?;
                                }
                                let s = self.substance(name_of(arg(c, "s", &path)?, &path)?, &path)?;
                                let max = amount(arg(c, "max", &path)?, &path)?;
                                match c.name.as_str() {
                                    "take" => Gene::Take { s, max, fee, src, threshold },
                                    "drop" => Gene::Drop { s, max, fee, src, threshold },
                                    "give" => Gene::Give { s, max, fee, src, threshold },
                                    _ => Gene::Steal { s, max, fee, src, threshold },
                                }
                            }
                            "guard" => {
                                self.reach("guard", &path)?;
                                let s = self.substance(name_of(arg(c, "s", &path)?, &path)?, &path)?;
                                Gene::Guard { s, fee, src, threshold }
                            }
                            "divide" => {
                                let Some(requires) = def.requires else { return err("E0116", path, "`divide` needs a `requires` condition") };
                                let split = float(arg(c, "split", &path)?, &path)?;
                                if !(0.0..=1.0).contains(&split) {
                                    return err("E0601", path, "`split` must be in 0.0..1.0");
                                }
                                // The child's share in whole basis points (spec §7.5).
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

// ---------- the reaction space ----------

struct ReactionSpace {
    included: Vec<ReactionId>,
    max_rate: Option<Mu>,
    /// Every reaction a gene may name, in mutation order, with its maximum gene rate.
    entries: Vec<(ReactionId, Mu)>,
}

/// Builds the reaction space (spec §4.3): the `include` list, plus with `generate:
/// balanced(...)` every balanced, downhill canonical reaction not in `exclude`, added to
/// `chem` under its formula. Entries are in lexicographic order of their formulas as
/// substance-order term lists, so neighbouring entries are similar reactions; a declared
/// reaction that generation finds again keeps its ID.
fn reaction_space(w: &ast::World, chem: &mut Chemistry, reactions: &HashMap<String, ReactionId>, at: &Cell<Pos>) -> Result<ReactionSpace, Error> {
    let path = format!("{}.reaction_space", w.name);
    let block = w.decls.iter().find_map(|d| match &d.node {
        WDecl::Config { name, entries } if name == "reaction_space" => Some((d.pos, entries)),
        _ => None,
    });
    let limit = w.decls.iter().find_map(|d| match &d.node {
        WDecl::Config { name, entries } if name == "limits" => entries.iter().find_map(|e| match (&e.node.key, &e.node.value) {
            (k, Value::Expr(Expr::Lit(Lit::Int(n)))) if k == "max_reaction_space" => Some(*n as usize),
            _ => None,
        }),
        _ => None,
    });
    let Some((pos, entries)) = block else {
        // No reaction space: genes may name any declared reaction, at any rate.
        let all: Vec<ReactionId> = (0..chem.reactions.len()).collect();
        return Ok(ReactionSpace { entries: all.iter().map(|&r| (r, F32_EXACT)).collect(), included: all, max_rate: None });
    };
    at.set(pos);
    let (mut included, mut max_rate, mut generated_max, mut generate, mut exclude) = (Vec::new(), None, None, None, Vec::new());
    let ids = |v: &Value| -> Result<ReactionId, Error> { lookup(reactions, name_of(expr(v)?, &path)?, &path, "reaction") };
    for e in entries {
        at.set(e.pos);
        let list = || match &e.node.value {
            Value::Expr(Expr::List(items)) => Ok(items),
            _ => err("E0205", &path, format!("`{}` is a list", e.node.key)),
        };
        match e.node.key.as_str() {
            "include" => included = list()?.iter().map(ids).collect::<Result<_, _>>()?,
            "max_rate" | "generated_max_rate" => {
                let m = amount(expr(&e.node.value)?, &path)?;
                if m > F32_EXACT {
                    return err("E0204", path, format!("`{}` is above 2^24 µ, so gene rates would not convert to f32 exactly (spec §3.5)", e.node.key));
                }
                if e.node.key == "max_rate" { max_rate = Some(m) } else { generated_max = Some(m) }
            }
            "generate" => match expr(&e.node.value)? {
                Expr::Path(p) if p.root == "none" && p.segs.is_empty() => generate = None,
                Expr::Call(c) if c.name == "balanced" => {
                    only_args(c, &["max_inputs", "max_outputs", "max_coeff"], &path)?;
                    let n = |k: &str| -> Result<i64, Error> { int(arg(c, k, &path)?, &path) };
                    generate = Some((n("max_inputs")? as usize, n("max_outputs")? as usize, n("max_coeff")? as u32));
                }
                _ => return err("E0205", path, "`generate` is `balanced(...)` or `none`"),
            },
            "exclude" => {
                for v in list()? {
                    match v {
                        Value::Formula(f) => {
                            let order = |n: &str| chem.substances.iter().position(|s| s.name == n).unwrap_or(usize::MAX);
                            let canon = emerge_lang::check::canonical(f, &order);
                            let side = |terms: &[ast::Term]| -> Result<Side, Error> {
                                terms
                                    .iter()
                                    .map(|t| match chem.substances.iter().position(|s| s.name == t.name) {
                                        Some(s) => Ok((s, t.coeff.unwrap_or(1) as u32)),
                                        None => err("E0114", &path, format!("unknown substance `{}`", t.name)),
                                    })
                                    .collect()
                            };
                            exclude.push((side(&canon.inputs)?, side(&canon.outputs)?));
                        }
                        v => {
                            let r = &chem.reactions[ids(v)?];
                            exclude.push((r.inputs.clone(), r.outputs.clone()));
                        }
                    }
                }
            }
            k => return err("E0115", path, format!("unknown setting `{k}`")),
        }
    }
    let generated_max = generated_max.or(max_rate).unwrap_or(F32_EXACT);
    let declared = |chem: &Chemistry, sides: &Side2| chem.reactions.iter().position(|r| r.inputs == sides.0 && r.outputs == sides.1);
    let mut space: Vec<(Side2, ReactionId, Mu)> = included
        .iter()
        .map(|&r| ((chem.reactions[r].inputs.clone(), chem.reactions[r].outputs.clone()), r, max_rate.unwrap_or(F32_EXACT)))
        .collect();
    if let Some((mi, mo, mc)) = generate {
        for sides in chem.balanced(mi, mo, mc) {
            if exclude.contains(&sides) {
                continue;
            }
            let id = match declared(chem, &sides) {
                Some(r) if included.contains(&r) => continue,
                Some(r) => r,
                None => {
                    let name = chem.formula(&sides.0, &sides.1);
                    chem.reaction(&name, &sides.0, &sides.1).map_err(|e| Error { code: "E0401", pos, path: path.clone(), message: format!("{e:?}") })?
                }
            };
            space.push((sides, id, generated_max));
        }
    }
    space.sort_by(|a, b| a.0.cmp(&b.0));
    let limit = limit.unwrap_or(65_536);
    if space.len() > limit {
        at.set(pos);
        return err("E0804", path, format!("the reaction space has {} reactions, more than `max_reaction_space` ({limit})", space.len()));
    }
    Ok(ReactionSpace { included, max_rate, entries: space.into_iter().map(|(_, r, m)| (r, m)).collect() })
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

/// Menu items emerge-ref runs. Other items are left out of a partial run.
const SENSES: &[&str] = &["inventory", "gradient", "probe", "crowd", "neighbour", "clock", "delta"];
const REGULATION: &[&str] = &["weigh", "product"];
const ACTIONS: &[&str] = &["move", "turn", "take", "drop", "give", "steal", "guard", "divide"];

fn opt_arg<'c>(c: &'c ast::Call, key: &str) -> Option<&'c Expr> {
    c.args.iter().find(|a| a.name.as_deref() == Some(key)).and_then(|a| match &a.value {
        Value::Expr(e) => Some(e),
        _ => None,
    })
}

/// `catalyze s(half: 100mu, boost: 10.0)` (spec §4.5).
fn catalyst(c: &ast::Call, substances: &HashMap<String, SubstanceId>, path: &str) -> Result<crate::chem::Catalyst, Error> {
    only_args(c, &["half", "boost"], path)?;
    let s = lookup(substances, &c.name, path, "substance")?;
    let half = amount(arg(c, "half", path)?, path)?;
    let boost = float(arg(c, "boost", path)?, path)?;
    if half == 0 {
        return err("E0204", path, "`half` must be greater than 0mu");
    }
    if !(boost >= 1.0 && boost.is_finite()) {
        return err("E0204", path, "`boost` must be at least 1.0, so a catalyst only speeds a reaction up");
    }
    Ok(crate::chem::Catalyst { s, half, boost })
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
