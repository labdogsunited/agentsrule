//! What mutation changed between a parent's genome and its child's, for causal replay
//! (`proposals/causal-replay.md`).
//!
//! Mutation renumbers signals (a signal that loses its last user is removed and the later
//! ones shift down) and modules (duplicating, deleting, splitting or merging one shifts the
//! later ones). So genes are aligned by a longest common subsequence of genes equal up to
//! their signal numbers, and the renumbering is read off the aligned pairs: each parent
//! signal maps to the child's one its writers are most often paired with (its readers, if
//! no aligned gene writes it), and each module likewise. An aligned gene
//! whose signals do not follow that map was rewired and counts as changed. Between aligned
//! genes, unmatched genes of the same kind are paired in order as changed; the rest are
//! removed or added.

use std::collections::HashMap;

use crate::mind::{Gene, Genome};
use crate::mutate::signals_mut;

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A gene only the child has, at its slot in the child, and whether it is expressed there.
    Added { at: usize, gene: Gene, expressed: bool },
    /// A gene only the parent has, at its slot in the parent.
    Removed { at: usize, gene: Gene },
    /// A gene of the same kind with other parameters, substance or wiring.
    Changed { from_at: usize, at: usize, from: Gene, to: Gene },
    /// The same gene, now active or inactive (its own bit, or its module's, or a move to
    /// another module).
    Expressed { at: usize, on: bool },
    /// A parent module that maps to a child module with the other on/off state.
    Module { index: usize, on: bool },
    /// How many mapped signals' `keep` changed.
    Keep { signals: usize },
}

/// The gene with every signal number set to 0, and its signal numbers in order.
fn shape(g: &Gene) -> (Gene, Vec<usize>) {
    let mut g = g.clone();
    let signals = signals_mut(&mut g).into_iter().map(|s| std::mem::replace(s, 0)).collect();
    (g, signals)
}

/// The most frequent partner of each key.
fn majority(votes: HashMap<(usize, usize), usize>) -> HashMap<usize, usize> {
    let mut best: HashMap<usize, (usize, usize)> = HashMap::new();
    let mut votes: Vec<_> = votes.into_iter().collect();
    votes.sort_unstable();
    for ((a, b), n) in votes {
        let e = best.entry(a).or_insert((b, 0));
        if n > e.1 {
            *e = (b, n);
        }
    }
    best.into_iter().map(|(a, (b, _))| (a, b)).collect()
}

pub fn diff(parent: &Genome, child: &Genome) -> Vec<Change> {
    let (p, c) = (&parent.slots, &child.slots);
    let (ps, cs): (Vec<_>, Vec<_>) = (p.iter().map(|s| shape(&s.gene)).collect(), c.iter().map(|s| shape(&s.gene)).collect());
    let same = |i: usize, j: usize| ps[i].0 == cs[j].0;
    // lcs[i][j]: longest common subsequence of p[i..] and c[j..].
    let mut lcs = vec![vec![0u16; c.len() + 1]; p.len() + 1];
    for i in (0..p.len()).rev() {
        for j in (0..c.len()).rev() {
            lcs[i][j] = if same(i, j) { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    // Aligned pairs, and runs of unmatched genes between them.
    let mut steps = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < p.len() || j < c.len() {
        if i < p.len() && j < c.len() && same(i, j) {
            steps.push((Some(i), Some(j)));
            (i, j) = (i + 1, j + 1);
        } else if j < c.len() && (i == p.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            steps.push((None, Some(j)));
            j += 1;
        } else {
            steps.push((Some(i), None));
            i += 1;
        }
    }
    let (mut written, mut read, mut mod_votes) = (HashMap::new(), HashMap::new(), HashMap::new());
    for &(a, b) in &steps {
        if let (Some(a), Some(b)) = (a, b) {
            for (x, y) in p[a].gene.writes().into_iter().zip(c[b].gene.writes()) {
                *written.entry((x, y)).or_insert(0) += 1;
            }
            for (&x, &y) in ps[a].1.iter().zip(&cs[b].1) {
                *read.entry((x, y)).or_insert(0) += 1;
            }
            *mod_votes.entry((p[a].module, c[b].module)).or_insert(0) += 1;
        }
    }
    let mut sig = majority(read);
    sig.extend(majority(written));
    let module = majority(mod_votes);
    // An identical gene counts as unchanged even where the map is unsure (its writer was lost).
    let follows = |a: usize, b: usize| p[a].gene == c[b].gene || ps[a].1.iter().zip(&cs[b].1).all(|(x, y)| sig.get(x) == Some(y));

    let mut out = Vec::new();
    let (mut gone, mut new) = (Vec::new(), Vec::new());
    let flush = |gone: &mut Vec<usize>, new: &mut Vec<usize>, out: &mut Vec<Change>| {
        let kind = |g: &Gene| std::mem::discriminant(g);
        for &a in gone.iter() {
            if let Some(k) = new.iter().position(|&b| kind(&p[a].gene) == kind(&c[b].gene)) {
                let b = new.remove(k);
                out.push(Change::Changed { from_at: a, at: b, from: p[a].gene.clone(), to: c[b].gene.clone() });
            } else {
                out.push(Change::Removed { at: a, gene: p[a].gene.clone() });
            }
        }
        out.extend(new.iter().map(|&b| Change::Added { at: b, gene: c[b].gene.clone(), expressed: c[b].expressed }));
        gone.clear();
        new.clear();
    };
    for &step in &steps {
        match step {
            (Some(a), Some(b)) => {
                flush(&mut gone, &mut new, &mut out);
                if !follows(a, b) {
                    out.push(Change::Changed { from_at: a, at: b, from: p[a].gene.clone(), to: c[b].gene.clone() });
                } else if parent.active(a) != child.active(b) {
                    out.push(Change::Expressed { at: b, on: child.active(b) });
                }
            }
            (None, Some(b)) => new.push(b),
            (Some(a), None) => gone.push(a),
            (None, None) => {}
        }
    }
    flush(&mut gone, &mut new, &mut out);
    let mut flips: Vec<_> = module.iter().filter(|&(&a, &b)| parent.modules[a] != child.modules[b]).map(|(&a, &b)| (a, child.modules[b])).collect();
    flips.sort_unstable();
    out.extend(flips.into_iter().map(|(index, on)| Change::Module { index, on }));
    let keep = sig.iter().filter(|&(&a, &b)| parent.keep[a] != child.keep[b]).count();
    if keep > 0 {
        out.push(Change::Keep { signals: keep });
    }
    out
}
