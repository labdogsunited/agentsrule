//! The static checks: each problem is reported with its code, path and position.

use emerge_lang::check::{check, Problem};
use emerge_lang::parse;

const DISH: &str = include_str!("../../../examples/dish.em");

/// The problems in `dish.em` with each `(from, to)` edit applied to its first occurrence.
fn problems(edits: &[(&str, &str)]) -> Vec<Problem> {
    let mut src = DISH.to_string();
    for (from, to) in edits {
        assert!(src.contains(from), "`{from}` is not in dish.em");
        src = src.replacen(from, to, 1);
    }
    check(&parse(&src).unwrap_or_else(|d| panic!("{d}")))
}

fn line_of(src_edits: &[(&str, &str)], needle: &str) -> usize {
    let mut src = DISH.to_string();
    for (from, to) in src_edits {
        src = src.replacen(from, to, 1);
    }
    src.lines().position(|l| l.contains(needle)).expect("needle") + 1
}

#[test]
fn dish_has_no_problems() {
    assert_eq!(problems(&[]), []);
}

#[test]
fn names_are_unique_across_kinds() {
    let edit = [("substance body = C + N potential 5;", "substance body = C + N potential 5;\n    substance rot = C potential 0;")];
    let p = problems(&edit);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!((p[0].code, p[0].path.as_str()), ("E0111", "Dish.reactions.rot"));
    assert_eq!(p[0].pos.line, line_of(&edit, "reaction rot :"));
}

#[test]
fn signals_must_not_take_world_names_or_reserved_forms() {
    let p = problems(&[("-> steer @weigh_steer", "-> sugar @weigh_steer"), ("<- steer threshold", "<- sugar threshold")]);
    assert!(p.iter().any(|p| p.code == "E0112" && p.message.contains("`sugar`")), "{p:?}");
    let p = problems(&[("-> steer @weigh_steer", "-> s7 @weigh_steer"), ("<- steer threshold", "<- s7 threshold")]);
    assert!(p.iter().any(|p| p.code == "E0113"), "{p:?}");
    let p = problems(&[("module forage {", "module m2 {")]);
    assert!(p.iter().any(|p| p.code == "E0113" && p.path == "Drifter.seed.m2"), "{p:?}");
}

#[test]
fn a_signal_that_is_read_must_be_written() {
    let edit = [("<- [fullness] -> ready", "<- [fulness] -> ready")];
    let p = problems(&edit);
    let e = p.iter().find(|p| p.code == "E0605").unwrap_or_else(|| panic!("{p:?}"));
    assert_eq!(e.path, "Drifter.seed.reproduce@weigh_ready");
    assert_eq!(e.message, "signal `fulness` is read but no gene writes it");
    assert_eq!(e.pos.line, line_of(&edit, "[fulness]"));
}

#[test]
fn the_seed_fits_its_profile() {
    let p = problems(&[("genes: 32;", "genes: 11;")]);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].code, "E0801");
    assert_eq!(p[0].message, "the seed has 12 genes, more than profile `fast` allows (11)");
}

#[test]
fn upkeep_base_is_not_free() {
    let p = problems(&[("base: 20mu;", "base: 0mu;")]);
    assert_eq!((p.len(), p[0].code, p[0].path.as_str()), (1, "E0702", "Dish.upkeep.base"));
}

#[test]
fn min_drop_is_at_least_one() {
    for bad in ["second_law { min_drop: 0; }", "second_law { min_drop: -5; }"] {
        let p = problems(&[("second_law { min_drop: 1; }", bad)]);
        assert_eq!(p.len(), 1, "{bad}: {p:?}");
        assert_eq!((p[0].code, p[0].path.as_str()), ("E0203", "Dish.second_law.min_drop"), "{bad}");
    }
    let p = problems(&[("second_law { min_drop: 1; }", "second_law { min_drop: 2; }")]);
    assert!(p.iter().all(|p| p.code == "E0402"), "2 is a valid setting; only reactions too shallow for it fail: {p:?}");
}

#[test]
fn every_problem_is_reported_not_only_the_first() {
    let p = problems(&[("base: 20mu;", "base: 0mu;"), ("genes: 32;", "genes: 11;"), ("<- [fullness] -> ready", "<- [fulness] -> ready")]);
    let codes: Vec<_> = p.iter().map(|p| p.code).collect();
    assert_eq!(codes, ["E0702", "E0605", "E0801"]);
}

#[test]
fn a_signal_field_with_several_writers_declares_how_they_combine() {
    let second = ("rule smell on scent {", "rule wind on scent {\n        self.value = 0.0;\n    }\n\n    rule smell on scent {");
    let p = problems(&[second]);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!((p[0].code, p[0].path.as_str()), ("E0305", "Dish.rules.smell"));

    let combined = ("field scent : Grid(w: 256, h: 256) signal;", "field scent : Grid(w: 256, h: 256) signal combine max;");
    assert_eq!(problems(&[second, combined]), []);

    let wrong = ("field scent : Grid(w: 256, h: 256) signal;", "field scent : Grid(w: 256, h: 256) signal combine any;");
    assert_eq!(problems(&[wrong])[0].code, "E0305");
}

/// `dish.em` with a lineage appended, and the codes of the problems it gives.
fn codes_with(extra: &str) -> Vec<&'static str> {
    let src = format!("{DISH}\n{extra}");
    check(&parse(&src).unwrap_or_else(|d| panic!("{d}"))).iter().map(|p| p.code).collect()
}

const EXTRA_GENE: &str = "reaction build rate 50mu @extra";

fn child(body: &str) -> String {
    format!("lineage Kid extends Drifter in Dish {{\n    seed genome {{\n        module metabolism {{\n            {body};\n        }}\n    }}\n}}\n")
}

#[test]
fn extends_is_checked_in_full() {
    assert_eq!(codes_with(&child(EXTRA_GENE)), Vec::<&str>::new());
    assert_eq!(codes_with(&child(&format!("{EXTRA_GENE} after @respiration"))), Vec::<&str>::new());
}

#[test]
fn extends_reports_a_bad_base_label_or_position() {
    assert_eq!(codes_with(&child("reaction build rate 50mu @extra").replace("Drifter", "Nobody")), ["E0114"]);
    assert_eq!(codes_with(&child(&format!("{EXTRA_GENE} after @nothing"))), ["E0114"]);
    assert_eq!(codes_with(&child("reaction build rate 50mu @eat_light")), ["E0118"]);
    assert_eq!(codes_with("lineage Kid2 extends Kid in Dish {}\n").first(), Some(&"E0114"));
    let chained = format!("{}\nlineage Kid2 extends Kid in Dish {{}}\n", child(EXTRA_GENE));
    assert_eq!(codes_with(&chained), ["E0117"]);
}

#[test]
fn after_needs_a_lineage_that_extends() {
    let plain = child(&format!("{EXTRA_GENE} after @respiration")).replace(" extends Drifter", "");
    assert_eq!(codes_with(&plain), ["E0118"]);
}

#[test]
fn extends_formats_and_parses_back() {
    let src = format!("{DISH}\n{}", child(&format!("{EXTRA_GENE} after @respiration")));
    let file = parse(&src).unwrap();
    let text = emerge_lang::format(&file);
    assert!(text.contains("lineage Kid extends Drifter in Dish {"));
    assert!(text.contains("@extra after @respiration;"));
    assert_eq!(parse(&text).unwrap(), file);
}
