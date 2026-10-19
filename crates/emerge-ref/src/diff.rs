//! What mutation changed between a parent's genome and its child's, for causal replay
//! (`proposals/causal-replay.md`). Genes are aligned by a longest common subsequence of
//! equal genes; between aligned genes, unmatched genes of the same kind are paired in order
//! as changed, and the rest are removed or added. Module numbers are ignored, because
//! deleting or splitting a module renumbers the later ones; module flips are reported.

use crate::mind::{Gene, Genome};

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A gene only the child has, at its slot in the child.
    Added { at: usize, gene: Gene },
    /// A gene only the parent has, at its slot in the parent.
    Removed { at: usize, gene: Gene },
    /// A gene of the same kind with other parameters, substance or wiring.
    Changed { from_at: usize, at: usize, from: Gene, to: Gene },
    /// The same gene, switched on or off.
    Expressed { at: usize, on: bool },
    /// A module present in both, switched on or off.
    Module { index: usize, on: bool },
    /// How many signals' `keep` changed.
    Keep { signals: usize },
}

pub fn diff(parent: &Genome, child: &Genome) -> Vec<Change> {
    let (p, c) = (&parent.slots, &child.slots);
    let same = |i: usize, j: usize| p[i].gene == c[j].gene;
    // lcs[i][j]: longest common subsequence of p[i..] and c[j..].
    let mut lcs = vec![vec![0u16; c.len() + 1]; p.len() + 1];
    for i in (0..p.len()).rev() {
        for j in (0..c.len()).rev() {
            lcs[i][j] = if same(i, j) { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
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
        out.extend(new.iter().map(|&b| Change::Added { at: b, gene: c[b].gene.clone() }));
        gone.clear();
        new.clear();
    };
    while i < p.len() || j < c.len() {
        if i < p.len() && j < c.len() && same(i, j) {
            flush(&mut gone, &mut new, &mut out);
            if p[i].expressed != c[j].expressed {
                out.push(Change::Expressed { at: j, on: c[j].expressed });
            }
            (i, j) = (i + 1, j + 1);
        } else if j < c.len() && (i == p.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            new.push(j);
            j += 1;
        } else {
            gone.push(i);
            i += 1;
        }
    }
    flush(&mut gone, &mut new, &mut out);
    for (m, (&a, &b)) in parent.modules.iter().zip(&child.modules).enumerate() {
        if a != b {
            out.push(Change::Module { index: m, on: b });
        }
    }
    let keep = parent.keep.iter().zip(&child.keep).filter(|(a, b)| a != b).count();
    if keep > 0 {
        out.push(Change::Keep { signals: keep });
    }
    out
}
