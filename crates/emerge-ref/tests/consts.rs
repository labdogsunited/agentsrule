//! Constants: a dish written with them is the same world as the dish written out.

use emerge_ref::compile::{compile, Options};

const DISH: &str = include_str!("../../../examples/dish.em");

/// `examples/dish.em` with the grid size, upkeep per gene, a rule's amount and `max_age` as constants and sums.
fn with_consts() -> String {
    let mut src = DISH.replacen("world Dish {\n", "world Dish {\n    const side = 2 * 128;\n    const lifetime = 25_000 - 5_000;\n    const base_amount = 160mu;\n", 1);
    for (from, to) in [
        ("Grid(w: 256, h: 256) holds", "Grid(w: side, h: side) holds"),
        ("Grid(w: 256, h: 256) signal", "Grid(w: side / 1, h: side) signal"),
        ("per_gene: 5mu;", "per_gene: 1mu + 4mu;"),
        ("max_age: 20_000;", "max_age: lifetime;"),
        ("amount: 40mu, overflow: void) @transfer_light", "amount: base_amount * 2500bp, overflow: void) @transfer_light"),
    ] {
        assert!(src.contains(from), "`{from}` is not in dish.em");
        src = src.replacen(from, to, 1);
    }
    src
}

fn fingerprint(src: &str, ticks: u64) -> Vec<String> {
    let mut w = compile(src, &["Drifter"], Options { seed: 5, allow_partial: true, mutation: true }).unwrap_or_else(|e| panic!("{e:?}")).world;
    for _ in 0..ticks {
        w.run_tick();
    }
    let mut out = vec![format!("tick {} births {} deaths {}", w.tick, w.births, w.deaths)];
    for e in &w.entities {
        out.push(format!("{} {:?} {:?} {:?}", e.id, e.pos, e.store.amounts, e.genome));
    }
    out
}

#[test]
fn a_dish_with_constants_runs_exactly_as_the_one_written_out() {
    let src = with_consts();
    assert_ne!(src, DISH);
    let (a, b) = (fingerprint(DISH, 100), fingerprint(&src, 100));
    assert!(a.len() > 100);
    assert_eq!(a, b);
}
