//! The renderer (`proposals/visual-language.md` V2): a field is drawn as exact cells, every pixel of
//! a cell has the colour of that cell's value under the view's scale and palette, and what cannot be
//! shown is shown as such.

use emerge_lang::ast::{Item, View};
use emerge_view::palette::{self, EMPTY, UNAVAILABLE};
use emerge_view::{render, Agent, Frame, Options};

fn view(src: &str) -> View {
    let file = emerge_lang::parse(src).unwrap();
    file.items.into_iter().find_map(|i| match i.node {
        Item::View(v) => Some(v),
        _ => None,
    }).unwrap()
}

/// A 7 × 5 grid of one substance, `x` µ in each cell, and the given agents.
fn frame(cells: Vec<u32>, agents: Vec<Agent>) -> Frame {
    Frame { tick: 3, revision: 1, world_w: 14.0, world_h: 10.0, wrap: true, field: "cells".into(), grid_w: 7, grid_h: 5, substances: vec!["sugar".into()], cells, lineages: vec!["A".into(), "B".into()], agents, made: None, states: Vec::new() }
}

fn pixel(pm: &tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 3] {
    let p = pm.pixel(x, y).unwrap();
    [p.red(), p.green(), p.blue()]
}

/// Every pixel of every cell, against the mapping worked out here.
fn cells_are_exact(src: &str, cells: Vec<u32>, want: impl Fn(f64) -> [u8; 3]) {
    let k = 6;
    let f = frame(cells.clone(), vec![]);
    let pm = render(&view(src), &f, Options { cell_px: k, legend: false });
    assert_eq!((pm.width(), pm.height()), (7 * k, 5 * k));
    for (c, &mu) in cells.iter().enumerate() {
        let expect = want(mu as f64 / 1000.0);
        let (cx, cy) = ((c % 7) as u32 * k, (c / 7) as u32 * k);
        for dy in 0..k {
            for dx in 0..k {
                assert_eq!(pixel(&pm, cx + dx, cy + dy), expect, "cell {c} ({mu}µ), pixel {dx},{dy}");
            }
        }
    }
}

#[test]
fn a_log_scale_maps_each_cell_and_zero_is_empty() {
    let src = "view V for W {\n    layer g = field(cells.sugar) {\n        color: value |> scale(log, range: 1.0..1_000.0) |> palette(magma);\n    }\n}\n";
    let cells: Vec<u32> = (0..35).map(|i| if i % 5 == 0 { 0 } else { 137 * i * i }).collect();
    cells_are_exact(src, cells, |u| {
        if u <= 0.0 {
            EMPTY
        } else {
            palette::sample("magma", ((u.ln() - 0.0) / (1000f64.ln())).clamp(0.0, 1.0))
        }
    });
}

#[test]
fn a_linear_scale_clamps_and_a_constant_field_is_one_colour() {
    let src = "view V for W {\n    layer g = field(cells.sugar) {\n        color: value |> scale(linear, range: 2.0..30.0) |> palette(viridis);\n    }\n}\n";
    let cells: Vec<u32> = (0..35).map(|i| 1000 * i).collect();
    cells_are_exact(src, cells, |u| palette::sample("viridis", ((u - 2.0) / 28.0).clamp(0.0, 1.0)));
    cells_are_exact(src, vec![7_000; 35], |_| palette::sample("viridis", 5.0 / 28.0));
}

#[test]
fn agents_are_drawn_and_what_is_not_computed_is_grey() {
    let a = |id, lineage, x| Agent { id, lineage, pos: [x, 5.0], heading: [1.0, 0.0], age: 1, expressed_genes: 4, amounts: vec![0], made: 0 };
    let f = frame(vec![0; 35], vec![a(u64::MAX, 0, 3.0), a(7, 1, 11.0)]);
    let by_lineage = view("view V for W {\n    layer life = agents(where: true) {\n        glyph: dot();\n        color: lineage_id |> palette(categorical);\n        size: 1.5;\n    }\n}\n");
    let pm = render(&by_lineage, &f, Options { cell_px: 10, legend: false });
    assert_eq!(pixel(&pm, 15, 25), palette::category(0), "agent A at its centre");
    assert_eq!(pixel(&pm, 55, 25), palette::category(1), "agent B at its centre");
    assert_eq!(pixel(&pm, 35, 25), EMPTY, "nothing between them");
    let species = view("view V for W {\n    layer life = agents(where: true) {\n        glyph: dot();\n        color: species_id |> palette(categorical);\n        size: 1.5;\n    }\n}\n");
    let pm = render(&species, &f, Options { cell_px: 10, legend: false });
    assert_eq!(pixel(&pm, 15, 25), UNAVAILABLE);
    // The legend renders too (and names the unavailable binding; read by eye in the PNG).
    let pm = render(&species, &f, Options { cell_px: 10, legend: true });
    assert!(pm.width() > 70);
}

#[test]
fn made_needs_a_window_and_the_legend_names_it() {
    let a = |id, x, made| Agent { id, lineage: 0, pos: [x, 5.0], heading: [1.0, 0.0], age: 1, expressed_genes: 4, amounts: vec![0], made };
    let mut f = frame(vec![0; 35], vec![a(1, 3.0, 0), a(2, 11.0, 250)]);
    let producers = view("view V for W {\n    layer life = agents(where: made > 0mu) {\n        glyph: dot();\n        color: lineage_id |> palette(categorical);\n        size: 1.5;\n    }\n}\n");
    // Without a window, `made` is not computed: nobody passes the filter as a producer.
    let pm = render(&producers, &f, Options { cell_px: 10, legend: false });
    assert_eq!((pixel(&pm, 15, 25), pixel(&pm, 55, 25)), (EMPTY, EMPTY));
    f.made = Some(emerge_view::frame::Made { substance: "body".into(), from_tick: 0 });
    let pm = render(&producers, &f, Options { cell_px: 10, legend: false });
    assert_eq!(pixel(&pm, 15, 25), EMPTY, "made nothing in the window");
    assert_eq!(pixel(&pm, 55, 25), palette::category(0), "a producer");
    let (_, key) = emerge_view::render::agents_layer(&producers.layers[0].node, &f);
    assert!(key.entries.iter().any(|e| e == "made: body made by its own reactions, ticks 0–3"), "{:?}", key.entries);
}


#[test]
fn a_state_field_is_drawn_cell_by_cell_with_its_states_in_the_legend() {
    let mut f = frame(vec![0; 35], Vec::new());
    let cells: Vec<u8> = (0..35).map(|i| u8::from(i % 3 == 0)).collect();
    f.states = vec![emerge_view::frame::States { name: "life".into(), names: vec!["dead".into(), "alive".into()], cells: cells.clone() }];
    let v = view("view V for W {\n    layer board = field(life) {\n        color: value |> scale(linear, range: 0.0..1.0) |> palette(grays);\n    }\n}\n");
    let pm = render(&v, &f, Options { cell_px: 10, legend: false });
    let (dead, alive) = (palette::sample("grays", 0.0), palette::sample("grays", 1.0));
    for (i, &c) in cells.iter().enumerate() {
        let (x, y) = ((i % 7) as u32 * 10 + 5, (i / 7) as u32 * 10 + 5);
        assert_eq!(pixel(&pm, x, y), if c == 1 { alive } else { dead }, "cell {i}");
    }
    let (_, key) = emerge_view::render::field_layer(&v.layers[0].node, &f);
    assert_eq!(key.categories, vec![("dead".to_string(), dead), ("alive".to_string(), alive)]);
}
