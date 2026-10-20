//! `examples/dish.em` is the parser fixture (spec §16, exit criterion 1): it parses,
//! formatting it changes nothing, and parse → format → parse gives the same tree.

const DISH: &str = include_str!("../../../examples/dish.em");

#[test]
fn dish_parses_and_is_canonical() {
    let file = emerge_lang::parse(DISH).unwrap_or_else(|d| panic!("{d}"));
    let formatted = emerge_lang::format(&file);
    if formatted != DISH {
        let diff: Vec<_> = DISH
            .lines()
            .zip(formatted.lines())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .take(5)
            .map(|(n, (a, b))| format!("line {}:\n  file: {a}\n  fmt:  {b}", n + 1))
            .collect();
        panic!("examples/dish.em is not in canonical form:\n{}", diff.join("\n"));
    }
    assert_eq!(emerge_lang::parse(&formatted).unwrap(), file);
}

#[test]
fn dish_passes_the_chemistry_checks() {
    let file = emerge_lang::parse(DISH).unwrap();
    let problems = emerge_lang::check::check(&file);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn fmt_puts_reaction_terms_in_canonical_order() {
    // `build` lists its inputs against declaration order (energy, carbon, nitrate).
    let src = DISH.replacen("reaction build : energy + carbon + nitrate", "reaction build : nitrate + carbon + energy", 1);
    assert_ne!(src, DISH);
    let file = emerge_lang::parse(&src).unwrap();
    assert!(emerge_lang::check::check(&file).iter().any(|p| p.code == "E0404"));
    let once = emerge_lang::format(&file);
    assert_eq!(once, DISH);
    assert_eq!(emerge_lang::format(&emerge_lang::parse(&once).unwrap()), once);
}

#[test]
fn fmt_leaves_a_reaction_with_an_undeclared_substance_alone() {
    let src = DISH.replacen("reaction rot : body -> carbon + nitrate", "reaction rot : body -> nitrate + carbon + mud", 1);
    let out = emerge_lang::format(&emerge_lang::parse(&src).unwrap());
    assert!(out.contains("reaction rot : body -> nitrate + carbon + mud"));
}
