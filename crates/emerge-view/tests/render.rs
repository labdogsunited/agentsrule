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
    Frame { tick: 3, revision: 1, world_w: 14.0, world_h: 10.0, wrap: true, field: "cells".into(), grid_w: 7, grid_h: 5, substances: vec!["sugar".into()], cells, lineages: vec!["A".into(), "B".into()], agents }
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
    let a = |id, lineage, x| Agent { id, lineage, pos: [x, 5.0], heading: [1.0, 0.0], age: 1, expressed_genes: 4, amounts: vec![0] };
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
