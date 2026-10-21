//! The creature capability (`proposals/checked-schedule-ir.md` §3, milestone 1): a world needs
//! `upkeep` and `remains` only when it has creatures, and then it needs both.

use emerge_ref::compile::{compile, Options};

const DISH: &str = include_str!("../../../examples/dish.em");
const LIFE: &str = include_str!("../../../examples/life.em");

fn opts() -> Options {
    Options { seed: 1, allow_partial: true, mutation: false }
}

#[test]
fn a_state_automaton_needs_no_chemistry_upkeep_or_remains() {
    let mut w = compile(LIFE, &[], opts()).unwrap_or_else(|e| panic!("{e}")).world;
    assert!(!w.creatures);
    assert_eq!(w.chem.substances.len(), 0);
    let alive = |w: &emerge_ref::World| w.states[0].cells.iter().filter(|&&c| c != 0).count();
    let before = alive(&w);
    assert!(before > 0);
    for _ in 0..40 {
        w.step().unwrap();
    }
    assert_ne!(alive(&w), before, "the glider gun changes the field");
    w.audit().unwrap();
}

#[test]
fn a_world_with_a_lineage_still_needs_upkeep_and_remains() {
    // The dish has lineages, so it has creatures, whichever lineages a run selects.
    for (what, src) in [
        ("upkeep", cut(DISH, "    upkeep {", "    }\n")),
        ("remains", DISH.replacen("    remains: void;\n", "", 1)),
    ] {
        for selected in [&["Drifter"][..], &[]] {
            let e = compile(&src, selected, opts()).err().unwrap_or_else(|| panic!("{what}: compiled"));
            assert_eq!(e.code, "E0116", "{what}: {e}");
            assert!(e.message.contains(what), "{e}");
        }
    }
}

#[test]
fn a_menu_or_upkeep_alone_makes_a_creature_world() {
    // No lineage, but the world declares `upkeep`: it must declare `remains` too.
    let src = LIFE.replacen("    rule conway", "    upkeep {\n        pay: energy;\n        becomes: heat;\n        vent: void;\n        base: 1mu;\n    }\n\n    rule conway", 1);
    let src = src.replacen("    field life", "    element E;\n    second_law { min_drop: 1; }\n    substance energy = E potential 1;\n    substance heat = E potential 0;\n    sink void : any;\n    field life", 1);
    let e = compile(&src, &[], opts()).err().expect("upkeep without remains");
    assert_eq!(e.code, "E0116", "{e}");
}

#[test]
fn a_world_without_creatures_refuses_entities() {
    let w = compile(LIFE, &[], opts()).unwrap().world;
    let mut target = w.clone();
    let other = compile(DISH, &["Drifter"], opts()).unwrap().world;
    let e = target.inject(other, 0).err().expect("injection is refused");
    assert!(e.contains("no creature capability"), "{e}");
    assert!(target.entities.is_empty());
}

#[test]
#[should_panic(expected = "no creature capability")]
fn spawning_into_a_world_without_creatures_is_a_programming_error() {
    let mut w = compile(LIFE, &[], opts()).unwrap().world;
    w.spawn(0, vec![]);
}

/// `src` without the block that starts at the line `start` and ends at the first line `end`.
fn cut(src: &str, start: &str, end: &str) -> String {
    let a = src.find(start).expect("block start");
    let b = a + src[a..].find(end).expect("block end") + end.len();
    format!("{}{}", &src[..a], &src[b..])
}
