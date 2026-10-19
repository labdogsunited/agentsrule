//! Genome diffs at birth (`emerge_ref::diff`), for causal replay.

use emerge_ref::diff::{diff, Change};
use emerge_ref::{Gene, Genome, Input, Sense, U};

const CARBON: usize = 4;
const NITRATE: usize = 5;

fn parent() -> Genome {
    Genome::simple(vec![Gene::take(CARBON, U, 5), Gene::react(2, U), Gene::divide(5_000, (6, 10 * U), 0)])
}

#[test]
fn an_unchanged_genome_has_no_changes() {
    assert_eq!(diff(&parent(), &parent()), vec![]);
}

#[test]
fn a_duplicated_take_whose_copy_takes_nitrate_is_one_added_gene() {
    let mut child = parent();
    let mut copy = child.slots[0].clone();
    copy.gene = Gene::take(NITRATE, U, 5);
    child.slots.insert(1, copy);
    assert_eq!(diff(&parent(), &child), vec![Change::Added { at: 1, gene: Gene::take(NITRATE, U, 5), expressed: true }]);
}

#[test]
fn a_new_substance_is_a_change_and_a_lost_gene_is_removed() {
    let mut child = parent();
    child.slots[0].gene = Gene::take(NITRATE, U, 5);
    child.slots.remove(2);
    assert_eq!(
        diff(&parent(), &child),
        vec![
            Change::Changed { from_at: 0, at: 0, from: Gene::take(CARBON, U, 5), to: Gene::take(NITRATE, U, 5) },
            Change::Removed { at: 2, gene: Gene::divide(5_000, (6, 10 * U), 0) },
        ]
    );
}

#[test]
fn expression_flips_of_genes_and_modules_are_reported() {
    let mut child = parent();
    child.slots[1].expressed = false;
    assert_eq!(diff(&parent(), &child), vec![Change::Expressed { at: 1, on: false }]);
    // A module switched off makes all its genes inactive.
    child.modules[0] = false;
    let off = (0..3).map(|at| Change::Expressed { at, on: false });
    assert_eq!(diff(&parent(), &child), off.chain([Change::Module { index: 0, on: false }]).collect::<Vec<_>>());
}

/// Senses of substances 1, 2, 3 writing s0, s1, s2, and `take`s of 1, 2, 3 reading them.
fn wired() -> Genome {
    let sense = |out| Gene::Sense { sense: Sense::Inventory(out + 1), out, out2: None };
    let take = |s| Gene::Take { s: s + 1, max: U, fee: 5, src: Input::Signal(s), threshold: 0.0 };
    Genome::simple(vec![sense(0), sense(1), sense(2), take(0), take(1), take(2)])
}

#[test]
fn removing_a_signal_renumbers_the_later_ones_without_reporting_them_as_changed() {
    // The child lost the sense and the take of s0; s1 and s2 became s0 and s1.
    let parent = wired();
    let mut child = wired();
    child.slots.remove(3);
    child.slots.remove(0);
    for slot in &mut child.slots {
        match &mut slot.gene {
            Gene::Sense { out, .. } => *out -= 1,
            Gene::Take { src: Input::Signal(s), .. } => *s -= 1,
            _ => {}
        }
    }
    child.keep.pop();
    let removed = diff(&parent, &child);
    assert_eq!(removed, vec![Change::Removed { at: 0, gene: parent.slots[0].gene.clone() }, Change::Removed { at: 3, gene: parent.slots[3].gene.clone() }]);
}

#[test]
fn a_rewired_gene_is_changed() {
    let parent = wired();
    let mut child = wired();
    child.slots[5].gene = Gene::Take { s: 3, max: U, fee: 5, src: Input::Signal(0), threshold: 0.0 };
    assert_eq!(diff(&parent, &child), vec![Change::Changed { from_at: 5, at: 5, from: parent.slots[5].gene.clone(), to: child.slots[5].gene.clone() }]);
}

#[test]
fn a_dormant_copy_of_a_module_is_added_genes_not_a_module_flip() {
    // Modules [on, on]; module 0 is duplicated, dormant, after itself: [on, off, on].
    let mut parent = parent();
    parent.modules.push(true);
    parent.slots[2].module = 1;
    let mut child = parent.clone();
    child.modules = vec![true, false, true];
    child.slots[2].module = 2;
    for k in 0..2 {
        let mut copy = child.slots[k].clone();
        copy.module = 1;
        child.slots.insert(2 + k, copy);
    }
    let changes = diff(&parent, &child);
    assert!(changes.iter().all(|c| matches!(c, Change::Added { .. })), "{changes:?}");
    assert_eq!(changes.len(), 2);
}
