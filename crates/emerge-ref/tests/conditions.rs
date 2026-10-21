//! `when`, `else` and `let` in world rules (`proposals/rule-conditions.md`, stage 1). The pass rules
//! of the proposal, each as a test: an always-true condition equals none, a never-true one equals
//! absence, conditions read the start-of-tick snapshot exactly, `let` and `else` equal the same rule
//! spelled out, the ledger stays exact, refusals say why, and a swap can add a conditional rule.

use emerge_ref::compile::{compile, Options};
use emerge_ref::swap::Live;
use emerge_ref::World;

const DISH: &str = include_str!("../../../examples/dish.em");

fn small() -> String {
    DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;")
}

fn opts(seed: u64) -> Options {
    Options { seed, allow_partial: true, mutation: true }
}

/// The state a run is judged by: every cell, every creature's store and genome, the counters.
fn state(w: &World) -> String {
    let cells: Vec<Vec<u32>> = w.cells.iter().map(|c| c.amounts.to_vec()).collect();
    let ents: Vec<String> = w.entities.iter().map(|e| format!("{} {} {:?} {:?} {:?}", e.id, e.age, e.pos, e.store.amounts.to_vec(), e.genome)).collect();
    format!("{cells:?}\n{ents:?}\n{} {} {}", w.births, w.deaths, w.next_id)
}

fn run(src: &str, lineages: &[&str], seed: u64, ticks: u32) -> World {
    let mut w = compile(src, lineages, opts(seed)).unwrap_or_else(|e| panic!("{e}")).world;
    for t in 0..ticks {
        w.step().unwrap_or_else(|e| panic!("tick {t}: {e:?}"));
    }
    w.audit().unwrap();
    w
}

const SUN: &str = "transfer(light, from: sun, to: self, amount: 40mu, overflow: void) @transfer_light;";
const ROT: &str = "react(rot, rate: 50bp) @react_rot;";
const SPREAD: &str = "diffuse_conserved([sugar, carbon, nitrate, body], rate: 300bp) @diffuse_conserved;";

/// `dish.em` with every rule cut out except those given back.
fn cut_rules(src: &str, names: &[&str]) -> String {
    let mut out = src.to_string();
    for n in names {
        let a = out.find(&format!("    rule {n} on")).unwrap_or_else(|| panic!("no rule {n}"));
        let b = a + out[a..].find("\n    }\n").unwrap() + "\n    }\n".len();
        out.replace_range(a..b, "");
    }
    out
}

#[test]
fn a_condition_that_always_holds_equals_no_condition() {
    let plain = small();
    let wrapped = plain
        .replace(SUN, &format!("when 1 == 1 {{ {SUN} }}"))
        .replace(ROT, &format!("when tick >= 0 {{ {ROT} }}"))
        .replace(SPREAD, &format!("when self.carbon >= 0u and not (self.nitrate > cap(nitrate)) {{ {SPREAD} }}"));
    assert_ne!(plain, wrapped);
    for seed in [3, 7] {
        let (a, b) = (run(&plain, &["Drifter", "Builder"], seed, 600), run(&wrapped, &["Drifter", "Builder"], seed, 600));
        assert!(a.births > 0 && !a.entities.is_empty());
        assert_eq!(state(&a), state(&b), "seed {seed}: the guarded rules differ from the plain ones");
    }
}

#[test]
fn a_condition_that_never_holds_is_absence() {
    let absent = cut_rules(&small(), &["sunlight"]);
    let never = small().replace(SUN, &format!("when 1 == 2 {{ {SUN} }}"));
    let (a, b) = (run(&absent, &["Drifter", "Builder"], 3, 400), run(&never, &["Drifter", "Builder"], 3, 400));
    assert_eq!(state(&a), state(&b));
    // And the sunlight does matter: the plain world is not the same.
    assert_ne!(state(&run(&small(), &["Drifter", "Builder"], 3, 400)), state(&a));
}

/// A world with no creatures and one rule of our own: the dish's chemistry, cells at 20u nitrate.
fn nitrate_world(rules: &str) -> String {
    let mut src = cut_rules(&small(), &["sunlight", "weather", "rotting", "spread"]);
    let at = src.find("    genesis {").unwrap();
    src.insert_str(at, &format!("{rules}\n"));
    src
}

#[test]
fn a_condition_reads_the_cell_as_the_tick_begins() {
    // Each tick adds 1u while the cell holds under 25u. From 20u the cells reach exactly 25u and stop.
    let src = nitrate_world("    rule top_up on cells {\n        when self.nitrate < 25u {\n            transfer(nitrate, from: rock, to: self, amount: 1u, overflow: void) @transfer_nitrate;\n        }\n    }\n");
    let mut w = compile(&src, &[], opts(1)).unwrap_or_else(|e| panic!("{e}")).world;
    let n = w.chem.substances.iter().position(|s| s.name == "nitrate").unwrap();
    assert!(w.cells.iter().all(|c| c.amounts[n] == 20_000));
    for tick in 1..=12u32 {
        w.step().unwrap();
        let want = (20_000 + 1_000 * tick).min(25_000);
        assert!(w.cells.iter().all(|c| c.amounts[n] == want), "tick {tick}: expected every cell at {want}µ");
    }
    w.audit().unwrap();
}

#[test]
fn a_conditional_reaction_does_not_see_this_ticks_transfers() {
    // `photosynthesis` needs light, which the sun's transfer brings each tick (the field holds 1u at most, and the sun brings 40µ a tick).
    // Conditioned on `self.light >= 40mu`, it reads the start-of-tick light: none in the first tick, though the
    // transfer delivers light before the reactions run; from the second tick on there is some.
    let rules = |guard: &str| {
        let body = "react(photosynthesis, rate: 10000bp) @react_photo;";
        let react = if guard.is_empty() { body.to_string() } else { format!("when {guard} {{ {body} }}") };
        let mut src = cut_rules(&small(), &["weather", "rotting", "spread"]);
        let at = src.find("    genesis {").unwrap();
        src.insert_str(at, &format!("    rule photo on cells {{\n        {react}\n    }}\n\n"));
        src
    };
    let sugar = |w: &World| {
        let s = w.chem.substances.iter().position(|s| s.name == "sugar").unwrap();
        w.cells.iter().map(|c| c.amounts[s] as u64).sum::<u64>()
    };
    let (mut plain, mut guarded) = (compile(&rules(""), &[], opts(1)).unwrap().world, compile(&rules("self.light >= 40mu"), &[], opts(1)).unwrap().world);
    plain.step().unwrap();
    guarded.step().unwrap();
    assert!(sugar(&plain) > 0, "without a condition the light of this tick is used at once");
    assert_eq!(sugar(&guarded), 0, "with one, the reaction reads the light there was at the start of the tick: none");
    guarded.step().unwrap();
    assert!(sugar(&guarded) > 0, "in the second tick there is light at the start");
    guarded.audit().unwrap();
}

#[test]
fn let_and_else_equal_the_rule_spelled_out() {
    let sugared = nitrate_world(
        "    rule gate on cells {\n        let low = self.nitrate < 25u;\n        when low {\n            transfer(nitrate, from: rock, to: self, amount: 1u, overflow: void) @transfer_nitrate;\n        } else {\n            transfer(carbon, from: rock, to: self, amount: 1u, overflow: void) @transfer_carbon;\n        }\n    }\n",
    );
    let spelled = nitrate_world(
        "    rule gate on cells {\n        when self.nitrate < 25u {\n            transfer(nitrate, from: rock, to: self, amount: 1u, overflow: void) @transfer_nitrate;\n        }\n        when not (self.nitrate < 25u) {\n            transfer(carbon, from: rock, to: self, amount: 1u, overflow: void) @transfer_carbon;\n        }\n    }\n",
    );
    let (a, b) = (run(&sugared, &[], 1, 20), run(&spelled, &[], 1, 20));
    assert_eq!(state(&a), state(&b));
    // Both branches ran: nitrate stopped at 25u, and carbon grew from the tick it did.
    let at = |w: &World, name: &str| {
        let s = w.chem.substances.iter().position(|s| s.name == name).unwrap();
        w.cells[0].amounts[s]
    };
    assert_eq!(at(&a, "nitrate"), 25_000);
    assert_eq!(at(&a, "carbon"), 50_000 + 15_000, "20 ticks, the first 5 of them on the nitrate branch");
}

#[test]
fn nested_conditions_hold_where_all_of_them_hold() {
    let nested = nitrate_world("    rule gate on cells {\n        when self.nitrate < 25u {\n            when tick < 3 {\n                transfer(nitrate, from: rock, to: self, amount: 1u, overflow: void) @transfer_nitrate;\n            }\n        }\n    }\n");
    let w = run(&nested, &[], 1, 10);
    let s = w.chem.substances.iter().position(|s| s.name == "nitrate").unwrap();
    assert!(w.cells.iter().all(|c| c.amounts[s] == 23_000), "added in ticks 0, 1 and 2 only");
}

#[test]
fn what_cannot_be_evaluated_yet_is_refused_with_the_reason() {
    let with = |stmt: &str| nitrate_world(&format!("    rule gate on cells {{\n        {stmt}\n    }}\n"));
    let eff = "transfer(nitrate, from: rock, to: self, amount: 1u, overflow: void) @transfer_nitrate;";
    let code = |cond: &str| compile(&with(&format!("when {cond} {{ {eff} }}")), &[], opts(1)).err().map(|e| (e.code, e.message));
    let (c, m) = code("self.nitrate / 2u > 3u").unwrap();
    assert_eq!(c, "E1001", "{m}");
    assert!(m.contains("division"), "{m}");
    assert_eq!(code("above.nitrate > 0u").unwrap().0, "E1001");
    assert_eq!(code("self.value > 0.5").unwrap().0, "E1001");
    // A comparison of an amount with an integer, a condition that is not true or false, an unknown name.
    assert_eq!(code("self.nitrate > 5").unwrap().0, "E0205");
    assert_eq!(code("5u").unwrap().0, "E0205");
    assert_eq!(code("mystery > 1").unwrap().0, "E0114");
    // `for` in a world rule is not run either.
    let e = compile(&with(&format!("for cell in cells {{ {eff} }}")), &[], opts(1)).err().unwrap();
    assert_eq!(e.code, "E1001");
    assert!(compile(&with(&format!("when self.nitrate > 1u {{ {eff} }}")), &[], opts(1)).is_ok());
}

#[test]
fn a_swap_can_add_a_conditional_rule() {
    let mut live = Live::new(&small(), &["Drifter", "Builder"], opts(2)).unwrap();
    for _ in 0..5 {
        live.world.run_tick();
    }
    assert!(live.world.gpu_unsupported().is_none());
    let patch = "patch Dish {\n    add after rules.spread: rule gate on cells { when tick >= 0 and self.nitrate < 30u { transfer(nitrate, from: rock, to: self, amount: 1u, overflow: void) @transfer_gate; } }\n}\n";
    live.hotswap(patch, live.revision).unwrap_or_else(|e| panic!("{e}"));
    assert!(live.world.laws.iter().any(|l| l.is_conditional()));
    for _ in 0..50 {
        live.world.run_tick();
    }
    live.world.audit().unwrap();
    // The GPU runs it too (crates/emerge-gpu/tests/conditions.rs); only a condition deeper than its stack is refused.
    assert!(live.world.gpu_unsupported().is_none());
    // The rule can be changed and removed again.
    live.hotswap("patch Dish {\n    remove rules.gate;\n}\n", live.revision).unwrap();
    assert!(!live.world.laws.iter().any(|l| l.is_conditional()));
}
