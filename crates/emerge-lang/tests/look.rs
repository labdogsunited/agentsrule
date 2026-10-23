//! The Look network (`proposals/houdini-node-system.md` §11, H5): a view as a typed dataflow of its own syntax, edits that
//! print through the formatter and keep the file checkable, and any node shown alone.

use emerge_lang::look::{derived, edit_view, pipeline, put_view, view_in, LookOp};

const DISH: &str = include_str!("../../../examples/dish.em");
const VIEWS: &str = include_str!("../../../examples/views/dish.em");

fn file() -> emerge_lang::ast::File {
    emerge_lang::parse(&format!("{DISH}\n{VIEWS}")).unwrap()
}

#[test]
fn a_view_is_a_typed_network_of_its_own_pieces() {
    let p = pipeline(&file(), "Energy").unwrap();
    let node = |id: &str| p.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no {id}"));
    assert_eq!(node("layer:ground/source").out, "cells");
    assert_eq!((node("layer:ground/color/0").kind, node("layer:ground/color/0").out), ("read", "number"));
    assert_eq!(node("layer:ground/color/1").out, "scaled");
    assert_eq!(node("layer:ground/color/1").params, vec![("kind".to_string(), "linear".to_string()), ("range".to_string(), "0.0..150.0".to_string())]);
    assert_eq!(node("layer:ground/color/2").out, "colour");
    assert_eq!(node("layer:life/color/0").out, "number", "units(energy) is a number");
    assert_eq!(node("layer:life/size/0").kind, "expression");
    assert_eq!(node("layer:life/glyph").out, "glyph");
    // Every wire joins two nodes; the composite takes the layers in their order.
    for w in &p.wires {
        assert!(p.nodes.iter().any(|n| n.id == w.from) && p.nodes.iter().any(|n| n.id == w.to), "{w:?}");
    }
    let into: Vec<&str> = p.wires.iter().filter(|w| w.to == "composite").map(|w| w.from.as_str()).collect();
    assert_eq!(into, ["layer:ground", "layer:life"]);
    // A body is one node, typed as the attributes it writes.
    let b = pipeline(&file(), "Biolume").unwrap();
    assert!(b.nodes.iter().any(|n| n.id == "layer:halo/body" && n.text.contains("@pscale")));
}

#[test]
fn edits_print_through_the_formatter_and_keep_the_file_checkable() {
    let v = view_in(VIEWS, "Energy").unwrap();
    let step = |v: &emerge_lang::ast::View, op: LookOp| edit_view(v, &op).unwrap();
    let a = step(&v, LookOp::SetStage { layer: "ground".into(), entry: "color".into(), index: 1, text: "scale(log, range: 1.0..150.0)".into() });
    let b = step(&a, LookOp::SetStage { layer: "ground".into(), entry: "color".into(), index: 2, text: "palette(viridis)".into() });
    let c = step(&b, LookOp::InsertStage { layer: "life".into(), entry: "color".into(), after: 0, text: "scale(linear, range: 0.0..40.0)".into() });
    let c = step(&c, LookOp::RemoveStage { layer: "life".into(), entry: "color".into(), index: 2 });
    let d = step(&c, LookOp::AddLayer { name: "nitrate".into(), source: "field(cells.nitrate)".into(), color: "value |> scale(log, range: 0.5..60.0) |> palette(blues)".into() });
    let d = step(&d, LookOp::SetEntry { layer: "nitrate".into(), key: "opacity".into(), text: Some("0.5".into()) });
    let d = step(&d, LookOp::MoveLayer { layer: "nitrate".into(), up: false });
    let doc = put_view(VIEWS, &d).unwrap();
    let text = format!("{DISH}\n{doc}");
    let (errors, _) = emerge_lang::view::check(&emerge_lang::parse(&text).unwrap());
    assert!(errors.is_empty(), "{errors:?}");
    assert!(doc.contains("color: value |> scale(log, range: 1.0..150.0) |> palette(viridis);"), "{doc}");
    let back = view_in(&doc, "Energy").unwrap();
    assert_eq!(back.layers.iter().map(|l| l.node.name.as_str()).collect::<Vec<_>>(), ["ground", "nitrate", "life"]);
    // The other views of the document are as they were.
    assert_eq!(view_in(&doc, "Lineages"), view_in(VIEWS, "Lineages"));
    // A stage that is not a call, a missing layer, the last layer: refused with a reason.
    assert!(edit_view(&v, &LookOp::SetStage { layer: "ground".into(), entry: "color".into(), index: 1, text: "1.0".into() }).is_err());
    assert!(edit_view(&v, &LookOp::RemoveLayer { layer: "nope".into() }).is_err());
    // An edit the checker refuses is caught by whoever applies it: palette(categorical) of a scaled number.
    let bad = step(&v, LookOp::SetStage { layer: "ground".into(), entry: "color".into(), index: 2, text: "palette(nosuch)".into() });
    let (errors, _) = emerge_lang::view::check(&emerge_lang::parse(&format!("{DISH}\n{}", put_view(VIEWS, &bad).unwrap())).unwrap());
    assert!(!errors.is_empty());
}

#[test]
fn any_node_can_be_shown_alone() {
    let f = file();
    let w = f.items.iter().find_map(|i| match &i.node { emerge_lang::ast::Item::World(w) => Some(w.clone()), _ => None }).unwrap();
    let v = view_in(VIEWS, "Energy").unwrap();
    // A scaled value through grays; a number asks for its range first.
    let s = derived(&v, "layer:ground/color/1", "P", None, &w).unwrap();
    assert!(s.needs_range.is_none());
    let one = emerge_lang::format(&emerge_lang::ast::File { items: vec![emerge_lang::ast::Node { trivia: Default::default(), pos: Default::default(), node: emerge_lang::ast::Item::View(s.view) }], tail: vec![] });
    assert!(one.contains("palette(grays)") && one.contains("layer ground"), "{one}");
    let n = derived(&v, "layer:life/color/0", "P", None, &w).unwrap();
    assert!(n.needs_range.is_some());
    let n = derived(&v, "layer:life/color/0", "P", Some((0.0, 12.5)), &w).unwrap();
    assert!(n.shows.contains("12.5"));
    // Every derived view checks.
    for id in ["composite", "layer:ground", "layer:ground/source", "layer:ground/color/0", "layer:ground/color", "layer:life/source", "layer:life/size", "layer:life/glyph", "layer:life/color/1"] {
        let d = derived(&v, id, "P", Some((0.0, 1.0)), &w).unwrap();
        let doc = put_view("", &d.view).unwrap();
        let (errors, _) = emerge_lang::view::check(&emerge_lang::parse(&format!("{DISH}\n{doc}")).unwrap());
        assert!(errors.is_empty(), "{id}: {errors:?}\n{doc}");
    }
}
