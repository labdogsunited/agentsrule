//! The optimistic closure of `reach` (proposals/mathematical-tools-for-ai.md §3).

use emerge_ref::compile::{compile, Options};
use emerge_ref::reach::reach;

/// A fueled cycle: `fuel + a -> 2 b + spent` and `fuel + b -> 2 a + spent`. Fuel holds the
/// carbon `a` and `b` need, but each reaction needs one of them first.
fn world(seed_a: bool, generate: bool) -> String {
    let space = if generate {
        "include: [ab, ba];\n        generate: balanced(max_inputs: 1, max_outputs: 2, max_coeff: 1);"
    } else {
        "include: [ab, ba];"
    };
    let a = if seed_a { "\n        transfer(a, from: rock, to: self, amount: 1u) @transfer_a;" } else { "" };
    format!(
        r#"world Pot {{
    space: Continuous2D(w: 1.0, h: 1.0, wrap: true, max_interaction_radius: 1.0);

    element C;
    element E;

    second_law {{ min_drop: 1; }}

    substance fuel = C + E potential 3;
    substance a = C potential 1;
    substance b = C potential 1;
    substance spent = E potential 0;
    substance energy = E potential 8;
    substance heat = E potential 0;

    field cells : Grid(w: 1, h: 1) holds [fuel: 100u];

    source rock : [fuel, a];
    source spark : [energy];
    sink void : any;

    waste spent -> void;
    waste heat -> void;
    remains: void;

    reaction ab : fuel + a -> 2 b + spent;
    reaction ba : fuel + b -> 2 a + spent;

    reaction_space {{
        {space}
    }}

    upkeep {{
        pay: energy;
        becomes: heat;
        vent: void;
        base: 1mu;
        per_gene: 0mu;
        per_signal: 0mu;
        per_carried_u: 0mu;
    }}

    limits {{
        max_entities: 10;
        inventory_cap: 100u;
    }}

    max_age: 1_000;
    child_offset: 0.5;
    mind_steps: 1;
}}

lineage Cell in Pot {{
    seed genome {{
        module main {{
            reaction ab rate 1u @ab;
        }}
    }}

    genesis {{
        count: 1;
        position = uniform(x: 0.5..0.5, y: 0.5..0.5);
        transfer(energy, from: spark, to: self, amount: 10u) @transfer_energy;
        transfer(fuel, from: rock, to: self, amount: 10u) @transfer_fuel;{a}
    }}
}}
"#
    )
}

fn run(seed_a: bool, generate: bool) -> (Vec<String>, usize, Vec<(String, String)>) {
    let c = compile(&world(seed_a, generate), &["Cell"], Options::default()).expect("compiles");
    let w = &c.world;
    let r = reach(w);
    let name = |s: usize| w.chem.substances[s].name.clone();
    let never = (0..r.possible.len()).filter(|&s| !r.possible[s]).map(name).collect();
    let witness = r.witness.iter().map(|&(i, s)| (w.chem.formula(&w.chem.reactions[i].inputs, &w.chem.reactions[i].outputs), name(s))).collect();
    (never, r.dead.len(), witness)
}

#[test]
fn an_empty_cycle_can_never_start_whatever_the_fuel() {
    let (never, dead, witness) = run(false, false);
    // `spent` too: only the cycle makes it.
    assert_eq!(never, ["a", "b", "spent"]);
    assert_eq!(dead, 2);
    // Each reaction that makes a member of the set needs a member of it.
    assert_eq!(witness.len(), 2);
    assert!(witness.iter().all(|(_, s)| s == "a" || s == "b"));
}

#[test]
fn one_seeded_member_opens_the_cycle() {
    let (never, dead, witness) = run(true, false);
    assert!(never.is_empty());
    assert_eq!(dead, 0);
    assert!(witness.is_empty());
}

#[test]
fn a_generated_reaction_can_bypass_the_cycle() {
    // `generate` adds `fuel -> a + spent` and `fuel -> b + spent`, so the set is no
    // longer empty for good: the check must read the compiled reaction space.
    let (never, _, _) = run(false, true);
    assert!(never.is_empty());
}
