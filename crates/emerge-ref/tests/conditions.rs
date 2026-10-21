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
    assert_eq!(code("mean(moore.nitrate) > 0u").unwrap().0, "E1001");
    assert_eq!(code("scent.here > 0u").unwrap().0, "E1001");
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

// ---------- stage 2a: neighbour reads (proposals/rule-conditions.md) ----------

/// A `w × h` grid that wraps or not, no creatures, and the rules given; cells start at 50u carbon,
/// 20u nitrate, 1u body and no light.
fn grid_world(w: usize, h: usize, wrap: bool, rules: &str) -> World {
    let src = cut_rules(DISH, &["sunlight", "weather", "rotting", "spread"])
        .replace("Grid(w: 256, h: 256)", &format!("Grid(w: {w}, h: {h})"))
        .replace("wrap: true", &format!("wrap: {wrap}"));
    let at = src.find("    genesis {").unwrap();
    let mut src = src;
    src.insert_str(at, &format!("{rules}\n"));
    compile(&src, &[], opts(1)).unwrap_or_else(|e| panic!("{e}")).world
}

fn inflow(s: &str, amount: &str, from: &str) -> String {
    format!("transfer({s}, from: {from}, to: self, amount: {amount}, overflow: void) @transfer_{s}")
}

/// One rule per line of `(name, condition, substance, amount, source)`.
fn rules(list: &[(&str, &str, &str, &str, &str)]) -> String {
    list.iter().map(|(n, c, s, a, f)| format!("    rule {n} on cells {{\n        when {c} {{ {}; }}\n    }}\n", inflow(s, a, f))).collect()
}

/// The amount of `name` in every cell, row by row.
fn rows(w: &World, name: &str, width: usize) -> Vec<Vec<u32>> {
    let s = w.chem.substances.iter().position(|x| x.name == name).unwrap();
    w.cells.chunks(width).map(|r| r.iter().map(|c| c.amounts[s]).collect()).collect()
}

const NEIGHBOUR_RULES: [(&str, &str, &str, &str, &str); 4] = [
    ("grow", "above.nitrate == 0u", "nitrate", "1u", "rock"),
    ("stripe", "max(vonneumann.nitrate) > self.nitrate", "carbon", "1u", "rock"),
    ("inner", "sum(moore.nitrate) > 8 * self.nitrate", "body", "1mu", "rock"),
    ("none", "min(moore.nitrate) == 0u", "light", "1mu", "sun"),
];

#[test]
fn neighbour_reads_give_the_amounts_worked_out_by_hand() {
    // A 6 × 5 grid that does not wrap. Only the top row has no cell above, which reads 0, so only it
    // grows (1u a tick). From the second tick the row below it sees a richer neighbour above (`max`),
    // and its inner cells, with three richer cells among their eight, have a Moore sum above 8 times
    // their own (`sum`). Cells outside the grid are no members, so `min` is never 0.
    let mut w = grid_world(6, 5, false, &rules(&NEIGHBOUR_RULES));
    for _ in 0..5 {
        w.step().unwrap();
    }
    w.audit().unwrap();
    let row = |v: u32| vec![v; 6];
    assert_eq!(rows(&w, "nitrate", 6), vec![row(25_000), row(20_000), row(20_000), row(20_000), row(20_000)]);
    assert_eq!(rows(&w, "carbon", 6), vec![row(50_000), row(54_000), row(50_000), row(50_000), row(50_000)]);
    assert_eq!(rows(&w, "body", 6)[1], vec![1_000, 1_004, 1_004, 1_004, 1_004, 1_000], "the edge cells of row 1 have five neighbours");
    assert!(rows(&w, "body", 6).iter().enumerate().all(|(y, r)| y == 1 || r.iter().all(|&b| b == 1_000)));
    assert!(rows(&w, "light", 6).iter().flatten().all(|&l| l == 0));
    // On the same grid wrapping, every cell has a cell above, and nothing happens at all.
    let mut w = grid_world(6, 5, true, &rules(&NEIGHBOUR_RULES));
    for _ in 0..5 {
        w.step().unwrap();
    }
    assert!(rows(&w, "nitrate", 6).iter().flatten().all(|&n| n == 20_000));
    assert!(rows(&w, "carbon", 6).iter().flatten().all(|&n| n == 50_000));
}

#[test]
fn border_cells_have_fewer_neighbours() {
    // Every cell has 8 Moore neighbours at 20u or more, except on the border of a grid that does not wrap.
    let r = rules(&[("border", "count(moore.nitrate >= 20u) < 8", "nitrate", "1u", "rock")]);
    let mut w = grid_world(5, 4, false, &r);
    for _ in 0..3 {
        w.step().unwrap();
    }
    let grid = rows(&w, "nitrate", 5);
    for (y, row) in grid.iter().enumerate() {
        for (x, &n) in row.iter().enumerate() {
            let border = x == 0 || y == 0 || x == 4 || y == 3;
            assert_eq!(n, if border { 23_000 } else { 20_000 }, "cell ({x}, {y})");
        }
    }
    let mut w = grid_world(5, 4, true, &r);
    w.step().unwrap();
    assert!(rows(&w, "nitrate", 5).iter().flatten().all(|&n| n == 20_000));
}

#[test]
fn on_a_narrow_wrapping_grid_a_neighbour_can_count_twice() {
    // Two cells wide: left and right are the same cell, and so are the three cells to each side; one
    // cell high would make the cell its own neighbour. All 8 members are there, as in `life_like`.
    let r = rules(&[
        ("all", "count(moore.nitrate >= 20u) == 8 and sum(moore.nitrate) == 8 * self.nitrate", "nitrate", "1u", "rock"),
        ("self_too", "count(vonneumann.carbon == self.carbon) == 4", "carbon", "1u", "rock"),
    ]);
    let mut w = grid_world(2, 1, true, &r);
    w.step().unwrap();
    assert!(rows(&w, "nitrate", 2).iter().flatten().all(|&n| n == 21_000));
    assert!(rows(&w, "carbon", 2).iter().flatten().all(|&n| n == 51_000));
}

#[test]
fn neighbour_reads_refuse_what_they_cannot_mean() {
    let code = |cond: &str| {
        let src = cut_rules(&small(), &["sunlight", "weather", "rotting", "spread"]);
        let at = src.find("    genesis {").unwrap();
        let mut src = src;
        src.insert_str(at, &rules(&[("gate", cond, "nitrate", "1u", "rock")]));
        compile(&src, &[], opts(1)).err().map(|e| (e.code, e.message))
    };
    for (cond, want, says) in [
        ("mean(moore.nitrate) > 1u", "E1001", "mean"),
        ("argmin(moore.nitrate) > 1u", "E1001", "argmin"),
        ("sum(moore.nitrate + sum(moore.carbon)) > 1u", "E1001", "inside another reduction"),
        ("sum(moore.nitrate + vonneumann.carbon) > 1u", "E0205", "not both"),
        ("moore.nitrate > 1u", "E0205", "inside `sum`"),
        ("min(self.nitrate, 5u) > 1u", "E0005", "only the first argument may be unnamed"),
        ("count(moore.nitrate) > 1", "E0205", "count"),
        ("sum(moore.nitrate > 1u) > 1", "E0205", "use `count"),
        ("sum(self.nitrate) > 1u", "E0205", "needs a collection"),
        ("scent.here > 1u", "E1001", "scent"),
    ] {
        let (c, m) = code(cond).unwrap_or_else(|| panic!("`{cond}` was accepted"));
        assert_eq!(c, want, "`{cond}`: {m}");
        assert!(m.contains(says), "`{cond}`: {m}");
    }
    assert!(code("count(moore.nitrate > self.nitrate) >= 3 and min(vonneumann.carbon * 2) < 100u + below.carbon").is_none());
}
