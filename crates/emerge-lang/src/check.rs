//! Static checks on a parsed world. So far: chemistry (`language.md` §6.2, §6.5; spec
//! §3.2, §4.4): every substance has a known composition, and every hand-written reaction
//! balances its elements, runs downhill by at least `min_drop`, and is in canonical form.

use std::collections::HashMap;

use crate::ast::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub code: &'static str,
    /// The item's path, such as `Dish.reactions.build`.
    pub path: String,
    pub message: String,
    /// A suggested replacement, where there is one.
    pub fix: Option<String>,
}

pub fn check(file: &File) -> Vec<Problem> {
    let mut out = Vec::new();
    for item in &file.items {
        if let Item::World(w) = &item.node {
            chemistry(w, &mut out);
        }
    }
    out
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
                for e in entries {
                    if let (true, Value::Expr(Expr::Lit(Lit::Int(m)))) = (e.node.key == "min_drop", &e.node.value) {
                        min_drop = *m;
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
            out.push(Problem { code: "E0406", path, message: format!("unknown substances: {}", unknown.join(", ")), fix: None });
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
