//! The runner and `emerge order` use one schedule (`proposals/checked-schedule-ir.md` §2.1): for every
//! world file in the repository, the laws the runner builds are the rule statements the schedule puts
//! in Move and Transform, in declaration order, and a state rule is a signal rule. This would have
//! caught `life_like` being reported as "not placed" while the runner ran it in Set.

use emerge_lang::schedule::{lower, Slot};
use emerge_ref::compile::{compile, Options};
use emerge_ref::Law;
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
fn the_laws_the_runner_builds_are_the_statements_the_schedule_places() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    world_files(&root, &mut files);
    let mut checked = 0;
    for path in files {
        let src = std::fs::read_to_string(&path).unwrap();
        let opts = Options { seed: 1, allow_partial: true, mutation: false };
        let Ok(c) = compile(&src, &[], opts) else { continue };
        let file = emerge_lang::prepare(&emerge_lang::parse(&src).unwrap()).unwrap();
        let world = c.world;
        let name = file.items.iter().find_map(|i| match &i.node {
            emerge_lang::ast::Item::World(w) => Some(w.name.clone()),
            _ => None,
        });
        let schedule = lower(&file, &name.unwrap()).unwrap();
        assert_eq!(schedule.creatures, world.creatures, "{}", path.display());

        // Moves and Transforms, as the schedule places them, in declaration order.
        let mut expected = Vec::new();
        for r in schedule.rules.iter().filter(|r| Some(&r.on) == c.field.as_ref()) {
            for st in &r.stmts {
                expected.push(st.slot.clone().unwrap_or_else(|why| panic!("{}: rule {}: {why}", path.display(), r.name)));
            }
        }
        let built: Vec<Slot> = world
            .laws
            .iter()
            .map(|l| match l.parts().0 {
                Law::Inflow { .. } | Law::Shift { .. } | Law::Diffuse { .. } => Slot::Transfer,
                Law::React { .. } => Slot::React,
                Law::When { .. } => unreachable!("parts() unwraps a condition"),
            })
            .collect();
        assert_eq!(built, expected, "{}", path.display());

        // A state rule is a signal rule: placed in Set, and the field has it.
        for r in schedule.rules.iter().filter(|r| world.states.iter().any(|f| f.name == r.on)) {
            assert_eq!(r.slot, Ok(Slot::Signal), "{}: rule {}", path.display(), r.name);
            let f = world.states.iter().find(|f| f.name == r.on).unwrap();
            assert!(f.life_like.is_some(), "{}: rule {} is placed but the runner has no rule on `{}`", path.display(), r.name, f.name);
        }
        checked += 1;
    }
    assert!(checked >= 6, "only {checked} world files compiled");
}
