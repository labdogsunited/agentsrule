//! Catalysts (spec §4.5): a substance that raises a reaction's cap where it is present,
//! without being consumed.

mod common;

use common::{dish, dish_cell};
use emerge_ref::chem::Catalyst;
use emerge_ref::compile::{compile, Options};
use emerge_ref::{End, Gene, Law, U};

const DISH: &str = include_str!("../../../examples/dish.em");

#[test]
fn the_cap_grows_with_the_amount_present() {
    let k = Catalyst { s: 0, half: U, boost: 4.0 };
    assert_eq!(k.scale(U, 0), U);
    assert_eq!(k.scale(U, U), 2_500);
    assert_eq!(k.scale(U, 3 * U), 3_250);
    assert!(k.scale(U, u32::MAX) < 4 * U);
    assert_eq!(Catalyst { s: 0, half: U, boost: 1.0 }.scale(777, 5 * U), 777);
}

/// One entity with 10u of sugar and `nitrate` µ of nitrate, running respiration at 1u for one tick.
fn respired(catalyst: Option<Catalyst>, nitrate: u32) -> (u32, u32) {
    let (mut w, d) = dish(0, 0);
    w.chem.reactions[d.respiration].catalyst = catalyst;
    let c = dish_cell(&mut w, &d);
    let e = w.spawn(c, vec![Gene::react(d.respiration, U)]);
    w.endow(End::Entity(e), d.sugar, 10 * U);
    w.endow(End::Entity(e), d.nitrate, nitrate);
    w.step().unwrap();
    let e = &w.entities[0];
    (10 * U - e.store.amounts[d.sugar], e.store.amounts[d.nitrate])
}

#[test]
fn an_entity_reaction_runs_faster_and_keeps_its_catalyst() {
    let k = |s| Some(Catalyst { s, half: U, boost: 4.0 });
    let (_, d) = dish(0, 0);
    assert_eq!(respired(None, U), (U, U));
    assert_eq!(respired(k(d.nitrate), 0), (U, 0), "without the catalyst the reaction runs at its normal rate");
    assert_eq!(respired(k(d.nitrate), U), (2_500, U), "the catalyst is not consumed");
}

#[test]
fn a_world_reaction_rate_is_scaled_too() {
    let rotted = |catalyst| {
        let (mut w, d) = dish(0, 0);
        w.chem.reactions[d.rot].catalyst = catalyst;
        let c = dish_cell(&mut w, &d);
        w.endow(End::Cell(c), d.body, 100 * U);
        w.endow(End::Cell(c), d.sugar, U);
        w.laws.push(Law::React { reaction: d.rot, rate_bp: 100 });
        w.step().unwrap();
        100 * U - w.cells[0].amounts[d.body]
    };
    let (_, d) = dish(0, 0);
    assert_eq!(rotted(None), U);
    assert_eq!(rotted(Some(Catalyst { s: d.sugar, half: U, boost: 3.0 })), 2 * U);
}

#[test]
fn catalysts_compile_and_are_checked() {
    let with = |c: &str| DISH.replace("reaction respiration : sugar -> energy + carbon;", &format!("reaction respiration : sugar -> energy + carbon catalyze {c};"));
    let opts = || Options { seed: 1, allow_partial: true, mutation: false };
    let w = compile(&with("nitrate(half: 250mu, boost: 4.0)"), &["Drifter"], opts()).unwrap_or_else(|e| panic!("{e}")).world;
    let r = w.chem.reactions.iter().find(|r| r.name == "respiration").unwrap();
    let nitrate = w.chem.substances.iter().position(|s| s.name == "nitrate").unwrap();
    assert_eq!(r.catalyst, Some(Catalyst { s: nitrate, half: 250, boost: 4.0 }));
    for (c, needle) in [
        ("gold(half: 1u, boost: 2.0)", "unknown substance `gold`"),
        ("nitrate(half: 0mu, boost: 2.0)", "`half` must be greater than 0mu"),
        ("nitrate(half: 1u, boost: 0.5)", "`boost` must be at least 1.0"),
        ("nitrate(half: 1u)", "needs `boost`"),
        ("nitrate(half: 1u, boost: 2.0, lag: 1)", "takes no argument `lag`"),
    ] {
        let e = compile(&with(c), &["Drifter"], opts()).err().unwrap_or_else(|| panic!("`{c}` was accepted"));
        assert!(e.message.contains(needle), "{c}: {e}");
    }
}
