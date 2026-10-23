//! `for NAME in [a, b] { ... }` in a world rule: the body written once for each name (`language.md` §7.5).

use emerge_lang::{format, parse, prepare};

const DISH: &str = include_str!("../../../examples/dish.em");

fn with_rule(rule: &str) -> String {
    let at = DISH.find("    rule spread on cells").unwrap();
    format!("{}{rule}\n{}", &DISH[..at], &DISH[at..])
}

/// The text of the prepared file (loops unrolled), for comparing two spellings of one world.
fn prepared_text(src: &str) -> String {
    let file = parse(src).unwrap_or_else(|d| panic!("{d}"));
    format(&prepare(&file).unwrap_or_else(|p| panic!("{}: {}", p.code, p.message)))
}

fn problem(src: &str) -> (&'static str, String) {
    let file = parse(src).unwrap_or_else(|d| panic!("{d}"));
    let p = prepare(&file).expect_err("should be refused");
    (p.code, p.message)
}

#[test]
fn a_loop_is_the_statements_written_out() {
    let looped = with_rule("    rule rain on cells {\n        for s in [carbon, nitrate] {\n            transfer(s, from: rock, to: self, amount: 5mu, overflow: void) @rain;\n        }\n    }\n");
    let written = with_rule("    rule rain on cells {\n        transfer(carbon, from: rock, to: self, amount: 5mu, overflow: void) @rain_carbon;\n        transfer(nitrate, from: rock, to: self, amount: 5mu, overflow: void) @rain_nitrate;\n    }\n");
    assert_eq!(prepared_text(&looped), prepared_text(&written));
}

#[test]
fn the_name_reaches_every_place_a_name_can_stand_and_loops_nest() {
    // in a condition (`self.s`), a direction (`to: d`), under a `when`, and in a second loop
    let looped = with_rule("    rule wind on cells {\n        for d in [above, below] {\n            for s in [carbon, nitrate] {\n                when self.s >= 5u { transfer(s, from: self, to: d, amount: 3mu) @wind; }\n            }\n        }\n    }\n");
    let written = with_rule("    rule wind on cells {\n        when self.carbon >= 5u { transfer(carbon, from: self, to: above, amount: 3mu) @wind_above_carbon; }\n        when self.nitrate >= 5u { transfer(nitrate, from: self, to: above, amount: 3mu) @wind_above_nitrate; }\n        when self.carbon >= 5u { transfer(carbon, from: self, to: below, amount: 3mu) @wind_below_carbon; }\n        when self.nitrate >= 5u { transfer(nitrate, from: self, to: below, amount: 3mu) @wind_below_nitrate; }\n    }\n");
    // a label gets the outer name first, then the inner
    assert_eq!(prepared_text(&looped), prepared_text(&written));
}

#[test]
fn fmt_keeps_the_loop_and_round_trips() {
    let looped = with_rule("    rule rain on cells {\n        for s in [carbon, nitrate] {\n            transfer(s, from: rock, to: self, amount: 5mu, overflow: void) @rain;\n        }\n    }\n");
    let once = format(&parse(&looped).unwrap());
    assert!(once.contains("for s in [carbon, nitrate] {"), "{once}");
    assert_eq!(format(&parse(&once).unwrap()), once);
}

#[test]
fn a_loop_that_cannot_run_says_why() {
    let rule = |head: &str| with_rule(&format!("    rule rain on cells {{\n        {head} {{\n            transfer(s, from: rock, to: self, amount: 5mu, overflow: void) @rain;\n        }}\n    }}\n"));
    assert_eq!(problem(&rule("for s in [carbon, carbon]")).0, "E0120");
    assert_eq!(problem(&rule("for carbon in [nitrate]")).0, "E0111");
    assert_eq!(problem(&rule("for tick in [carbon]")).0, "E0110");
    // an empty list does not parse: a loop needs a name to go through
    assert_eq!(parse(&rule("for s in []")).unwrap_err().code, "E0001");
}
