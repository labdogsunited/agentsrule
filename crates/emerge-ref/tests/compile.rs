//! Compiling `examples/dish.em`.
//!
//! The fingerprints were recorded from a hand translation of `dish.em` into Rust, after a
//! test showed the compiled worlds matched it exactly at genesis and over 300 ticks. They
//! change only if the world, the compiler or the runner's behaviour changes.

use emerge_ref::compile::compile;
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
    let mut w = compile(DISH, &[lineage], 7).unwrap_or_else(|e| panic!("{e}")).world;
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
fn what_the_runner_cannot_use_is_listed() {
    let skipped = compile(DISH, &["Drifter"], 1).unwrap().skipped;
    for s in ["Dish.fields.scent", "Dish.rules.smell", "Dish.reaction_space", "Drifter.mutation", "ruleset Tinker", "view Main", "chart Health"] {
        assert!(skipped.iter().any(|x| x.starts_with(s)), "{s} not in {skipped:?}");
    }
}

#[test]
fn unsupported_or_unknown_things_are_errors() {
    let src = DISH.replace("action take(s: light, max: 1u) @eat_light;", "action emit(sig: scent, max: 1.0) @eat_light;");
    let e = compile(&src, &["Drifter"], 1).err().expect("emit is not supported yet");
    assert!(e.message.contains("emit"), "{e}");
    assert_eq!(e.path, "Drifter.seed.metabolism@eat_light");

    let src = DISH.replace("sense inventory(s: body) -> fullness", "sense inventory(s: gold) -> fullness");
    let e = compile(&src, &["Drifter"], 1).err().unwrap();
    assert_eq!(e.message, "unknown substance `gold`");

    let src = DISH.replace("reaction rot : body -> carbon + nitrate;", "reaction rot : carbon + nitrate -> body;");
    let e = compile(&src, &["Drifter"], 1).err().unwrap();
    assert!(e.message.starts_with("E0402"), "{e}");

    let e = compile(DISH, &["Nobody"], 1).err().unwrap();
    assert_eq!(e.message, "no such lineage");
}
