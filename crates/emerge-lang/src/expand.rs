//! `lineage B extends A in W { ... }`: the full lineage, worked out before checking
//! (`language.md` §9). `format` never calls this, so `fmt` keeps the `extends` form.
//!
//! `B` starts as a copy of `A` (one level only; `A` must not extend anything and must
//! be in the same world). Each part of `B` then merges into it:
//!
//! - `profile NAME`, `mutation`: `B`'s entries replace the entries of the same key and
//!   add the rest; a profile `A` lacks is added.
//! - `seed genome`: `B`'s signals are added (no repeats). A module of the same name gets
//!   `B`'s genes; a new module is added at the end. A gene goes after the gene named by
//!   its `after @label`, or at the end of the module without one.
//! - `genesis`: replaces `A`'s.

use crate::ast::*;
use crate::check::Problem;

fn problem(code: &'static str, path: &str, pos: Pos, message: String) -> Problem {
    Problem { code, path: path.to_string(), pos, message, fix: None }
}

/// The file with every `extends` worked out, or the first problem found.
pub fn expand(file: &File) -> Result<File, Problem> {
    let mut out = file.clone();
    for (n, item) in file.items.iter().enumerate() {
        let Item::Lineage(l) = &item.node else { continue };
        let path = l.name.as_str();
        let Some(base) = &l.base else {
            if let Some(g) = after_in(&l.decls) {
                return Err(problem("E0118", path, g, "`after` is only allowed in a lineage that `extends` another".into()));
            }
            continue;
        };
        let found = file.items.iter().find_map(|i| match &i.node {
            Item::Lineage(b) if &b.name == base => Some(b),
            _ => None,
        });
        let Some(b) = found else {
            return Err(problem("E0114", path, item.pos, format!("lineage `{base}` is not declared")));
        };
        if b.world != l.world {
            return Err(problem("E0117", path, item.pos, format!("`{base}` is in world `{}`, not `{}`", b.world, l.world)));
        }
        if b.base.is_some() {
            return Err(problem("E0117", path, item.pos, format!("`{base}` extends another lineage; `extends` goes one level only")));
        }
        out.items[n].node = Item::Lineage(Lineage { name: l.name.clone(), base: None, world: l.world.clone(), decls: merge(l, b)? });
    }
    Ok(out)
}

fn after_in(decls: &[Node<LDecl>]) -> Option<Pos> {
    decls.iter().find_map(|d| match &d.node {
        LDecl::Seed { modules, .. } => modules.iter().flat_map(|m| &m.node.genes).find(|g| g.node.after.is_some()).map(|g| g.pos),
        _ => None,
    })
}

fn merge(child: &Lineage, base: &Lineage) -> Result<Vec<Node<LDecl>>, Problem> {
    let path = child.name.as_str();
    let mut decls = base.decls.clone();
    for d in &child.decls {
        match &d.node {
            LDecl::Profile { name, entries } => {
                let same = decls.iter_mut().find_map(|x| match &mut x.node {
                    LDecl::Profile { name: n, entries: e } if n == name => Some(e),
                    _ => None,
                });
                match same {
                    Some(e) => entries_into(e, entries),
                    None => decls.push(d.clone()),
                }
            }
            LDecl::Mutation(entries) => {
                let same = decls.iter_mut().find_map(|x| match &mut x.node {
                    LDecl::Mutation(e) => Some(e),
                    _ => None,
                });
                match same {
                    Some(e) => entries_into(e, entries),
                    None => decls.push(d.clone()),
                }
            }
            LDecl::Genesis(_) => match decls.iter_mut().find(|x| matches!(x.node, LDecl::Genesis(_))) {
                Some(x) => *x = d.clone(),
                None => decls.push(d.clone()),
            },
            LDecl::Seed { signals, modules } => {
                let same = decls.iter_mut().find_map(|x| match &mut x.node {
                    LDecl::Seed { signals: s, modules: m } => Some((s, m)),
                    _ => None,
                });
                match same {
                    Some((s, m)) => seed_into(path, s, m, signals, modules)?,
                    None => {
                        let mut copy = d.clone();
                        if let LDecl::Seed { modules, .. } = &mut copy.node {
                            let mut none = Vec::new();
                            let mut mods: Vec<Node<Module>> = Vec::new();
                            seed_into(path, &mut none, &mut mods, &[], modules)?;
                            *modules = mods;
                        }
                        decls.push(copy);
                    }
                }
            }
        }
    }
    Ok(decls)
}

fn entries_into(into: &mut Vec<Node<Entry>>, from: &[Node<Entry>]) {
    for e in from {
        match into.iter_mut().find(|x| x.node.key == e.node.key) {
            Some(x) => *x = e.clone(),
            None => into.push(e.clone()),
        }
    }
}

fn labels(modules: &[Node<Module>]) -> Vec<&str> {
    modules.iter().flat_map(|m| &m.node.genes).filter_map(|g| g.node.label.as_deref()).collect()
}

fn seed_into(
    path: &str,
    signals: &mut Vec<Node<(String, String)>>,
    modules: &mut Vec<Node<Module>>,
    add_signals: &[Node<(String, String)>],
    add_modules: &[Node<Module>],
) -> Result<(), Problem> {
    for s in add_signals {
        if signals.iter().any(|x| x.node.0 == s.node.0) {
            return Err(problem("E0118", path, s.pos, format!("signal `{}` is already in the seed genome it extends", s.node.0)));
        }
        signals.push(s.clone());
    }
    for m in add_modules {
        let at = match modules.iter().position(|x| x.node.name == m.node.name) {
            Some(at) => {
                if modules[at].node.dormant != m.node.dormant {
                    return Err(problem("E0118", path, m.pos, format!("module `{}` is declared dormant differently from the one it extends", m.node.name)));
                }
                at
            }
            None => {
                modules.push(Node { trivia: m.trivia.clone(), pos: m.pos, node: Module { dormant: m.node.dormant, name: m.node.name.clone(), genes: Vec::new() } });
                modules.len() - 1
            }
        };
        for g in &m.node.genes {
            if let Some(l) = &g.node.label {
                if labels(modules).contains(&l.as_str()) {
                    return Err(problem("E0118", path, g.pos, format!("label `@{l}` is already used in the seed genome it extends")));
                }
            }
            let genes = &mut modules[at].node.genes;
            let place = match &g.node.after {
                None => genes.len(),
                Some(a) => match genes.iter().position(|x| x.node.label.as_deref() == Some(a.as_str())) {
                    Some(k) => k + 1,
                    None => return Err(problem("E0114", path, g.pos, format!("no gene `@{a}` in module `{}` to go after", m.node.name))),
                },
            };
            let mut gene = g.clone();
            gene.node.after = None;
            genes.insert(place, gene);
        }
    }
    Ok(())
}
