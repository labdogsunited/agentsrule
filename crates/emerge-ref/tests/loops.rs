//! Loops in world rules: a dish written with `for` is the same world as the dish written out.

use emerge_ref::compile::{compile, Options};
use emerge_ref::swap::Live;

const DISH: &str = include_str!("../../../examples/dish.em");

fn with_rule(rule: &str) -> String {
    let at = DISH.find("    rule spread on cells").unwrap();
    format!("{}{rule}\n{}", &DISH[..at], &DISH[at..])
}

const LOOPED: &str = "    rule wind on cells {\n        for d in [above, below] {\n            for s in [carbon, nitrate] {\n                when self.s >= 5u { transfer(s, from: self, to: d, amount: 3mu) @wind; }\n            }\n        }\n        for s in [carbon, nitrate] {\n            transfer(s, from: rock, to: self, amount: 5mu, overflow: void) @rain;\n        }\n    }\n";
const WRITTEN: &str = "    rule wind on cells {\n        when self.carbon >= 5u { transfer(carbon, from: self, to: above, amount: 3mu) @wind_above_carbon; }\n        when self.nitrate >= 5u { transfer(nitrate, from: self, to: above, amount: 3mu) @wind_above_nitrate; }\n        when self.carbon >= 5u { transfer(carbon, from: self, to: below, amount: 3mu) @wind_below_carbon; }\n        when self.nitrate >= 5u { transfer(nitrate, from: self, to: below, amount: 3mu) @wind_below_nitrate; }\n        transfer(carbon, from: rock, to: self, amount: 5mu, overflow: void) @rain_carbon;\n        transfer(nitrate, from: rock, to: self, amount: 5mu, overflow: void) @rain_nitrate;\n    }\n";

fn fingerprint(src: &str, ticks: u64) -> Vec<String> {
    let mut w = compile(src, &["Drifter"], Options { seed: 5, allow_partial: true, mutation: true }).unwrap_or_else(|e| panic!("{e:?}")).world;
    for _ in 0..ticks {
        w.run_tick();
    }
    let mut out = vec![format!("tick {} births {} deaths {} laws {}", w.tick, w.births, w.deaths, w.laws.len())];
    out.extend(w.cells.iter().map(|c| format!("{:?}", c.amounts)));
    for e in &w.entities {
        out.push(format!("{} {:?} {:?} {:?}", e.id, e.pos, e.store.amounts, e.genome));
    }
    out
}

#[test]
fn a_dish_with_loops_runs_exactly_as_the_one_written_out() {
    let (looped, written) = (with_rule(LOOPED), with_rule(WRITTEN));
    let (a, b) = (fingerprint(&looped, 100), fingerprint(&written, 100));
    assert!(a.len() > 100);
    assert!(a[0].ends_with("laws 12"), "6 new statements besides the dish's own: {}", a[0]);
    assert_eq!(a, b);
    assert_ne!(a, fingerprint(DISH, 100), "the rule does something");
}

#[test]
fn a_name_the_world_lacks_is_refused_at_the_loop() {
    let src = with_rule("    rule rain on cells {\n        for s in [carbon, unobtainium] {\n            transfer(s, from: rock, to: self, amount: 5mu, overflow: void) @rain;\n        }\n    }\n");
    let err = compile(&src, &["Drifter"], Options { seed: 5, allow_partial: true, mutation: true }).err().expect("refused");
    let text = format!("{err:?}");
    assert!(text.contains("unobtainium"), "{text}");
}

#[test]
fn a_swap_edits_the_loop_not_one_of_its_copies() {
    let mut src = with_rule(LOOPED);
    let mut live = Live::new(&src, &["Drifter"], Options { seed: 5, allow_partial: true, mutation: true }).unwrap();
    for _ in 0..5 { live.world.run_tick(); }
    // the loop's own label reaches the statement in the loop, so both copies change
    live.hotswap("patch Dish {\n    set rules.wind@rain.amount = 9mu;\n}\n", live.revision).unwrap_or_else(|e| panic!("{e:?}"));
    for _ in 0..20 { live.world.run_tick(); }
    src = src.replace("amount: 5mu, overflow: void) @rain;", "amount: 9mu, overflow: void) @rain;");
    assert!(live.source.contains("9mu"), "{}", live.source);
    // a copy's own label (the names added) is not in the source: refused, and the world is untouched
    let before = live.revision;
    let err = live.hotswap("patch Dish {\n    set rules.wind@rain_nitrate.amount = 7mu;\n}\n", live.revision).err().expect("refused");
    assert!(format!("{err:?}").contains("rain_nitrate"), "{err:?}");
    assert_eq!(live.revision, before);
}
