//! Cell-to-cell transfers (`proposals/rule-conditions.md`, stage 2b): `transfer(s, from: self, to:
//! below, amount)` in a transfer rule. Fixtures worked out by hand on worlds without creatures.

use emerge_ref::compile::{compile, Options};
use emerge_ref::World;

const DISH: &str = include_str!("../../../examples/dish.em");

fn cut_rules(src: &str, names: &[&str]) -> String {
    let mut out = src.to_string();
    for n in names {
        let a = out.find(&format!("    rule {n} on")).unwrap_or_else(|| panic!("no rule {n}"));
        let b = a + out[a..].find("\n    }\n").unwrap() + "\n    }\n".len();
        out.replace_range(a..b, "");
    }
    out
}

fn source(w: usize, h: usize, wrap: bool, nitrate_cap: &str, rules: &str) -> String {
    let src = cut_rules(DISH, &["sunlight", "weather", "rotting", "spread"])
        .replace("Grid(w: 256, h: 256)", &format!("Grid(w: {w}, h: {h})"))
        .replace("wrap: true", &format!("wrap: {wrap}"))
        .replace("nitrate: 2_000u", &format!("nitrate: {nitrate_cap}"));
    let at = src.find("    genesis {").unwrap();
    let mut src = src;
    src.insert_str(at, &format!("{rules}\n"));
    src
}

fn world(w: usize, h: usize, wrap: bool, nitrate_cap: &str, rules: &str) -> World {
    compile(&source(w, h, wrap, nitrate_cap, rules), &[], Options { seed: 1, allow_partial: true, mutation: false }).unwrap_or_else(|e| panic!("{e}")).world
}

fn rule(body: &str) -> String {
    format!("    rule shift on cells {{\n        {body}\n    }}\n")
}

fn rows(w: &World, name: &str, width: usize) -> Vec<Vec<u32>> {
    let s = w.chem.substances.iter().position(|x| x.name == name).unwrap();
    w.cells.chunks(width).map(|r| r.iter().map(|c| c.amounts[s]).collect()).collect()
}

const DOWN: &str = "transfer(nitrate, from: self, to: below, amount: 1u) @transfer_nitrate;";

#[test]
fn a_cell_sends_to_its_neighbour_and_the_edge_sends_nothing() {
    // Every cell holds 20u. Each sends 1u down; the bottom row has no cell below and sends nothing.
    // After one tick the top row has lost 1u and the bottom row gained 1u; after two, the top row has
    // lost 2u and the bottom row gained 2u; the rows between send and receive 1u each tick.
    let mut w = world(6, 5, false, "2_000u", &rule(DOWN));
    w.step().unwrap();
    let g = rows(&w, "nitrate", 6);
    assert_eq!(g.iter().map(|r| r[0]).collect::<Vec<_>>(), [19_000, 20_000, 20_000, 20_000, 21_000]);
    w.step().unwrap();
    let g = rows(&w, "nitrate", 6);
    assert_eq!(g.iter().map(|r| r[3]).collect::<Vec<_>>(), [18_000, 20_000, 20_000, 20_000, 22_000]);
    w.audit().unwrap();
}

#[test]
fn on_a_wrapping_grid_nothing_changes_in_total_and_each_cell_stays_level() {
    let mut w = world(6, 5, true, "2_000u", &rule(DOWN));
    for _ in 0..4 {
        w.step().unwrap();
    }
    assert!(rows(&w, "nitrate", 6).iter().flatten().all(|&n| n == 20_000));
    w.audit().unwrap();
}

#[test]
fn every_direction_goes_where_its_name_says() {
    // On a 3 × 3 grid every cell sends 5u the same way. The cells on the far edge send nothing, so the
    // net is -5u on the edge the stream starts from and +5u on the edge it ends on, 0 between.
    for (to, net) in [
        ("above", [[5, 5, 5], [0, 0, 0], [-5, -5, -5]]),
        ("below", [[-5, -5, -5], [0, 0, 0], [5, 5, 5]]),
        ("left", [[5, 0, -5], [5, 0, -5], [5, 0, -5]]),
        ("right", [[-5, 0, 5], [-5, 0, 5], [-5, 0, 5]]),
    ] {
        let mut w = world(3, 3, false, "2_000u", &rule(&format!("transfer(nitrate, from: self, to: {to}, amount: 5u) @transfer_nitrate;")));
        w.step().unwrap();
        let got: Vec<Vec<i64>> = rows(&w, "nitrate", 3).iter().map(|r| r.iter().map(|&n| (n as i64 - 20_000) / 1_000).collect()).collect();
        assert_eq!(got, net.iter().map(|r| r.to_vec()).collect::<Vec<_>>(), "{to}");
    }
}

#[test]
fn two_senders_share_a_full_neighbours_headroom() {
    // Cells hold 20u and cap at 21u. In a row of three, the outer cells each send 1u to the middle
    // one, which has room for 1u: each gets half through, and keeps the rest.
    let src = rule("transfer(nitrate, from: self, to: right, amount: 1u) @transfer_nitrate;\n        transfer(nitrate, from: self, to: left, amount: 1u) @transfer_nitrate_2;");
    // Row of three, every cell sends both ways: the ends send once each (the other way is off the grid),
    // the middle sends twice. Origin shares: the middle has 20u so both its requests are met.
    let mut w = world(3, 1, false, "21u", &src);
    w.step().unwrap();
    let g = rows(&w, "nitrate", 3);
    // Middle receives 1u from each end but has room for 1u; sends 2u: net at the end of the tick the
    // headroom was measured at the start (1u), so only 1u in total gets in, 0.5u from each end.
    assert_eq!(g[0], [20_000 - 500 + 1_000, 20_000 - 2_000 + 1_000, 20_000 - 500 + 1_000]);
    w.audit().unwrap();
}

#[test]
fn refusals_say_why() {
    for (body, want) in [
        ("transfer(nitrate, from: self, to: moore, amount: 1u) @transfer_nitrate;", "above"),
        ("transfer(nitrate, from: self, to: below, amount: 1u, overflow: void) @transfer_nitrate;", "overflow"),
    ] {
        let e = compile(&source(3, 3, false, "2_000u", &rule(body)), &[], Options { seed: 1, allow_partial: true, mutation: false }).err().expect("must be refused");
        assert!(e.to_string().contains(want), "{e}");
    }
}
