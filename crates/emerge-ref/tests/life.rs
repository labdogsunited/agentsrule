//! State fields and `life_like` (`proposals/field-rules.md` step 1, plan item 12 V3): Game of Life
//! fixtures evolve exactly, cell by cell; `scatter` is reproducible; misuse is an error.

use emerge_ref::compile::{compile, Options};
use emerge_ref::World;
use std::collections::BTreeSet;

/// A 16 × 16 world with a state field `life`, its rule, and `genesis` statements; no lineages.
fn world(wrap: bool, rule: &str, genesis: &str) -> String {
    format!(
        "world Life {{
    space: Continuous2D(w: 16.0, h: 16.0, wrap: {wrap}, max_interaction_radius: 1.0);

    element E;

    second_law {{ min_drop: 1; }}

    substance energy = E potential 1;
    substance heat = E potential 0;

    field cells : Grid(w: 16, h: 16) holds [energy: 1u, heat: 1u];
    field life : Grid(w: 16, h: 16) states [dead, alive];

    sink void : any;

    waste heat -> void;
    remains: void;

    upkeep {{
        pay: energy;
        becomes: heat;
        vent: void;
        base: 1mu;
    }}

    limits {{
        max_entities: 10;
        inventory_cap: 1u;
    }}

    rule conway on life {{
        {rule}
    }}

    genesis {{
        {genesis}
    }}
}}
"
    )
}

const LIFE: &str = "life_like(born: [3], survives: [2, 3]) @life;";

fn run(src: &str) -> World {
    compile(src, &[], Options { seed: 7, allow_partial: false, mutation: false }).unwrap_or_else(|e| panic!("{e:?}")).world
}

/// The live cells.
fn alive(w: &World) -> BTreeSet<(usize, usize)> {
    let f = &w.states[0];
    (0..f.cells.len()).filter(|&i| f.cells[i] == 1).map(|i| (i % f.w, i / f.w)).collect()
}

fn ticks(w: &mut World, n: usize) {
    for _ in 0..n {
        w.step().unwrap();
    }
}

fn set(cells: &[(usize, usize)]) -> BTreeSet<(usize, usize)> {
    cells.iter().copied().collect()
}

#[test]
fn a_blinker_has_period_two_and_a_block_stays() {
    let mut w = run(&world(true, LIFE, "pattern(life, x: 6, y: 7, rle: \"3o!\", to: alive) @blinker;\n        pattern(life, x: 1, y: 1, rle: \"2o$2o!\", to: alive) @block;"));
    let block = [(1, 1), (2, 1), (1, 2), (2, 2)];
    let flat = set(&[(6, 7), (7, 7), (8, 7)].iter().chain(&block).copied().collect::<Vec<_>>());
    let upright = set(&[(7, 6), (7, 7), (7, 8)].iter().chain(&block).copied().collect::<Vec<_>>());
    assert_eq!(alive(&w), flat);
    ticks(&mut w, 1);
    assert_eq!(alive(&w), upright);
    ticks(&mut w, 1);
    assert_eq!(alive(&w), flat);
}

#[test]
fn a_glider_moves_one_cell_diagonally_every_four_ticks_and_wraps_home() {
    let glider = set(&[(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)]);
    let mut w = run(&world(true, LIFE, "pattern(life, x: 0, y: 0, rle: \"bo$2bo$3o!\", to: alive) @glider;"));
    assert_eq!(alive(&w), glider);
    ticks(&mut w, 4);
    assert_eq!(alive(&w), glider.iter().map(|&(x, y)| (x + 1, y + 1)).collect());
    // 64 ticks on a 16 × 16 wrapping grid: 16 cells each way, back where it started.
    ticks(&mut w, 60);
    assert_eq!(alive(&w), glider);
}

#[test]
fn on_a_bounded_grid_a_glider_ends_as_a_block_in_the_corner() {
    let mut w = run(&world(false, LIFE, "pattern(life, x: 0, y: 0, rle: \"bo$2bo$3o!\", to: alive) @glider;"));
    ticks(&mut w, 200);
    assert_eq!(alive(&w), set(&[(14, 14), (15, 14), (14, 15), (15, 15)]));
}

#[test]
fn scatter_is_reproducible_and_each_statement_draws_its_own() {
    let soup = |genesis: &str| alive(&run(&world(true, LIFE, genesis)));
    let a = soup("scatter(life, density: 0.3, to: alive) @soup;");
    assert_eq!(a, soup("scatter(life, density: 0.3, to: alive) @soup;"), "same seed, same soup");
    assert!((40..115).contains(&a.len()), "about 30% of 256 cells, got {}", a.len());
    // A second statement draws other numbers: the union is larger than either alone.
    let two = soup("scatter(life, density: 0.3, to: alive) @soup;\n        scatter(life, density: 0.3, to: alive) @more;");
    assert!(two.len() > a.len() && a.is_subset(&two));
    assert!(soup("scatter(life, density: 0.0, to: alive) @soup;").is_empty());
    assert_eq!(soup("scatter(life, density: 1.0, to: alive) @soup;").len(), 256);
}

#[test]
fn misuse_is_an_error() {
    let code = |src: String| compile(&src, &[], Options { seed: 1, allow_partial: false, mutation: false }).err().map(|e| e.code);
    assert_eq!(code(world(true, "life_like(born: [9], survives: [2]) @life;", "")), Some("E0203"), "a cell has at most 8 neighbours");
    assert!(code(world(true, "life_like(born: [3]) @life;", "")).is_some(), "survives is required");
    assert_eq!(code(world(true, LIFE, "pattern(life, x: 15, y: 15, rle: \"2o!\", to: alive) @off;")), Some("E0803"), "a pattern outside the grid");
    assert_eq!(code(world(true, LIFE, "pattern(life, x: 0, y: 0, rle: \"2x!\", to: alive) @bad;")), Some("E0205"), "not run-length encoding");
    assert_eq!(code(world(true, LIFE, "pattern(life, x: 0, y: 0, rle: \"2o!\", to: zombie) @bad;")), Some("E0114"), "an unknown state");
    assert_eq!(code(world(true, LIFE, "scatter(life, density: 1.5, to: alive) @bad;")), Some("E0203"));
    let three = world(true, LIFE, "").replace("states [dead, alive]", "states [dead, alive, dying]");
    assert_eq!(code(three), Some("E0205"), "life_like needs two states");
    let second = world(true, LIFE, "").replace("    genesis {", "    rule again on life {\n        life_like(born: [3], survives: [2, 3]) @again;\n    }\n\n    genesis {");
    assert_eq!(code(second), Some("E0305"), "one rule per state field");
    let other_grid = world(true, LIFE, "").replace("field life : Grid(w: 16, h: 16)", "field life : Grid(w: 8, h: 8)");
    assert_eq!(code(other_grid), Some("E0205"), "the conserved field's grid, for now");
}

#[test]
fn the_world_formats_to_itself() {
    let src = world(true, LIFE, "pattern(life, x: 0, y: 0, rle: \"bo$2bo$3o!\", to: alive) @glider;");
    let once = emerge_lang::format(&emerge_lang::parse(&src).unwrap());
    let twice = emerge_lang::format(&emerge_lang::parse(&once).unwrap());
    assert_eq!(once, twice);
    assert!(once.contains("states [dead, alive]") && once.contains("rle: \"bo$2bo$3o!\""), "{once}");
}

#[test]
fn a_changed_rule_is_not_swappable_and_views_can_show_the_field() {
    use emerge_ref::swap::{Live, Rejected};
    let src = world(true, LIFE, "pattern(life, x: 0, y: 0, rle: \"bo$2bo$3o!\", to: alive) @glider;");
    let mut l = Live::new(&src, &[], Options { seed: 1, allow_partial: false, mutation: false }).unwrap();
    let r = l.hotswap("patch Life {\n    replace rules.conway { life_like(born: [3, 6], survives: [2, 3]) @life; }\n}\n", 0);
    assert!(matches!(&r, Err(Rejected::NotSwappable(m)) if m.contains("state fields")), "{r:?}");
    // A view shows a state field as `field(life)`; `life.alive` is not a substance of it.
    let with = |layer: &str| {
        let file = emerge_lang::parse(&format!("{src}\nview Board for Life {{\n    layer board = {layer} {{\n        color: value |> scale(linear, range: 0.0..1.0) |> palette(grays);\n    }}\n}}\n")).unwrap();
        emerge_lang::view::check(&file).0.into_iter().map(|e| (e.code, e.message)).collect::<Vec<_>>()
    };
    assert!(with("field(life)").is_empty());
    let bad = with("field(life.alive)");
    assert!(bad.iter().any(|(c, m)| *c == "E0114" && m.contains("state field")), "{bad:?}");
}
