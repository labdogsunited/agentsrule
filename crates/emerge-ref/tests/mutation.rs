//! Variation at division (spec §8.1) and the generated reaction space (§4.3), including
//! exit tests 4 (the reaction space) and 5 (growth operators) of spec §16.

use emerge_ref::compile::{compile, Options};
use emerge_ref::mind::{Curve, Gene, Genome, Input, Sense, Slot};
use emerge_ref::mutate::{mutate, validate, Capacity, Draws, Menus, Rates};
use emerge_ref::philox::{self, Stream};
use emerge_ref::World;

const DISH: &str = include_str!("../../../examples/dish.em");

fn dish(lineage: &str, seed: u64, mutation: bool) -> World {
    compile(DISH, &[lineage], Options { seed, allow_partial: true, mutation }).unwrap_or_else(|e| panic!("{e}")).world
}

fn draws(n: u32) -> Draws {
    Draws::new(Stream { seed: 9, tick: n, stream: n, domain: philox::DOMAIN_ENTITY, aux: 0, pass: philox::MUTATE })
}

fn rates(x: f32) -> Rates {
    Rates {
        rate: x,
        param_sigma: 0.3,
        expression_flip: x,
        rewire: x,
        diverge: x,
        duplicate_gene: x,
        insert_gene: x,
        delete_gene: x,
        duplicate_module: x,
        delete_module: x,
        split_module: x,
        merge_modules: x,
    }
}

/// Mutates Drifter's seed over many generations with high rates, and checks every genome.
fn random_walk(cap: Capacity, generations: u32) -> (usize, usize) {
    let w = dish("Drifter", 1, true);
    let menus = w.menus.clone();
    let seed = w.entities[0].genome.clone();
    let (mut genome, mut max_genes, mut max_modules) = (seed.clone(), 0, 0);
    let r = rates(0.05);
    for n in 0..generations {
        genome = mutate(&genome, &r, &cap, &menus, &mut draws(n));
        if let Err(e) = validate(&genome, &cap, &menus) {
            panic!("generation {n}: {e}\n{genome:?}");
        }
        max_genes = max_genes.max(genome.slots.len());
        max_modules = max_modules.max(genome.modules.len());
        if genome.slots.is_empty() || n % 10_000 == 0 {
            genome = seed.clone();
        }
    }
    (max_genes, max_modules)
}

#[test]
fn a_million_mutations_give_only_valid_genomes_in_both_profiles() {
    // Exit test 5. Debug builds run a tenth of it.
    let n = if cfg!(debug_assertions) { 50_000 } else { 500_000 };
    let fast = Capacity { genes: 32, signals: 16, modules: 8 };
    let free = Capacity { genes: 256, signals: 64, modules: 32 };
    let (genes, modules) = random_walk(fast, n);
    assert!(genes == 32 && modules == 8, "growth should reach the fast capacity: {genes} genes, {modules} modules");
    let (genes, modules) = random_walk(free, n);
    assert!(genes > 32 && modules > 8, "growth should pass the fast capacity in free: {genes} genes, {modules} modules");
}

fn slot(gene: Gene, module: usize) -> Slot {
    Slot { gene, expressed: true, module }
}

#[test]
fn a_duplicated_module_gets_fresh_private_signals_and_shares_its_interface() {
    let w = dish("Drifter", 1, true);
    // Module 0 senses into s0 (also read by module 1) and computes s1 (read only inside).
    let weigh = |input: usize, out: usize| Gene::Weigh { bias: 0.0, curve: Curve::Linear, inputs: vec![(1.0, input)], out };
    let genome = Genome {
        slots: vec![
            slot(Gene::Sense { sense: Sense::Inventory(1), out: 0, out2: None }, 0),
            slot(weigh(0, 1), 0),
            slot(Gene::Move { speed: 1.0, fee_per_distance: 20, src: Input::Signal(1), threshold: 0.0 }, 0),
            slot(weigh(0, 2), 1),
            slot(Gene::Turn { angle_deg: 30.0, fee: 10, src: Input::Signal(2), threshold: -2.0 }, 1),
        ],
        modules: vec![true, true],
        keep: vec![0.0; 3],
    };
    let only = Rates { duplicate_module: 1.0, ..Rates::default() };
    let cap = Capacity { genes: 32, signals: 16, modules: 8 };
    let child = mutate(&genome, &only, &cap, &w.menus, &mut draws(1));
    validate(&child, &cap, &w.menus).unwrap();
    // Both modules were copied, each right after itself, and the copies are dormant.
    assert_eq!(child.modules, [true, false, true, false]);
    let genes = |m: usize| child.slots.iter().filter(|s| s.module == m).map(|s| s.gene.clone()).collect::<Vec<_>>();
    let (a, copy) = (genes(0), genes(1));
    let (Gene::Sense { out: a_sense, .. }, Gene::Sense { out: c_sense, .. }) = (&a[0], &copy[0]) else { panic!() };
    assert_eq!(a_sense, c_sense, "s0 is read outside the module, so the copy shares it");
    let (Gene::Weigh { out: a_out, .. }, Gene::Weigh { out: c_out, .. }) = (&a[1], &copy[1]) else { panic!() };
    assert_ne!(a_out, c_out, "s1 is private, so the copy writes a fresh signal");
    assert!(matches!(copy[2], Gene::Move { src: Input::Signal(s), .. } if s == *c_out), "the copy reads its own fresh signal");
}

#[test]
fn duplicated_genes_start_dormant() {
    let w = dish("Drifter", 1, true);
    let seed = w.entities[0].genome.clone();
    let only = Rates { duplicate_gene: 1.0, ..Rates::default() };
    let cap = Capacity { genes: 32, signals: 16, modules: 8 };
    let child = mutate(&seed, &only, &cap, &w.menus, &mut draws(2));
    assert_eq!(child.slots.len(), 2 * seed.slots.len());
    assert_eq!(child.active_genes(), seed.active_genes(), "copies cost nothing until switched on");
    assert!(child.slots.iter().skip(1).step_by(2).all(|s| !s.expressed));
}

#[test]
fn mutation_is_deterministic_and_changes_children() {
    let state = |w: &World| format!("{:?}", w.entities.iter().map(|e| (&e.genome, e.store.amounts)).collect::<Vec<_>>());
    let run = |mutation| {
        let mut w = dish("Drifter", 3, mutation);
        for _ in 0..200 {
            w.step().unwrap();
        }
        w
    };
    let (a, b, off) = (run(true), run(true), run(false));
    assert_eq!(state(&a), state(&b), "same seed, same run");
    let seed = &off.entities[0].genome;
    assert!(off.entities.iter().all(|e| &e.genome == seed), "with mutation off, every child is a copy");
    assert!(a.entities.iter().any(|e| &e.genome != seed), "with mutation on, some children differ");
}

#[test]
fn the_generated_reaction_space_is_balanced_downhill_and_unique() {
    // Exit test 4: no reaction with ΔG > −1 and no duplicate canonical forms.
    let w = dish("Drifter", 1, true);
    let space: &Menus = &w.menus;
    assert_eq!(space.reactions.len(), 96);
    let mut forms = std::collections::HashSet::new();
    for &(r, max) in &space.reactions {
        let rx = &w.chem.reactions[r];
        assert!(w.chem.delta_g(rx) <= -1, "{} is not downhill", rx.name);
        assert!(forms.insert((rx.inputs.clone(), rx.outputs.clone())), "{} appears twice", rx.name);
        let included = r < 4;
        assert_eq!(max, if included { 1_000 } else { 250 }, "{}", rx.name);
    }
    // `exclude` removes a generated reaction by its formula, in any spelling.
    let src = DISH.replace("exclude: [];", "exclude: [2 light -> 2 energy];");
    let w = compile(&src, &["Drifter"], Options { seed: 1, allow_partial: true, mutation: true }).unwrap().world;
    let names: Vec<_> = w.menus.reactions.iter().map(|&(r, _)| w.chem.reactions[r].name.as_str()).collect();
    assert_eq!(names.len(), 95);
    assert!(!names.contains(&"light -> energy"));
}

#[test]
fn a_lineage_with_mutation_needs_a_profile() {
    let src = DISH.replacen("    profile fast {\n        genes: 32;\n        signals: 16;\n        modules: 8;\n    }\n\n", "", 1);
    let e = compile(&src, &["Drifter"], Options { seed: 1, allow_partial: true, mutation: true }).err().unwrap();
    assert_eq!(e.code, "E0116");
    assert!(compile(&src, &["Drifter"], Options { seed: 1, allow_partial: true, mutation: false }).is_ok());
}
