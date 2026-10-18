//! Static checks on a parsed file, reporting every problem found:
//! - chemistry (`language.md` §6.2, §6.5; spec §3.2, §4.4): every substance has a known
//!   composition, and every hand-written reaction balances its elements, runs downhill by
//!   at least `min_drop`, and is in canonical form;
//! - names (§2.3, §8.3): unique within a world, signal names distinct from the world's,
//!   generated forms reserved;
//! - seeds (§8.3, §8.1): every signal read is written, and the seed fits its profile;
//! - upkeep (§6.6): `base` is above `0mu`;
//! - writers (§6.3): a signal field written by several rules declares how writes combine.

use std::collections::HashMap;

use crate::ast::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub code: &'static str,
    /// The item's path, such as `Dish.reactions.build`.
    pub path: String,
    /// Where the item starts.
    pub pos: Pos,
    pub message: String,
    /// A suggested replacement, where there is one.
    pub fix: Option<String>,
}

pub fn check(file: &File) -> Vec<Problem> {
    let mut out = Vec::new();
    for item in &file.items {
        match &item.node {
            Item::World(w) => {
                names(w, &mut out);
                writers(w, &mut out);
                chemistry(w, &mut out);
                upkeep(w, &mut out);
            }
            Item::Lineage(l) => {
                let world = file.items.iter().find_map(|i| match &i.node {
                    Item::World(w) if w.name == l.world => Some(w),
                    _ => None,
                });
                seed(l, world, &mut out);
            }
            _ => {}
        }
    }
    out
}

/// The name a world declaration introduces, and its path segment.
fn declared(d: &WDecl) -> Option<(&str, &'static str)> {
    Some(match d {
        WDecl::Substance { name, .. } => (name, "substances"),
        WDecl::Reaction { name, .. } => (name, "reactions"),
        WDecl::Rule { name, .. } => (name, "rules"),
        WDecl::Field { name, .. } => (name, "fields"),
        WDecl::Source { name, .. } => (name, "sources"),
        WDecl::Sink { name, .. } => (name, "sinks"),
        _ => return None,
    })
}

/// E0111: a name is declared once in a world, across all kinds.
fn names(w: &World, out: &mut Vec<Problem>) {
    let mut seen: HashMap<&str, &str> = HashMap::new();
    for d in &w.decls {
        let Some((name, kind)) = declared(&d.node) else { continue };
        if let Some(first) = seen.insert(name, kind) {
            out.push(Problem {
                code: "E0111",
                path: format!("{}.{kind}.{name}", w.name),
                pos: d.pos,
                message: format!("`{name}` is already declared in {}.{first}", w.name),
                fix: None,
            });
        }
    }
}

/// Whether a block assigns `self.value`, anywhere.
fn assigns_value(block: &Block) -> bool {
    block.iter().any(|s| match &s.node {
        Stmt::Assign { target, .. } => target.root == "self" && target.segs == [Seg::Field("value".into())],
        Stmt::When { then, otherwise, .. } => assigns_value(then) || otherwise.as_ref().is_some_and(assigns_value),
        Stmt::For { body, .. } => assigns_value(body),
        _ => false,
    })
}

/// E0305: a signal field that several rules write declares how their writes combine, with
/// an operator that applies to `f32` values (core.md §3).
fn writers(w: &World, out: &mut Vec<Problem>) {
    for d in &w.decls {
        let WDecl::Field { name, holds: None, combine, .. } = &d.node else { continue };
        let path = format!("{}.fields.{name}", w.name);
        if let Some(c) = combine.as_deref().filter(|c| !["sum", "max", "min"].contains(c)) {
            let message = format!("`combine {c}` does not apply to a signal field; use `sum`, `max` or `min`");
            out.push(Problem { code: "E0305", path: path.clone(), pos: d.pos, message, fix: None });
        }
        let rules: Vec<_> = w.decls.iter().filter(|r| matches!(&r.node, WDecl::Rule { on, body, .. } if on == name && assigns_value(body))).collect();
        if combine.is_none() && rules.len() > 1 {
            let WDecl::Rule { name: second, .. } = &rules[1].node else { unreachable!() };
            out.push(Problem {
                code: "E0305",
                path: format!("{}.rules.{second}", w.name),
                pos: rules[1].pos,
                message: format!("`{name}` is already written by another rule; declare how writes combine: `field {name} : ... signal combine max;`"),
                fix: None,
            });
        }
    }
}

/// E0702: upkeep's `base` is above `0mu`, so no entity lives for free.
fn upkeep(w: &World, out: &mut Vec<Problem>) {
    for d in &w.decls {
        let WDecl::Config { name, entries } = &d.node else { continue };
        if name != "upkeep" {
            continue;
        }
        for e in entries {
            if e.node.key == "base" && e.node.value == Value::Expr(Expr::Lit(Lit::Amount(0))) {
                out.push(Problem {
                    code: "E0702",
                    path: format!("{}.upkeep.base", w.name),
                    pos: e.pos,
                    message: "`base` must be greater than `0mu`, so no entity lives for free".into(),
                    fix: None,
                });
            }
        }
    }
}

/// Signals a gene reads and writes, by name.
fn signal_use(g: &GeneBody) -> (Vec<&str>, Vec<&str>) {
    fn source(s: &Option<Source>) -> Vec<&str> {
        match s {
            Some(Source::Signal(n)) => vec![n.as_str()],
            _ => vec![],
        }
    }
    match g {
        GeneBody::Sense { outs, .. } => (vec![], outs.iter().map(String::as_str).collect()),
        GeneBody::Regulate { inputs, out, .. } => (inputs.iter().map(|i| i.signal.as_str()).collect(), vec![out.as_str()]),
        GeneBody::Reaction { source: s, .. } | GeneBody::Action { source: s, .. } => (source(s), vec![]),
    }
}

/// Generated names (`s17`, `m3`) are reserved for evolution.
fn generated(name: &str, prefix: char) -> bool {
    name.len() > 1 && name.starts_with(prefix) && name[1..].bytes().all(|b| b.is_ascii_digit())
}

/// E0112, E0113, E0605 and E0801 for a lineage's seed genome.
fn seed(l: &Lineage, world: Option<&World>, out: &mut Vec<Problem>) {
    let world_names: Vec<&str> = world.map_or(vec![], |w| w.decls.iter().filter_map(|d| declared(&d.node).map(|(n, _)| n)).collect());
    for d in &l.decls {
        let LDecl::Seed { signals, modules } = &d.node else { continue };
        let mut problem = |code, path: String, pos, message: String| out.push(Problem { code, path, pos, message, fix: None });
        let mut written: Vec<&str> = signals.iter().map(|s| s.node.0.as_str()).collect();
        let mut all: Vec<&str> = written.clone();
        for s in signals {
            let n = &s.node.0;
            let path = format!("{}.seed.{n}", l.name);
            if world_names.contains(&n.as_str()) {
                problem("E0112", path.clone(), s.pos, format!("signal `{n}` has the name of a declaration in the world"));
            }
            if generated(n, 's') {
                problem("E0113", path, s.pos, format!("`{n}` is reserved for signals that evolution creates"));
            }
        }
        let mut genes = 0;
        for m in modules {
            if generated(&m.node.name, 'm') {
                let message = format!("`{}` is reserved for modules that evolution creates", m.node.name);
                problem("E0113", format!("{}.seed.{}", l.name, m.node.name), m.pos, message);
            }
            for g in &m.node.genes {
                genes += 1;
                let (_, writes) = signal_use(&g.node.body);
                written.extend(writes);
            }
        }
        for m in modules {
            for g in &m.node.genes {
                let path = format!("{}.seed.{}{}", l.name, m.node.name, g.node.label.as_ref().map(|x| format!("@{x}")).unwrap_or_default());
                let (reads, writes) = signal_use(&g.node.body);
                for n in reads.iter().chain(&writes) {
                    if !all.contains(n) {
                        all.push(n);
                        if world_names.contains(n) {
                            problem("E0112", path.clone(), g.pos, format!("signal `{n}` has the name of a declaration in the world"));
                        }
                        if generated(n, 's') {
                            problem("E0113", path.clone(), g.pos, format!("`{n}` is reserved for signals that evolution creates"));
                        }
                    }
                }
                for n in reads {
                    if !written.contains(&n) {
                        problem("E0605", path.clone(), g.pos, format!("signal `{n}` is read but no gene writes it"));
                    }
                }
            }
        }
        // E0801: the seed fits every profile the lineage declares.
        for p in &l.decls {
            let LDecl::Profile { name, entries } = &p.node else { continue };
            let size = |key: &str| entries.iter().find_map(|e| match (&e.node.key, &e.node.value) {
                (k, Value::Expr(Expr::Lit(Lit::Int(n)))) if k == key => Some(*n),
                _ => None,
            });
            for (key, used) in [("genes", genes), ("signals", all.len()), ("modules", modules.len())] {
                if let Some(cap) = size(key).filter(|&cap| used as i64 > cap) {
                    let message = format!("the seed has {used} {key}, more than profile `{name}` allows ({cap})");
                    problem("E0801", format!("{}.seed", l.name), d.pos, message);
                }
            }
        }
    }
}

fn chemistry(w: &World, out: &mut Vec<Problem>) {
    let mut elements = Vec::new();
    let mut min_drop = 1i64;
    // name -> (declaration index, composition by element, potential)
    let mut substances: HashMap<&str, (usize, HashMap<&str, i64>, i64)> = HashMap::new();
    for d in &w.decls {
        match &d.node {
            WDecl::Element(e) => elements.push(e.as_str()),
            WDecl::Config { name, entries } if name == "second_law" => {
                for e in entries.iter().filter(|e| e.node.key == "min_drop") {
                    match &e.node.value {
                        Value::Expr(Expr::Lit(Lit::Int(m))) if *m >= 1 => min_drop = *m,
                        _ => out.push(Problem {
                            code: "E0203",
                            path: format!("{}.second_law.min_drop", w.name),
                            pos: e.pos,
                            message: "`min_drop` must be a whole number of at least 1, so every reaction loses potential".into(),
                            fix: None,
                        }),
                    }
                }
            }
            WDecl::Substance { name, comp, potential } => {
                let mut c = HashMap::new();
                for term in comp {
                    if !elements.contains(&term.name.as_str()) {
                        out.push(Problem {
                            code: "E0405",
                            path: format!("{}.substances.{name}", w.name),
                            pos: d.pos,
                            message: format!("`{}` is not a declared element", term.name),
                            fix: None,
                        });
                    }
                    *c.entry(term.name.as_str()).or_insert(0) += term.coeff.unwrap_or(1);
                }
                substances.insert(name.as_str(), (substances.len(), c, *potential));
            }
            _ => {}
        }
    }
    for d in &w.decls {
        let WDecl::Reaction { name, formula, .. } = &d.node else { continue };
        let path = format!("{}.reactions.{name}", w.name);
        let unknown: Vec<_> = formula
            .inputs
            .iter()
            .chain(&formula.outputs)
            .filter(|t| !substances.contains_key(t.name.as_str()))
            .map(|t| t.name.clone())
            .collect();
        if !unknown.is_empty() {
            let message = format!("unknown substances: {}", unknown.join(", "));
            out.push(Problem { code: "E0406", path, pos: d.pos, message, fix: None });
            continue;
        }
        let side = |terms: &[Term], f: &dyn Fn(&str) -> i64| -> i64 {
            terms.iter().map(|t| t.coeff.unwrap_or(1) * f(&t.name)).sum()
        };
        // E0401: element balance.
        for e in &elements {
            let count = |s: &str| substances[s].1.get(e).copied().unwrap_or(0);
            let (i, o) = (side(&formula.inputs, &count), side(&formula.outputs, &count));
            if i != o {
                out.push(Problem {
                    code: "E0401",
                    path: path.clone(),
                    pos: d.pos,
                    message: format!("element {e} does not balance: {i} in, {o} out"),
                    fix: None,
                });
            }
        }
        // E0402: downhill by at least min_drop.
        let potential = |s: &str| substances[s].2;
        let dg = side(&formula.outputs, &potential) - side(&formula.inputs, &potential);
        if dg > -min_drop {
            out.push(Problem {
                code: "E0402",
                path: path.clone(),
                pos: d.pos,
                message: format!("reaction `{name}` is uphill: ΔG = {dg:+}; it must be ≤ −{min_drop}"),
                fix: None,
            });
        }
        // E0404: canonical form.
        let canonical = canonical(formula, &|s| substances[s].0);
        if &canonical != formula {
            out.push(Problem {
                code: "E0404",
                path,
                pos: d.pos,
                message: format!("reaction `{name}` is not in canonical form"),
                fix: Some(crate::fmt::formula_text(&canonical)),
            });
        }
    }
}

/// Aggregate repeated terms, cancel substances on both sides, divide by the common factor,
/// and sort each side by declaration order (spec §4.4).
pub fn canonical(f: &Formula, order: &dyn Fn(&str) -> usize) -> Formula {
    let mut net: HashMap<&str, i64> = HashMap::new();
    for t in &f.inputs {
        *net.entry(&t.name).or_insert(0) -= t.coeff.unwrap_or(1);
    }
    for t in &f.outputs {
        *net.entry(&t.name).or_insert(0) += t.coeff.unwrap_or(1);
    }
    let g = net.values().filter(|c| **c != 0).fold(0, |g, c| gcd(g, c.abs()));
    let side = |sign: i64| {
        let mut terms: Vec<Term> = net
            .iter()
            .filter(|(_, c)| **c * sign > 0)
            .map(|(name, c)| {
                let k = c.abs() / g.max(1);
                Term { coeff: (k != 1).then_some(k), name: name.to_string() }
            })
            .collect();
        terms.sort_by_key(|t| order(&t.name));
        terms
    };
    Formula { inputs: side(-1), outputs: side(1) }
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a } else { gcd(b, a % b) }
}
