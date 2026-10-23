//! Reusable operators (`proposals/reusable-operators.md`): a definition and its instances written out into ordinary rules
//! before anything checks the world, equal to the same rules written by hand; every refusal has its code; patches reach an
//! instance's arguments and version.

use emerge_lang::ast::{Item, WDecl};

const OPS: &str = include_str!("../../../examples/operators.em");

fn prepared_rules(src: &str) -> Vec<String> {
    let f = emerge_lang::prepare(&emerge_lang::parse(src).unwrap()).unwrap();
    f.items.iter().filter_map(|i| match &i.node { Item::World(w) => Some(w), _ => None }).flat_map(|w| w.decls.iter()).filter(|d| matches!(d.node, WDecl::Rule { .. })).map(|d| emerge_lang::fmt::decl_text(&d.node)).collect()
}

fn problem(src: &str) -> (&'static str, String) {
    let p = emerge_lang::prepare(&emerge_lang::parse(src).unwrap()).unwrap_err();
    (p.code, p.message)
}

#[test]
fn an_instance_is_its_rules_written_out() {
    // The file is in canonical form and checks.
    let file = emerge_lang::parse(OPS).unwrap();
    assert_eq!(emerge_lang::format(&file), OPS, "emerge fmt keeps the operators file as it is");
    let errors = emerge_lang::check::check(&file);
    assert!(errors.is_empty(), "{errors:?}");
    // The same world written by hand: the operator gone, each instance its rule.
    let hand = OPS.replace(&OPS[OPS.find("    // What one inflow is").unwrap()..OPS.find("    rule spread on cells").unwrap()], "")
        .replace("    use NutrientSupply version 1 as carbon_in(from: rock, into: cells, what: carbon, ration: 20mu);\n    use NutrientSupply version 1 as humus_in(from: rock, into: cells, what: humus);\n",
            "    rule carbon_in_supply on cells {\n        transfer(carbon, from: rock, to: self, amount: 20mu, overflow: void) @supply;\n    }\n\n    rule humus_in_supply on cells {\n        transfer(humus, from: rock, to: self, amount: 5mu, overflow: void) @supply;\n    }\n");
    assert!(!hand.contains("operator NutrientSupply") && !hand.contains("use NutrientSupply"), "{hand}");
    assert_eq!(prepared_rules(OPS), prepared_rules(&hand));
    // The schedule `emerge order` prints is the same.
    let order = |src: &str| emerge_lang::order::order(&emerge_lang::prepare(&emerge_lang::parse(src).unwrap()).unwrap(), "Soil").unwrap();
    assert_eq!(order(OPS), order(&hand));
}

#[test]
fn every_refusal_has_its_code() {
    let swap = |from: &str, to: &str| OPS.replacen(from, to, 1);
    let carbon = "use NutrientSupply version 1 as carbon_in(from: rock, into: cells, what: carbon, ration: 20mu);";
    assert_eq!(problem(&swap(carbon, "use NutrientSupply version 2 as carbon_in(from: rock, into: cells, what: carbon);")).0, "E0114");
    assert_eq!(problem(&swap(carbon, "use Nope version 1 as carbon_in(from: rock, into: cells, what: carbon);")).0, "E0114");
    assert_eq!(problem(&swap(carbon, "use NutrientSupply version 1 as carbon_in(from: rock, into: cells);")).0, "E0116");
    assert_eq!(problem(&swap(carbon, "use NutrientSupply version 1 as carbon_in(from: rock, into: cells, what: carbon, speed: 2mu);")).0, "E0115");
    assert_eq!(problem(&swap(carbon, "use NutrientSupply version 1 as carbon_in(from: rock, into: cells, what: carbon, ration: 20bp);")).0, "E0205");
    assert_eq!(problem(&swap(carbon, "use NutrientSupply version 1 as carbon_in(from: rock, into: cells, what: 3u);")).0, "E0205");
    assert_eq!(problem(&swap("as humus_in(", "as carbon_in(")).0, "E0111", "an instance name is used once");
    assert_eq!(problem(&swap("rule rotting on cells", "rule carbon_in_supply on cells")).0, "E0111", "a written-out rule may not take a name the world has");
    assert_eq!(problem(&swap("param spill: sink = void;", "param spill: sink = void;\n        param spill: sink = void;")).0, "E0111");
    assert_eq!(problem(&swap("param ration: amount = 5mu;", "param ration: weight = 5mu;")).0, "E0205");
    // A name the world lacks is written out as given; the compiler refuses it like any unknown name (the server test).
    assert!(prepared_rules(&swap(carbon, "use NutrientSupply version 1 as carbon_in(from: rock, into: cells, what: unobtainium);")).iter().any(|r| r.contains("transfer(unobtainium")));
}

#[test]
fn patches_reach_instances_and_definitions() {
    let file = emerge_lang::parse(OPS).unwrap();
    let apply = |p: &str| emerge_lang::patch::apply(&file, &emerge_lang::parse(p).unwrap()).map(|f| emerge_lang::format(&f));
    let set = apply("patch Soil { set uses.carbon_in.ration = 30mu; }").unwrap();
    assert!(prepared_rules(&set).iter().any(|r| r.contains("transfer(carbon, from: rock, to: self, amount: 30mu")));
    let spill = apply("patch Soil { set uses.humus_in.spill = void; }").unwrap();
    assert!(spill.contains("as humus_in(from: rock, into: cells, what: humus, spill: void)"), "an argument that was not given is added");
    let gone = apply("patch Soil { remove uses.humus_in; }").unwrap();
    assert!(!prepared_rules(&gone).iter().any(|r| r.contains("humus_in")));
    let v2 = apply("patch Soil { add operator NutrientSupply version 2 { param from: source; param into: field; param what: substance; param ration: amount = 1mu; rule supply on into { transfer(what, from: from, to: self, amount: ration) @supply; } } set uses.carbon_in.version = 2; }").unwrap();
    let rules = prepared_rules(&v2);
    assert!(rules.iter().any(|r| r.contains("amount: 20mu)")) && rules.iter().any(|r| r.contains("amount: 5mu, overflow: void")), "the upgraded instance has version 2's rule, the other keeps version 1's: {rules:?}");
    // A version whose parameters do not fit the instance's arguments is refused when the file is prepared.
    let misfit = apply("patch Soil { add operator NutrientSupply version 3 { param from: source; param into: field; param what: substance; rule supply on into { transfer(what, from: from, to: self, amount: 1mu) @supply; } } set uses.carbon_in.version = 3; }").unwrap();
    assert_eq!(problem(&misfit).0, "E0115");
    assert!(apply("patch Soil { add operator NutrientSupply version 1 { param x: source; rule r on cells { react(rot, rate: 1bp) @r; } } }").is_err(), "a name and version is declared once");
    assert!(apply("patch Soil { remove operators.NutrientSupply@v1; }").is_ok());
    assert!(apply("patch Soil { remove uses.nobody; }").is_err());
}
