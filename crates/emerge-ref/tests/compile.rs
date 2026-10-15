//! Compiling `examples/dish.em`.
//!
//! The fingerprints were recorded from a hand translation of `dish.em` into Rust, after a
//! test showed the compiled worlds matched it exactly at genesis and over 300 ticks. They
//! change only if the world, the compiler or the runner's behaviour changes.

use emerge_ref::compile::{compile, Options};
use emerge_ref::World;

const DISH: &str = include_str!("../../../examples/dish.em");

/// Everything a run can change, in a comparable form.
fn state(w: &World) -> String {
    let entities: Vec<_> = w
        .entities
        .iter()
        .map(|e| (e.id, e.cell, e.pos.map(f32::to_bits), e.heading.map(f32::to_bits), e.store.amounts, e.age, &e.genome))
        .collect();
    let cells: Vec<_> = w.cells.iter().map(|c| (c.amounts, c.caps)).collect();
    format!("{:?} {:?} {:?} {:?} {} {}", entities, cells, w.emitted, w.absorbed, w.births, w.deaths)
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

fn after_100_ticks(lineage: &str) -> (usize, u64, u64, u64) {
    let mut w = compile(DISH, &[lineage], Options { seed: 7, allow_partial: true, mutation: false }).unwrap_or_else(|e| panic!("{e}")).world;
    for _ in 0..100 {
        w.step().unwrap();
    }
    (w.entities.len(), w.births, w.deaths, fnv(&state(&w)))
}

#[test]
fn drifter_runs_as_recorded() {
    assert_eq!(after_100_ticks("Drifter"), (8_668, 6_883, 215, 0xc47e_3b24_dbc8_04c3));
}

#[test]
fn builder_runs_as_recorded() {
    assert_eq!(after_100_ticks("Builder"), (9_200, 15_354, 8_154, 0x2128_14be_3d20_aff6));
}

#[test]
fn a_partial_run_must_be_allowed() {
    let e = compile(DISH, &["Drifter"], Options { seed: 1, allow_partial: false, mutation: false }).err().expect("scent and smell are not run");
    assert!(e.message.contains("signal field `scent`") && e.message.contains("rule `smell`") && e.message.contains("action `emit`"), "{e}");

    let c = compile(DISH, &["Drifter"], Options { seed: 1, allow_partial: true, mutation: false }).unwrap();
    let partial: Vec<_> = c.partial.iter().map(|e| (e.code, e.path.as_str(), e.pos.line)).collect();
    let expected = [
        ("W1001", "Dish.fields.scent", line_of("field scent")),
        ("W1001", "Dish.actions.emit", line_of("action emit")),
        ("W1001", "Dish.rules.smell", line_of("rule smell")),
    ];
    assert_eq!(partial, expected);
    for s in ["Dish.reaction_space", "Drifter.mutation", "ruleset Tinker", "view Main", "chart Health"] {
        assert!(c.notes.iter().any(|x| x.starts_with(s)), "{s} not in {:?}", c.notes);
    }
}

fn line_of(needle: &str) -> usize {
    DISH.lines().position(|l| l.contains(needle)).expect("needle") + 1
}

/// Compiles `dish.em` with its first `from` (in `Drifter` where both lineages have it)
/// changed to `to`, and returns the error.
fn rejected(from: &str, to: &str) -> emerge_ref::compile::Error {
    assert!(DISH.contains(from), "`{from}` is not in dish.em");
    compile(&DISH.replacen(from, to, 1), &["Drifter"], Options { seed: 1, allow_partial: true, mutation: false }).err().unwrap_or_else(|| panic!("`{to}` was accepted"))
}

#[test]
fn gene_arguments_must_be_on_the_menu_and_in_range() {
    let e = rejected("action move(speed: 1.0) @go;", "action move(speed: 99.0) @go;");
    assert_eq!((e.path.as_str(), e.message.as_str()), ("Drifter.seed.forage@go", "`speed: 99.0` is outside the menu's range 0.0..2.0"));
    assert_eq!((e.code, e.pos.line, e.pos.col), ("E0601", line_of("action move(speed: 1.0) @go;"), 13));
    let e = rejected("action take(s: light, max: 1u)", "action take(s: light, max: 11u)");
    assert!(e.message.contains("outside the menu's range"), "{e}");
    let e = rejected("action move(speed: 1.0) @go;", "action move(speed: 1.0, sped: 1.0) @go;");
    assert_eq!(e.message, "`move` has no parameter `sped` on the world's menu");
    let e = rejected("regulate weigh(curve: tanh)", "regulate weigh(curve: cubic)");
    assert!(e.message.contains("is not one of the menu's choices"), "{e}");
    let e = rejected("[10.0 * food_y] -> steer", "[20.0 * food_y] -> steer");
    assert!(e.message.contains("weight 20.0") && e.message.contains("outside the menu's range"), "{e}");
    let e = rejected("sense gradient(q: cells.body)", "sense gradient(q: cells.heat)");
    assert_eq!(e.message, "field `cells` cannot hold `heat`");
    let e = rejected("reaction respiration rate 1u @respiration;", "reaction respiration rate 2u @respiration;");
    assert!(e.message.contains("above the reaction space's `max_rate`"), "{e}");
    let e = rejected("max: 0u..10u) cost 5mu;", "max: 0u..20_000u) cost 5mu;");
    assert!(e.message.contains("above 2^24"), "{e}");
}

#[test]
fn rules_must_name_declared_fields_sources_and_sinks() {
    let e = rejected("rule sunlight on cells", "rule sunlight on typo");
    assert_eq!((e.path.as_str(), e.message.as_str()), ("Dish.rules.sunlight", "unknown field `typo`"));
    assert_eq!((e.code, e.pos.line), ("E0114", line_of("rule sunlight on cells")));
    let e = rejected("transfer(light, from: sun,", "transfer(light, from: rock,");
    assert_eq!(e.message, "source `rock` does not emit `light`");
    let e = rejected("amount: 40mu, overflow: void)", "amount: 40mu, overflw: void)");
    assert_eq!(e.message, "`transfer` takes no argument `overflw`");
    let e = rejected("react(rot, rate: 50bp)", "react(rot, rate: 20_000bp)");
    assert!(e.message.contains("more than the whole amount"), "{e}");
    let e = rejected("sink void : any;", "sink void : [heat];");
    assert!(e.message.contains("`void` must accept every substance"), "{e}");
}

#[test]
fn genesis_must_draw_from_sources_that_emit_and_fit_caps() {
    let e = rejected("transfer(energy, from: spark, to: self, amount: 5u)", "transfer(energy, from: rock, to: self, amount: 5u)");
    assert_eq!((e.path.as_str(), e.message.as_str()), ("Drifter.genesis", "source `rock` does not emit `energy`"));
    assert_eq!((e.code, e.pos.line), ("E0703", line_of("transfer(energy, from: spark")));
    let e = rejected("transfer(energy, from: spark, to: self, amount: 5u)", "transfer(energy, from: spark, to: self, amount: 500u)");
    assert!(e.message.contains("above the inventory cap"), "{e}");
    let e = rejected("transfer(body, from: rock, to: cell, amount: 1u)", "transfer(body, from: rock, to: cell, amount: 5_000u)");
    assert!(e.message.contains("does not fit in a cell"), "{e}");
    let e = rejected("position = uniform(x: 0.0..512.0", "position = uniform(x: 0.0..600.0");
    assert!(e.message.contains("within the space"), "{e}");
}

#[test]
fn unsupported_or_unknown_things_are_errors() {
    let src = DISH.replace("action take(s: light, max: 1u) @eat_light;", "action emit(sig: scent, max: 1.0) @eat_light;");
    let e = compile(&src, &["Drifter"], Options { seed: 1, allow_partial: true, mutation: false }).err().expect("emit is not supported yet");
    assert!(e.message.contains("emit"), "{e}");
    assert_eq!(e.path, "Drifter.seed.metabolism@eat_light");

    let src = DISH.replace("sense inventory(s: body) -> fullness", "sense inventory(s: gold) -> fullness");
    let e = compile(&src, &["Drifter"], Options { seed: 1, allow_partial: true, mutation: false }).err().unwrap();
    assert_eq!(e.message, "unknown substance `gold`");

    let src = DISH.replace("reaction rot : body -> carbon + nitrate;", "reaction rot : carbon + nitrate -> body;");
    let e = compile(&src, &["Drifter"], Options { seed: 1, allow_partial: true, mutation: false }).err().unwrap();
    assert_eq!((e.code, e.pos.line), ("E0402", line_of("reaction rot :")), "{e}");

    let e = compile(DISH, &["Nobody"], Options { seed: 1, allow_partial: true, mutation: false }).err().unwrap();
    assert_eq!(e.message, "no such lineage");
}

#[test]
fn entities_count_what_their_own_reactions_make() {
    let mut w = compile(DISH, &["Builder"], Options { seed: 7, allow_partial: true, mutation: false }).unwrap().world;
    let body = w.chem.substances.iter().position(|s| s.name == "body").unwrap();
    for _ in 0..5 {
        w.step().unwrap();
    }
    assert!(w.entities.iter().all(|e| e.made == 0), "nothing is counted without a tracked substance");
    w.track = Some(body);
    let before = w.entity_extent.clone();
    w.step().unwrap();
    // Each µ of body is counted by the entity that made it. Entities that reached
    // `max_age` this tick made some and were removed, so the survivors count at most that.
    let made: u64 = w.entities.iter().map(|e| e.made).sum();
    let produced: u64 = w
        .chem
        .reactions
        .iter()
        .enumerate()
        .map(|(r, rx)| rx.outputs.iter().filter(|o| o.0 == body).map(|o| o.1 as u64).sum::<u64>() * (w.entity_extent[r] - before[r]))
        .sum();
    assert!(made > 0 && made <= produced, "made {made}, produced {produced}");
}
