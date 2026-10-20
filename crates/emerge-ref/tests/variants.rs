//! Variants by patch (`proposals/world-variants.md`): three hand-made copies of
//! `examples/dish.em` are rebuilt from it plus a patch file of at most 15 operations. The
//! patched tree prints as the copy's text, and a run of it is the run of the copy.

use emerge_lang::ast::{Item, PatchOp};
use emerge_ref::compile::{build, Options};

const DISH: &str = include_str!("../../../examples/dish.em");
const TEST9: &str = include_str!("../../../archive/test9-dish.em");
const CATALYST: &str = include_str!("../../../archive/catalyst-dish.em");
const CHEAP: &str = include_str!("../../../archive/cheap-dish.em");

fn patched(base: &str, patch: &str) -> emerge_lang::ast::File {
    let patch = emerge_lang::parse(patch).unwrap_or_else(|d| panic!("{d}"));
    emerge_lang::patch::apply(&emerge_lang::parse(base).unwrap(), &patch).unwrap_or_else(|p| panic!("{p:?}"))
}

fn operations(patch: &str) -> usize {
    let file = emerge_lang::parse(patch).unwrap();
    file.items
        .iter()
        .map(|i| match &i.node {
            Item::Patch(p) => p.ops.len(),
            _ => 0,
        })
        .sum()
}

fn fingerprint(file: &emerge_lang::ast::File, ticks: u64) -> Vec<String> {
    let options = Options { seed: 3, allow_partial: true, mutation: true };
    let mut w = build(file, &["Drifter", "Builder"], options).unwrap_or_else(|e| panic!("{e:?}")).world;
    for _ in 0..ticks {
        w.run_tick();
    }
    let mut out = vec![format!("tick {} births {} deaths {}", w.tick, w.births, w.deaths)];
    for e in &w.entities {
        out.push(format!("{} {:?} {:?} {:?}", e.id, e.pos, e.store.amounts, e.genome));
    }
    out
}

fn same_variant(copy: &str, patch: &str) {
    assert!(operations(patch) <= 15);
    let by_patch = patched(DISH, patch);
    let by_hand = emerge_lang::parse(copy).unwrap();
    assert_eq!(emerge_lang::format(&by_patch), emerge_lang::format(&by_hand));
    let (a, b) = (fingerprint(&by_patch, 150), fingerprint(&by_hand, 150));
    assert!(a.len() > 100, "the run should keep a population");
    assert_eq!(a, b);
}

#[test]
fn the_test9_dish_is_the_base_plus_a_patch() {
    same_variant(TEST9, include_str!("variants/test9.patch.em"));
}

#[test]
fn the_catalyst_dish_is_the_base_plus_a_patch() {
    same_variant(CATALYST, include_str!("variants/catalyst.patch.em"));
}

#[test]
fn the_cheap_dish_is_the_base_plus_a_patch() {
    same_variant(CHEAP, include_str!("variants/cheap.patch.em"));
}

fn fails(patch: &str) -> emerge_lang::check::Problem {
    let patch = emerge_lang::parse(patch).unwrap();
    emerge_lang::patch::apply(&emerge_lang::parse(DISH).unwrap(), &patch).unwrap_err()
}

#[test]
fn a_bad_path_fails_with_a_code_and_a_position() {
    let p = fails("patch Dish {\n    set max_age = 1000;\n    set rules.sunlight@no_such_label.amount = 1mu;\n}\n");
    assert_eq!((p.code, p.pos.line), ("E0114", 3));
    assert_eq!(p.path, "Dish.rules.sunlight@no_such_label.amount");
    assert_eq!(fails("patch Nope { set max_age = 1; }\n").code, "E0114");
    assert_eq!(fails("patch Dish { set upkeep.nothing = 1mu; }\n").code, "E0114");
    assert_eq!(fails("patch Dish { set decompose = rot; }\n").code, "E0114"); // not there: use `add`
}

#[test]
fn wrong_values_duplicates_and_unsupported_operations_are_errors() {
    assert_eq!(fails("patch Dish { set actions.steal.cost = 1.5; }\n").code, "E0205");
    assert_eq!(fails("patch Dish { add max_age: 5; }\n").code, "E0111");
    assert_eq!(fails("patch Dish { add after actions.steal: action take(s: any substance) cost 1mu; }\n").code, "E0111");
    assert_eq!(fails("patch Dish { remove rules.rotting; }\n").code, "E0904");
    assert_eq!(fails("patch Drifter { add after seed.forage: action move(d: 0u..1u) @m; }\n").code, "E0904");
    assert_eq!(fails("world X {\n}\n").code, "E0903");
}

#[test]
fn applying_a_patch_leaves_the_base_alone() {
    let before = emerge_lang::parse(DISH).unwrap();
    let _ = patched(DISH, include_str!("variants/test9.patch.em"));
    assert_eq!(emerge_lang::format(&before), DISH);
}

#[test]
fn set_makes_a_one_operation_patch() {
    let file = emerge_lang::patch::parse_set("Dish.rules.rotting@react_rot.rate=500bp").unwrap();
    let Item::Patch(p) = &file.items[0].node else { panic!() };
    assert!(matches!(p.ops[0].node, PatchOp::Set { .. }));
    let out = emerge_lang::patch::apply(&emerge_lang::parse(DISH).unwrap(), &file).unwrap();
    assert!(emerge_lang::format(&out).contains("react(rot, rate: 500bp) @react_rot;"));
    assert!(emerge_lang::patch::parse_set("no equals sign").is_err());
}
