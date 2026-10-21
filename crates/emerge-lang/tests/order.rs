//! `emerge order` (`order.rs`): the built-in steps are the numbered steps of spec §7.1 to
//! §7.5, in the same order, and each world rule of `examples/dish.em` lands in the phase
//! its effect gives it (spec §7.4).

use emerge_lang::order::{order, step_names};

const DISH: &str = include_str!("../../../examples/dish.em");

fn report(src: &str, world: &str) -> String {
    let file = emerge_lang::prepare(&emerge_lang::parse(src).unwrap()).unwrap();
    order(&file, world).unwrap()
}

/// Bold titles that start a step in spec §7.1 to §7.5: `1. **Senses.**` or `**Venting.**`.
fn spec_steps() -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let specs: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("spec-v") && n.ends_with(".md"))
        .collect();
    assert_eq!(specs.len(), 1, "expected one spec in the repo root, found {specs:?}");
    let text = std::fs::read_to_string(root.join(&specs[0])).unwrap();
    let start = text.find("### 7.1 Set").expect("spec §7.1");
    let end = text.find("### 7.4 World rules").expect("spec §7.4");
    let restructure = text.find("### 7.5 Restructure").expect("spec §7.5");
    let after = text.find("## 8. Evolution").expect("spec §8");
    let mut names = Vec::new();
    for part in [&text[start..end], &text[restructure..after]] {
        for line in part.lines() {
            let rest = line.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches(". ");
            if line.starts_with(' ') || !rest.starts_with("**") {
                continue;
            }
            let title = rest[2..].split("**").next().unwrap();
            names.push(title.trim_end_matches('.').to_string());
        }
    }
    names
}

#[test]
fn built_in_steps_match_the_spec() {
    assert_eq!(step_names(), spec_steps());
}

#[test]
fn dish_rules_land_in_their_phases() {
    let r = report(DISH, "Dish");
    let at = |needle: &str| r.find(needle).unwrap_or_else(|| panic!("missing `{needle}` in\n{r}"));
    let (signals, transfers, movement) = (at("Field signals"), at("3. Transfers"), at("Movement"));
    let (world_rx, venting, disposal) = (at("World reactions"), at("Venting"), at("Disposal"));
    assert!(signals < at("rule smell") && at("rule smell") < at("Move\n"));
    for rule in ["rule sunlight", "rule weather", "rule spread"] {
        assert!(transfers < at(rule) && at(rule) < movement, "{rule}");
    }
    assert!(at("rule sunlight") < at("rule weather") && at("rule weather") < at("rule spread"));
    assert!(world_rx < at("rule rotting") && at("rule rotting") < venting);
    assert!(at("waste heat -> void") > venting && at("waste heat -> void") < disposal);
    assert!(at("remains: void") > disposal);
    // Take (a Move step) runs before rotting (Transform), and disposal comes after.
    assert!(transfers < world_rx && world_rx < disposal);
}

#[test]
fn a_rule_that_mixes_kinds_is_placed_by_statement() {
    // The runner builds a law for each statement of such a rule, so the report lists each statement
    // under its own step (decision 7).
    let src = DISH.replace(
        "react(rot, rate: 50bp) @react_rot;",
        "react(rot, rate: 50bp) @react_rot;\n        diffuse_conserved([sugar], rate: 300bp) @diffuse_conserved;",
    );
    let r = report(&src, "Dish");
    assert!(!r.contains("not placed"), "{r}");
    let (transfers, world_rx, venting) = (r.find("Transfers").unwrap(), r.find("World reactions").unwrap(), r.find("Venting").unwrap());
    let rotting: Vec<usize> = r.match_indices("rule rotting on cells").map(|(i, _)| i).collect();
    assert_eq!(rotting.len(), 2, "{r}");
    assert!(transfers < rotting[0] && rotting[0] < world_rx && world_rx < rotting[1] && rotting[1] < venting, "{r}");
    // Each listing holds only the statements that run in that step.
    assert!(r[rotting[0]..world_rx].contains("diffuse_conserved") && !r[rotting[0]..world_rx].contains("react(rot"), "{r}");
    assert!(r[rotting[1]..venting].contains("react(rot") && !r[rotting[1]..venting].contains("diffuse_conserved"), "{r}");
}

#[test]
fn an_unknown_world_is_an_error() {
    let file = emerge_lang::parse(DISH).unwrap();
    assert_eq!(order(&file, "Nope").unwrap_err(), "no world `Nope`");
}

#[test]
fn decompose_is_listed_under_disposal() {
    let src = DISH.replace("remains: void;", "remains: void;\n    decompose: rot;");
    let r = report(&src, "Dish");
    let d = r.find("decompose: rot").unwrap_or_else(|| panic!("{r}"));
    assert!(r.find("Disposal").unwrap() < d && d < r.find("Division").unwrap());
}

#[test]
fn a_life_like_rule_is_placed_in_field_signals() {
    // Spec §7.4: a state rule is a signal rule, so it runs in Set under "Field signals".
    let life = include_str!("../../../examples/life.em");
    let r = report(life, "Life");
    assert!(!r.contains("not placed"), "{r}");
    let (signals, conway) = (r.find("Field signals").unwrap(), r.find("rule conway").unwrap());
    assert!(signals < conway && conway < r.find("Transfers").unwrap(), "{r}");
}

#[test]
fn a_world_without_creatures_has_no_creature_steps() {
    // checked-schedule-ir.md §3: no upkeep, disposal or entity passes are scheduled for a state automaton.
    let r = report(include_str!("../../../examples/life.em"), "Life");
    for gone in ["Senses", "Upkeep", "Movement", "Disposal", "Division", "Compaction"] {
        assert!(!r.contains(gone), "{gone} is scheduled for a world without creatures:\n{r}");
    }
    let dish = report(DISH, "Dish");
    for kept in ["Senses", "Upkeep", "Movement", "Disposal", "Division", "Compaction"] {
        assert!(dish.contains(kept), "{kept}");
    }
}

#[test]
fn every_step_says_what_it_reads_and_how_it_settles() {
    let r = report(DISH, "Dish");
    assert_eq!(r.lines().filter(|l| l.starts_with("       reads ")).count(), emerge_lang::order::step_names().len(), "{r}");
    assert!(r.contains("settles: two-stage arbitration"), "{r}");
}
