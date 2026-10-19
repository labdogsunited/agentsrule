//! Genome diffs at birth (`emerge_ref::diff`), for causal replay.

use emerge_ref::diff::{diff, Change};
use emerge_ref::{Gene, Genome, U};

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
    assert_eq!(diff(&parent(), &child), vec![Change::Added { at: 1, gene: Gene::take(NITRATE, U, 5) }]);
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
    child.modules[0] = false;
    assert_eq!(diff(&parent(), &child), vec![Change::Expressed { at: 1, on: false }, Change::Module { index: 0, on: false }]);
}
