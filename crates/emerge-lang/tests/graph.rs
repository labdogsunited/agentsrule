//! The graph projection (`proposals/node-editor.md` §5): every declaration and every rule
//! statement of every world file in the repository is exactly one box, each box's text is the
//! text `emerge fmt` writes for it, and the wires are the transfers and reactions the schedule
//! places.

use emerge_lang::ast::{Item, WDecl};
use emerge_lang::graph::{graph, Kind};
use std::path::{Path, PathBuf};

fn world_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()) {
        let p = e.path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if p.is_dir() && name != "target" && name != ".git" {
            world_files(&p, out);
        } else if name.ends_with(".em") && !name.ends_with(".patch.em") {
            out.push(p);
        }
    }
}

#[test]
fn every_statement_is_one_box_and_the_text_is_the_formatters() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    world_files(&root, &mut files);
    let mut worlds = 0;
    for path in files {
        let src = std::fs::read_to_string(&path).unwrap();
        let Ok(file) = emerge_lang::parse(&src) else { continue };
        if emerge_lang::prepare(&file).is_err() {
            continue;
        }
        let formatted = emerge_lang::format(&file);
        for item in &file.items {
            let Item::World(w) = &item.node else { continue };
            let g = graph(&file, &w.name).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            worlds += 1;
            // Declarations: one box each, ids unique.
            let mut ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
            ids.sort();
            let n = ids.len();
            ids.dedup();
            assert_eq!(n, ids.len(), "{}: box ids are not unique", path.display());
            let decl_boxes = g.nodes.iter().filter(|n| n.group.is_none()).count();
            assert_eq!(decl_boxes, w.decls.len(), "{}: {}", path.display(), w.name);
            // Statements: one box each, in their rule.
            let stmts: usize = w.decls.iter().map(|d| match &d.node {
                WDecl::Rule { body, .. } => body.len(),
                _ => 0,
            }).sum();
            assert_eq!(g.nodes.iter().filter(|n| n.group.is_some()).count(), stmts, "{}", path.display());
            // The text of a box is in the formatted file, indented one level inside the world.
            for node in g.nodes.iter().filter(|n| n.group.is_none()) {
                let block: Vec<String> = node.text.lines().map(|l| if l.is_empty() { String::new() } else { format!("    {l}") }).collect();
                let joined = block.join("\n");
                assert!(formatted.contains(&joined), "{}: the text of {} is not in `emerge fmt`'s output:\n{joined}", path.display(), node.id);
            }
            // Wires join existing boxes.
            for wire in &g.wires {
                assert!(g.nodes.iter().any(|n| n.id == wire.from) && g.nodes.iter().any(|n| n.id == wire.to), "{}: {wire:?}", path.display());
            }
        }
    }
    assert!(worlds >= 6, "only {worlds} worlds");
}

#[test]
fn the_dish_draws_its_flows() {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/dish.em")).unwrap();
    let file = emerge_lang::parse(&src).unwrap();
    let g = graph(&file, "Dish").unwrap();
    let has = |from: &str, to: &str, s: &str| g.wires.iter().any(|w| w.from == from && w.to == to && w.substance.as_deref() == Some(s));
    // sun --light--> transfer --light--> cells
    assert!(has("Dish.sources.sun", "Dish.rules.sunlight@transfer_light", "light"));
    assert!(has("Dish.rules.sunlight@transfer_light", "Dish.fields.cells", "light"));
    // a reaction rule: its formula, and the cells' inputs and outputs
    assert!(g.wires.iter().any(|w| w.from == "Dish.reactions.rot" && w.to == "Dish.rules.rotting@react_rot" && w.substance.is_none()));
    assert!(g.wires.iter().any(|w| w.from == "Dish.fields.cells" && w.to == "Dish.rules.rotting@react_rot" && w.substance.as_deref() == Some("body")));
    // a signal rule is a Signal box in Field signals, with no wires
    let smell = g.nodes.iter().find(|n| n.group.is_some() && n.id.starts_with("Dish.rules.smell")).unwrap();
    assert_eq!((smell.kind, smell.step), (Kind::Signal, Some("Field signals")));
    // transfers are placed in Transfers, the reaction in World reactions
    let step = |id: &str| g.nodes.iter().find(|n| n.id == id).unwrap().step;
    assert_eq!(step("Dish.rules.sunlight@transfer_light"), Some("Transfers"));
    assert_eq!(step("Dish.rules.rotting@react_rot"), Some("World reactions"));
}

#[test]
fn a_statement_the_editor_does_not_draw_is_a_code_box() {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/dish.em")).unwrap();
    let src = src.replace("transfer(light, from: sun,", "let a = 1.0;\n        transfer(light, from: sun,");
    let file = emerge_lang::parse(&src).unwrap();
    let g = graph(&file, "Dish").unwrap();
    let sunlight: Vec<_> = g.nodes.iter().filter(|n| n.group.as_deref() == Some("Dish.rules.sunlight")).collect();
    assert_eq!(sunlight.len(), 2, "{sunlight:?}");
    assert_eq!((sunlight[0].kind, sunlight[0].text.as_str()), (Kind::Code, "let a = 1.0;"));
    assert_eq!(sunlight[1].kind, Kind::Transfer);
}

fn count_nested(block: &[emerge_lang::ast::Node<emerge_lang::ast::Stmt>]) -> usize {
    use emerge_lang::ast::Stmt;
    block
        .iter()
        .map(|s| match &s.node {
            Stmt::When { then, otherwise, .. } => {
                let kids = then.len() + otherwise.as_ref().map_or(0, |o| o.len());
                kids + count_nested(then) + otherwise.as_ref().map_or(0, |o| count_nested(o))
            }
            _ => 0,
        })
        .sum::<usize>()
}

/// The graph contract (`proposals/houdini-node-system.md` §5): in every world file of the repository, every wire has a
/// contract that names ports that both of its boxes have, a derived wire says why it has no edit, every box that cannot
/// do something says why, and the statements inside `when` blocks are exactly the ones the source has.
#[test]
fn every_wire_resolves_to_real_ports_and_every_refusal_has_a_reason() {
    use emerge_lang::contract::contract;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    world_files(&root, &mut files);
    let (mut worlds, mut wires, mut nested_seen) = (0, 0, 0);
    for path in files {
        let src = std::fs::read_to_string(&path).unwrap();
        let Ok(file) = emerge_lang::parse(&src) else { continue };
        if emerge_lang::prepare(&file).is_err() {
            continue;
        }
        for item in &file.items {
            let Item::World(w) = &item.node else { continue };
            let g = graph(&file, &w.name).unwrap();
            let c = contract(&file, &w.name).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            worlds += 1;
            assert_eq!(c.wires.len(), g.wires.len());
            for (wire, wc) in g.wires.iter().zip(&c.wires) {
                wires += 1;
                let (a, b) = (&c.nodes[&wire.from], &c.nodes[&wire.to]);
                assert!(a.ports.iter().any(|p| p.key == wc.from_port && p.dir == "out"), "{}: {} has no out port `{}`", path.display(), wire.from, wc.from_port);
                assert!(b.ports.iter().any(|p| p.key == wc.to_port && p.dir == "in"), "{}: {} has no in port `{}`", path.display(), wire.to, wc.to_port);
                assert!(wc.edit.is_some() != wc.why_not.is_some(), "a wire either has an edit or says why not");
            }
            for (id, n) in &c.nodes {
                for op in ["edit_text", "set_param", "delete", "rewire", "duplicate", "label"] {
                    let can = match op {
                        "edit_text" => n.caps.edit_text,
                        "set_param" => n.caps.set_param,
                        "delete" => n.caps.delete,
                        "rewire" => n.caps.rewire,
                        "duplicate" => n.caps.duplicate,
                        _ => n.caps.label,
                    };
                    assert!(can || n.caps.why_not.contains_key(op), "{}: {id} cannot {op} and does not say why", path.display());
                }
            }
            // Nested statements: exactly those of the source, each owned by an existing statement, ids unique.
            let want: usize = w.decls.iter().map(|d| match &d.node { WDecl::Rule { body, .. } => count_nested(body), _ => 0 }).sum();
            assert_eq!(c.nested.len(), want, "{}: nested statements", path.display());
            let mut ids: Vec<&str> = c.nested.iter().map(|n| n.id.as_str()).collect();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), c.nested.len());
            assert!(c.nested.iter().all(|n| c.nodes.contains_key(&n.container)));
            nested_seen += c.nested.len();
        }
    }
    assert!(worlds >= 5 && wires > 20, "worlds {worlds}, wires {wires}");
    assert!(nested_seen > 0, "some world in the repository has a `when` with statements in it");
}
