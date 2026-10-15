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
