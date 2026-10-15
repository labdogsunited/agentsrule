//! Compiles a parsed `.em` file into a runnable `World` (spec §15, `language.md`).
//!
//! Covers what the runner can run: the chemistry, one conserved grid field over
//! `Continuous2D`, sources and sinks, waste, upkeep, limits, transfer, reaction and
//! diffusion rules, world genesis, and lineages whose seed genomes use the senses, genes
//! and actions the runner implements. Declarations the runner has no use for yet (signal
//! fields and their rules, the generated reaction space, profiles, mutation, rulesets,
//! views, charts) are skipped and listed in `Compiled::skipped`. A genome or rule that
//! needs something the runner lacks is an error, never silently dropped.

use std::collections::HashMap;

use emerge_lang::ast::{self, BinOp, Expr, GeneBody, LDecl, Lit, MenuItem, Seg, Stmt, Value, WDecl};

use crate::chem::{Chemistry, Mu, ReactionId, SubstanceId};
use crate::mind::{Curve, Gene, Genome, Input, Sense, Slot, ONE};
use crate::philox::{self, Stream};
use crate::world::{End, Law, Sink, SinkId, SourceId, Space, Upkeep, World};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The item's path, such as `Dish.upkeep` or `Drifter.seed.forage`.
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

pub struct Compiled {
    pub world: World,
    /// What was declared but not compiled, because the runner does not use it yet.
    pub skipped: Vec<String>,
}

/// Parses and checks `src`, then builds its world seeded with `lineages`, in that order.
pub fn compile(src: &str, lineages: &[&str], seed: u64) -> Result<Compiled, Error> {
    let file = emerge_lang::parse(src).map_err(|d| Error { path: format!("{}:{}", d.line, d.col), message: format!("{} {}", d.code, d.message) })?;
    if let Some(p) = emerge_lang::check::check(&file).into_iter().next() {
        return Err(Error { path: p.path, message: format!("{} {}", p.code, p.message) });
    }
    let worlds: Vec<&ast::World> = file.items.iter().filter_map(|i| match &i.node {
        ast::Item::World(w) => Some(w),
        _ => None,
    }).collect();
    let [w] = worlds[..] else {
        return Err(Error { path: "file".into(), message: format!("expected one world, found {}", worlds.len()) });
    };
    let mut c = Compiler::new(w)?;
    c.world.seed = seed;
    for item in &file.items {
        match &item.node {
            ast::Item::World(_) | ast::Item::Lineage(_) => {}
            ast::Item::Ruleset(r) => c.skipped.push(format!("ruleset {}", r.name)),
            ast::Item::View(v) => c.skipped.push(format!("view {}", v.name)),
            ast::Item::Chart(ch) => c.skipped.push(format!("chart {}", ch.name)),
            ast::Item::Explore(e) => c.skipped.push(format!("explore {}", e.name)),
            ast::Item::Patch(p) => c.skipped.push(format!("patch {}", p.target)),
        }
    }
    for name in lineages {
        let l = file.items.iter().find_map(|i| match &i.node {
            ast::Item::Lineage(l) if l.name == *name => Some(l),
            _ => None,
        });
        let l = l.ok_or_else(|| Error { path: name.to_string(), message: "no such lineage".into() })?;
        if l.world != w.name {
            return Err(Error { path: l.name.clone(), message: format!("lineage is for `{}`, not `{}`", l.world, w.name) });
        }
        c.lineage(l)?;
    }
    Ok(Compiled { world: c.world, skipped: c.skipped })
}

struct ActionDef {
    cost: Mu,
    per_distance: bool,
    requires: Option<(SubstanceId, Mu)>,
}

struct Compiler<'a> {
    w: &'a ast::World,
    world: World,
    skipped: Vec<String>,
    substances: HashMap<String, SubstanceId>,
    reactions: HashMap<String, ReactionId>,
    sources: HashMap<String, SourceId>,
    sinks: HashMap<String, SinkId>,
    /// The conserved grid field, if any.
    grid: Option<String>,
    senses: Vec<String>,
    actions: HashMap<String, ActionDef>,
    regulation: Vec<String>,
}

fn err<T>(path: impl Into<String>, message: impl Into<String>) -> Result<T, Error> {
    Err(Error { path: path.into(), message: message.into() })
}

impl<'a> Compiler<'a> {
    fn new(w: &'a ast::World) -> Result<Self, Error> {
        let p = |what: &str| format!("{}.{what}", w.name);

        // Chemistry, in declaration order. `check` has already validated it.
        let elements: Vec<&str> = w.decls.iter().filter_map(|d| match &d.node {
            WDecl::Element(e) => Some(e.as_str()),
            _ => None,
        }).collect();
        let mut chem = Chemistry::new(&elements);
        let mut substances = HashMap::new();
        let mut reactions = HashMap::new();
        for d in &w.decls {
            match &d.node {
                WDecl::Config { name, entries } if name == "second_law" => {
                    for e in entries {
                        match (e.node.key.as_str(), &e.node.value) {
                            ("min_drop", Value::Expr(Expr::Lit(Lit::Int(m)))) => chem.min_drop = *m,
                            (k, _) => return err(p("second_law"), format!("unknown setting `{k}`")),
                        }
                    }
                }
                WDecl::Substance { name, comp, potential } => {
                    let terms: Vec<(&str, u32)> = comp.iter().map(|t| (t.name.as_str(), t.coeff.unwrap_or(1) as u32)).collect();
                    substances.insert(name.clone(), chem.substance(name, &terms, *potential));
                }
                WDecl::Reaction { name, formula, catalyze } => {
                    if catalyze.is_some() {
                        return err(p(&format!("reactions.{name}")), "catalysts are not supported by emerge-ref yet");
                    }
                    let side = |terms: &[ast::Term]| -> Vec<(SubstanceId, u32)> {
                        terms.iter().map(|t| (substances[&t.name], t.coeff.unwrap_or(1) as u32)).collect()
                    };
                    let (i, o) = (side(&formula.inputs), side(&formula.outputs));
                    let r = chem.reaction(name, &i, &o).map_err(|e| Error { path: p(&format!("reactions.{name}")), message: format!("{e:?}") })?;
                    reactions.insert(name.clone(), r);
                }
                _ => {}
            }
        }

        // Sinks first: the world is built around its payment route and remains.
        let mut sinks = Vec::new();
        let mut sink_ids = HashMap::new();
        for d in &w.decls {
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
            None => return err(p("remains"), "a world must declare `remains`"),
        };
        let config = |block: &str| w.decls.iter().find_map(|d| match &d.node {
            WDecl::Config { name, entries } if name == block => Some(entries),
            _ => None,
        });
        let Some(up) = config("upkeep") else { return err(p("upkeep"), "a world must declare `upkeep`") };
        let mut upkeep = Upkeep { pay: 0, becomes: 0, vent: 0, base: 0, per_gene: 0, per_signal: 0, per_signal_keep: 0, per_carried_u: 0 };
        let (mut pay, mut becomes, mut vent) = (None, None, None);
        for e in up {
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
                k => return err(path, format!("unknown setting `{k}`")),
            }
        }
        match (pay, becomes, vent) {
            (Some(a), Some(b), Some(c)) => (upkeep.pay, upkeep.becomes, upkeep.vent) = (a, b, c),
            _ => return err(p("upkeep"), "upkeep needs `pay`, `becomes` and `vent`"),
        }
        let world = World::new(chem, upkeep, sinks, remains).map_err(|e| Error { path: p("upkeep"), message: format!("{e:?}") })?;

        let mut c = Compiler {
            w,
            world,
            skipped: Vec::new(),
            substances,
            reactions,
            sources: HashMap::new(),
            sinks: sink_ids,
            grid: None,
            senses: Vec::new(),
            actions: HashMap::new(),
            regulation: Vec::new(),
        };
        c.declarations()?;
        Ok(c)
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
                        k => return err(path, format!("unknown setting `{k}`")),
                    }
                }
                WDecl::Config { name, entries } if name == "limits" => {
                    for e in entries {
                        let path = self.path("limits");
                        let v = expr(&e.node.value)?;
                        match e.node.key.as_str() {
                            "max_entities" => self.world.max_entities = int(v, &path)? as usize,
                            "inventory_cap" => self.world.inventory_cap = amount(v, &path)?,
                            "max_reaction_space" => {}
                            k => return err(path, format!("unknown setting `{k}`")),
                        }
                    }
                }
                WDecl::Config { name, .. } if name == "reaction_space" => {
                    self.skipped.push(format!("{}.reaction_space (generated reactions are used only by mutation)", w.name));
                }
                WDecl::Source { name, emits } => {
                    for s in emits {
                        self.substance(s, &self.path("sources"))?;
                    }
                    let id = self.world.add_source(name);
                    self.sources.insert(name.clone(), id);
                }
                WDecl::Waste { substance, sink } => {
                    let path = self.path("waste");
                    let s = self.substance(substance, &path)?;
                    let k = lookup(&self.sinks, sink, &path, "sink")?;
                    self.world.waste.push((s, k));
                }
                WDecl::Menu { kind, items } => self.menu(kind, items)?,
                _ => {}
            }
        }

        // Fields: one conserved grid over the continuous space.
        let Some(space) = space else { return err(self.path("space"), "a world must declare `space`") };
        if space.name != "Continuous2D" {
            return err(self.path("space"), format!("`{}` is not supported by emerge-ref yet", space.name));
        }
        let sp = self.path("space");
        let (sw, sh) = (float(arg(&space, "w", &sp)?, &sp)?, float(arg(&space, "h", &sp)?, &sp)?);
        let wrap = boolean(arg(&space, "wrap", &sp)?, &sp)?;
        for d in &w.decls {
            let WDecl::Field { name, kind, holds } = &d.node else { continue };
            let path = self.path(&format!("fields.{name}"));
            let Some(holds) = holds else {
                self.skipped.push(format!("{}.fields.{name} (signal fields are not supported by emerge-ref yet)", w.name));
                continue;
            };
            if self.grid.is_some() {
                return err(path, "emerge-ref supports one conserved field");
            }
            if kind.name != "Grid" {
                return err(path, format!("`{}` is not supported by emerge-ref yet", kind.name));
            }
            let (gw, gh) = (int(arg(kind, "w", &path)?, &path)?, int(arg(kind, "h", &path)?, &path)?);
            let mut caps = vec![0; self.world.chem.substances.len()];
            for (s, cap) in holds {
                let Lit::Amount(m) = cap else { return err(path, format!("the cap of `{s}` must be an amount")) };
                caps[self.substance(s, &path)?] = mu(*m, &path)?;
            }
            let space = Space { w: sw, h: sh, wrap, grid_w: gw as usize, grid_h: gh as usize, child_offset };
            self.world.grid(space, caps);
            self.grid = Some(name.clone());
        }

        // World rules, in declaration order, then world genesis.
        for d in &w.decls {
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
            match &item.node {
                MenuItem::Sense { name, .. } if kind == "senses" => self.senses.push(name.clone()),
                MenuItem::Gene { name, .. } if kind == "regulation" => self.regulation.push(name.clone()),
                MenuItem::Action { name, cost, per, requires, .. } if kind == "actions" => {
                    let path = self.path(&format!("actions.{name}"));
                    let Lit::Amount(cost) = cost else { return err(path, "a cost must be an amount") };
                    let per_distance = match per.as_deref() {
                        None => false,
                        Some("distance") => true,
                        Some(other) => return err(path, format!("cost per `{other}` is not supported")),
                    };
                    let requires = match requires {
                        None => None,
                        Some(Expr::Binary(BinOp::Ge, s, a)) => Some((self.substance(name_of(s, &path)?, &path)?, amount(a, &path)?)),
                        Some(_) => return err(path, "emerge-ref supports `requires <substance> >= <amount>` only"),
                    };
                    self.actions.insert(name.clone(), ActionDef { cost: mu(*cost, &path)?, per_distance, requires });
                }
                _ => return err(self.path(kind), "menu item does not match its menu"),
            }
        }
        Ok(())
    }

    fn rule(&mut self, name: &str, on: &str, body: &ast::Block) -> Result<(), Error> {
        let path = self.path(&format!("rules.{name}"));
        if self.grid.as_deref() != Some(on) {
            self.skipped.push(format!("{path} (on `{on}`, which emerge-ref does not run)"));
            return Ok(());
        }
        for stmt in body {
            let Stmt::Call { call: c, .. } = &stmt.node else {
                return err(path, "emerge-ref runs only `transfer`, `react` and `diffuse_conserved` statements in rules");
            };
            let law = match c.name.as_str() {
                "transfer" => {
                    let s = self.substance(name_of(subject(c, &path)?, &path)?, &path)?;
                    let from = name_of(arg(c, "from", &path)?, &path)?;
                    let Some(&source) = self.sources.get(from) else {
                        return err(path, format!("`{from}`: emerge-ref supports transfers from sources only"));
                    };
                    if name_of(arg(c, "to", &path)?, &path)? != "self" {
                        return err(path, "a rule's transfer must go `to: self`");
                    }
                    let overflow = match opt_arg(c, "overflow") {
                        Some(o) => Some(lookup(&self.sinks, name_of(o, &path)?, &path, "sink")?),
                        None => None,
                    };
                    Law::Inflow { s, source, amount: amount(arg(c, "amount", &path)?, &path)?, overflow }
                }
                "react" => {
                    let r = lookup(&self.reactions, name_of(subject(c, &path)?, &path)?, &path, "reaction")?;
                    Law::React { reaction: r, rate_bp: rate(arg(c, "rate", &path)?, &path)? }
                }
                "diffuse_conserved" => {
                    let Expr::List(items) = subject(c, &path)? else { return err(path, "expected a list of substances") };
                    let substances = items.iter().map(|v| self.substance(name_of(expr(v)?, &path)?, &path)).collect::<Result<_, _>>()?;
                    Law::Diffuse { substances, rate_bp: rate(arg(c, "rate", &path)?, &path)? }
                }
                other => return err(path, format!("`{other}` is not supported by emerge-ref yet")),
            };
            self.world.laws.push(law);
        }
        Ok(())
    }

    fn world_genesis(&mut self, body: &ast::Block) -> Result<(), Error> {
        let path = self.path("genesis");
        for stmt in body {
            let Stmt::For { var, field, body } = &stmt.node else {
                return err(path, "emerge-ref supports `for cell in <field> { transfer(...) }` only");
            };
            if self.grid.as_deref() != Some(field.as_str()) {
                return err(path, format!("`{field}` is not the conserved field"));
            }
            let mut endow = Vec::new();
            for s in body {
                let Stmt::Call { call: c, .. } = &s.node else { return err(path, "expected `transfer(...)`") };
                if c.name != "transfer" || name_of(arg(c, "to", &path)?, &path)? != var {
                    return err(path, format!("expected `transfer(..., to: {var}, ...)`"));
                }
                let from = name_of(arg(c, "from", &path)?, &path)?;
                lookup(&self.sources, from, &path, "source")?;
                let s = self.substance(name_of(subject(c, &path)?, &path)?, &path)?;
                endow.push((s, amount(arg(c, "amount", &path)?, &path)?));
            }
            for cell in 0..self.world.cells.len() {
                for &(s, a) in &endow {
                    self.world.endow(End::Cell(cell), s, a);
                }
            }
        }
        Ok(())
    }

    fn lineage(&mut self, l: &ast::Lineage) -> Result<(), Error> {
        let mut genome = None;
        for d in &l.decls {
            match &d.node {
                LDecl::Profile { name, .. } => self.skipped.push(format!("{}.profile.{name} (capacity applies to mutation only)", l.name)),
                LDecl::Mutation(_) => self.skipped.push(format!("{}.mutation (mutation is not implemented)", l.name)),
                LDecl::Seed { signals, modules } => genome = Some(self.genome(&l.name, signals, modules)?),
                LDecl::Genesis(_) => {}
            }
        }
        let Some(genome) = genome else { return err(&l.name, "a lineage needs a seed genome") };
        for d in &l.decls {
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
            let (n, k) = &s.node;
            names.push(n.clone());
            keep.push(k.parse::<f32>().map_err(|_| Error { path: format!("{lineage}.seed.{n}"), message: "bad keep".into() })?);
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
                        if !self.senses.contains(&c.name) {
                            return err(path, format!("sense `{}` is not on the world's menu", c.name));
                        }
                        let sense = match c.name.as_str() {
                            "inventory" => Sense::Inventory(self.substance(name_of(arg(c, "s", &path)?, &path)?, &path)?),
                            "gradient" => {
                                let q = arg(c, "q", &path)?;
                                match q {
                                    Expr::Path(ast::Path { root, segs }) if Some(root.as_str()) == self.grid.as_deref() && segs.len() == 1 => {
                                        let Seg::Field(s) = &segs[0] else { return err(path, "expected `<field>.<substance>`") };
                                        Sense::Gradient(self.substance(s, &path)?)
                                    }
                                    _ => return err(path, "emerge-ref supports `gradient` of the conserved field only"),
                                }
                            }
                            other => return err(path, format!("sense `{other}` is not supported by emerge-ref yet")),
                        };
                        let want = if matches!(sense, Sense::Gradient(_)) { 2 } else { 1 };
                        if outs.len() != want {
                            return err(path, format!("`{}` writes {want} signal(s)", c.name));
                        }
                        let out = sig(&outs[0], &mut names, &mut keep);
                        let out2 = outs.get(1).map(|o| sig(o, &mut names, &mut keep));
                        Gene::Sense { sense, out, out2 }
                    }
                    GeneBody::Regulate { call: c, inputs, out } => {
                        if !self.regulation.contains(&c.name) {
                            return err(path, format!("regulation gene `{}` is not on the world's menu", c.name));
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
                                    Some(other) => return err(path, format!("unknown curve `{other}`")),
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
                                let [a, b] = &inputs[..] else { return err(path, "`product` reads two signals") };
                                if a.weight.is_some() || b.weight.is_some() {
                                    return err(path, "`product` inputs take no weights");
                                }
                                let (a, b) = (sig(&a.signal, &mut names, &mut keep), sig(&b.signal, &mut names, &mut keep));
                                Gene::Product { a, b, out: sig(out, &mut names, &mut keep) }
                            }
                            other => return err(path, format!("regulation gene `{other}` is not supported by emerge-ref yet")),
                        }
                    }
                    GeneBody::Reaction { name: r, rate: rt, source: s } => {
                        let reaction = lookup(&self.reactions, r, &path, "reaction")?;
                        let Lit::Amount(rt) = rt else { return err(path, "a reaction rate is an amount") };
                        Gene::React { reaction, rate: mu(*rt, &path)?, speed: source(s, &mut names, &mut keep)? }
                    }
                    GeneBody::Action { call: c, source: s, threshold } => {
                        let Some(def) = self.actions.get(&c.name) else {
                            return err(path, format!("action `{}` is not on the world's menu", c.name));
                        };
                        let src = source(s, &mut names, &mut keep)?;
                        let threshold = match threshold {
                            Some(t) => parse_f32(t, &path)?,
                            None => 0.0,
                        };
                        let fee = def.cost;
                        if def.per_distance != (c.name == "move") {
                            return err(path, format!("emerge-ref charges per distance for `move` only, not `{}`", c.name));
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
                                let Some(requires) = def.requires else { return err(path, "`divide` needs a `requires` condition") };
                                let split = float(arg(c, "split", &path)?, &path)?;
                                if !(0.0..=1.0).contains(&split) {
                                    return err(path, "`split` must be in 0.0..1.0");
                                }
                                // The child's share in whole basis points (DISCREPANCIES.md).
                                let split_bp = (split * 10_000.0).round() as u32;
                                Gene::Divide { split_bp, requires, fee, src, threshold }
                            }
                            other => return err(path, format!("action `{other}` is not supported by emerge-ref yet")),
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
            match &stmt.node {
                Stmt::Setting { key, value } if key == "count" => count = Some(int(value, &path)?),
                Stmt::Assign { target, value } if target.root == "position" && target.segs.is_empty() => {
                    let c = call(value, &path)?;
                    if c.name != "uniform" {
                        return err(path, "emerge-ref supports `position = uniform(x: a..b, y: c..d)` only");
                    }
                    ranges = Some((range(arg(&c, "x", &path)?, &path)?, range(arg(&c, "y", &path)?, &path)?));
                }
                Stmt::Call { call: c, .. } if c.name == "transfer" => {
                    if name_of(arg(c, "to", &path)?, &path)? != "self" {
                        return err(path, "a lineage's genesis transfer must go `to: self`");
                    }
                    lookup(&self.sources, name_of(arg(c, "from", &path)?, &path)?, &path, "source")?;
                    let s = self.substance(name_of(subject(c, &path)?, &path)?, &path)?;
                    endow.push((s, amount(arg(c, "amount", &path)?, &path)?));
                }
                _ => return err(path, "unsupported statement in a lineage's genesis"),
            }
        }
        let Some(count) = count else { return err(path, "genesis needs `count`") };
        let Some(((x0, x1), (y0, y1))) = ranges else { return err(path, "genesis needs `position`") };
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
    map.get(n).copied().ok_or_else(|| Error { path: path.into(), message: format!("unknown {what} `{n}`") })
}

fn expr(v: &Value) -> Result<&Expr, Error> {
    match v {
        Value::Expr(e) => Ok(e),
        _ => err("value", "expected an expression"),
    }
}

fn name_of<'e>(e: &'e Expr, path: &str) -> Result<&'e str, Error> {
    match e {
        Expr::Path(ast::Path { root, segs }) if segs.is_empty() => Ok(root),
        _ => err(path, "expected a name"),
    }
}

fn call(e: &Expr, path: &str) -> Result<ast::Call, Error> {
    match e {
        Expr::Call(c) => Ok(c.clone()),
        _ => err(path, "expected a call"),
    }
}

fn opt_arg<'c>(c: &'c ast::Call, key: &str) -> Option<&'c Expr> {
    c.args.iter().find(|a| a.name.as_deref() == Some(key)).and_then(|a| match &a.value {
        Value::Expr(e) => Some(e),
        _ => None,
    })
}

fn arg<'c>(c: &'c ast::Call, key: &str, path: &str) -> Result<&'c Expr, Error> {
    opt_arg(c, key).ok_or_else(|| Error { path: path.into(), message: format!("`{}` needs `{key}`", c.name) })
}

fn subject<'c>(c: &'c ast::Call, path: &str) -> Result<&'c Expr, Error> {
    match c.args.first() {
        Some(ast::Arg { name: None, value: Value::Expr(e) }) => Ok(e),
        _ => err(path, format!("`{}` needs a subject", c.name)),
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

fn parse_f32(s: &str, path: &str) -> Result<f32, Error> {
    s.parse().map_err(|_| Error { path: path.into(), message: format!("bad number `{s}`") })
}

fn float(e: &Expr, path: &str) -> Result<f32, Error> {
    match lit(e) {
        Some((neg, Lit::Float(s))) => parse_f32(s, path).map(|v| if neg { -v } else { v }),
        _ => err(path, "expected a float literal such as `1.0`"),
    }
}

fn int(e: &Expr, path: &str) -> Result<i64, Error> {
    match lit(e) {
        Some((neg, Lit::Int(i))) => Ok(if neg { -i } else { *i }),
        _ => err(path, "expected an integer"),
    }
}

fn boolean(e: &Expr, path: &str) -> Result<bool, Error> {
    match lit(e) {
        Some((false, Lit::Bool(b))) => Ok(*b),
        _ => err(path, "expected `true` or `false`"),
    }
}

fn mu(m: u64, path: &str) -> Result<Mu, Error> {
    Mu::try_from(m).map_err(|_| Error { path: path.into(), message: "amount too large".into() })
}

fn amount(e: &Expr, path: &str) -> Result<Mu, Error> {
    match lit(e) {
        Some((false, Lit::Amount(m))) => mu(*m, path),
        _ => err(path, "expected an amount such as `5u` or `250mu`"),
    }
}

fn rate(e: &Expr, path: &str) -> Result<u32, Error> {
    match lit(e) {
        Some((false, Lit::Rate(b))) => u32::try_from(*b).map_err(|_| Error { path: path.into(), message: "rate too large".into() }),
        _ => err(path, "expected a rate such as `50bp`"),
    }
}

fn range(e: &Expr, path: &str) -> Result<(f32, f32), Error> {
    match lit(e) {
        Some((false, Lit::Range(a, b))) => {
            let f = |l: &Lit| match l {
                Lit::Float(s) => parse_f32(s, path),
                _ => err(path, "expected a float range such as `0.0..512.0`"),
            };
            Ok((f(a)?, f(b)?))
        }
        _ => err(path, "expected a range"),
    }
}
