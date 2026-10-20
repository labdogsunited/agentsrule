//! Constants and constant arithmetic: `resolve` replaces and folds them (`language.md` §3.4).

use emerge_lang::ast::*;
use emerge_lang::{parse, resolve};

/// Resolves `world W { <decls> max_age: <value>; }` and returns the `max_age` expression.
fn value(decls: &str, value: &str) -> Result<Expr, (&'static str, String)> {
    let src = format!("world W {{\n{decls}\n    max_age: {value};\n}}\n");
    let file = parse(&src).unwrap_or_else(|d| panic!("{d}"));
    let out = resolve(&file).map_err(|p| (p.code, p.message))?;
    let Item::World(w) = &out.items[0].node else { panic!() };
    w.decls.iter().find_map(|d| match &d.node {
        WDecl::Setting(Entry { key, value: Value::Expr(e) }) if key == "max_age" => Some(e.clone()),
        _ => None,
    }).ok_or(("none", String::new()))
}

fn lit(l: Lit) -> Expr {
    Expr::Lit(l)
}

fn neg(l: Lit) -> Expr {
    Expr::Neg(Box::new(Expr::Lit(l)))
}

#[test]
fn integers_fold_and_saturate() {
    assert_eq!(value("", "10_000 + 10_000"), Ok(lit(Lit::Int(20_000))));
    assert_eq!(value("", "2 + 3 * 4"), Ok(lit(Lit::Int(14))));
    assert_eq!(value("", "(2 + 3) * 4"), Ok(lit(Lit::Int(20))));
    assert_eq!(value("", "5 - 8"), Ok(neg(Lit::Int(3))));
    assert_eq!(value("", "7 / 2"), Ok(lit(Lit::Int(3))));
    assert_eq!(value("", "7 / 0"), Ok(lit(Lit::Int(0))));
    assert_eq!(value("", "2_000_000_000 + 2_000_000_000"), Ok(lit(Lit::Int(i32::MAX as i64))));
}

#[test]
fn amounts_and_rates_follow_the_numeric_contract() {
    assert_eq!(value("", "1u + 250mu"), Ok(lit(Lit::Amount(1_250))));
    assert_eq!(value("", "1u - 2u"), Ok(lit(Lit::Amount(0))));
    assert_eq!(value("", "1u * 50bp"), Ok(lit(Lit::Amount(5))));
    assert_eq!(value("", "999mu * 50bp"), Ok(lit(Lit::Amount(4))));
    assert_eq!(value("", "50bp * 1u"), Ok(lit(Lit::Amount(5))));
    assert_eq!(value("", "1u / 3"), Ok(lit(Lit::Amount(333))));
    assert_eq!(value("", "1u / 0"), Ok(lit(Lit::Amount(0))));
}

#[test]
fn floats_are_f32_operation_by_operation() {
    let Ok(Expr::Lit(Lit::Float(s))) = value("", "0.1 + 0.2") else { panic!() };
    assert_eq!(s.parse::<f32>().unwrap(), 0.1f32 + 0.2f32);
    let Ok(Expr::Lit(Lit::Float(s))) = value("", "1.0 / 0.0") else { panic!() };
    assert_eq!(s.parse::<f32>().unwrap(), 0.0);
    assert!(s.contains('.') || s.contains('e'));
}

#[test]
fn what_the_language_does_not_define_is_left_alone() {
    let mixed = value("", "1u + 2").unwrap();
    assert!(matches!(mixed, Expr::Binary(BinOp::Add, ..)));
    assert!(matches!(value("", "self.x + (1 + 2)").unwrap(), Expr::Binary(BinOp::Add, _, b) if *b == lit(Lit::Int(3))));
}

#[test]
fn constants_resolve_in_any_order() {
    let decls = "    const a = b * 2;\n    const b = c + 1;\n    const c = 4;";
    assert_eq!(value(decls, "a"), Ok(lit(Lit::Int(10))));
}

#[test]
fn a_cycle_and_a_non_constant_are_reported() {
    assert_eq!(value("    const a = b;\n    const b = a;", "1").unwrap_err().0, "E0119");
    assert_eq!(value("    const a = nothing;", "1").unwrap_err().0, "E0205");
    assert_eq!(value("    const a = 1u + 2;", "1").unwrap_err().0, "E0205");
}

#[test]
fn a_constant_cannot_share_a_name_with_another_declaration() {
    let decls = "    element C;\n    substance carbon = C potential 0;\n    const carbon = 1;";
    assert_eq!(value(decls, "1").unwrap_err().0, "E0111");
    assert_eq!(value("    const a = 1;\n    const a = 2;", "1").unwrap_err().0, "E0111");
}

#[test]
fn fmt_keeps_constants_and_expressions() {
    let src = "world W {\n    const side = 2 * 128;\n    max_age: side + 1;\n}\n";
    let file = parse(src).unwrap();
    assert_eq!(emerge_lang::format(&file), src);
}

#[test]
fn a_variable_cannot_take_a_constants_name() {
    let dish = include_str!("../../../examples/dish.em");
    let with = |c: &str| dish.replacen("world Dish {\n", &format!("world Dish {{\n    const {c} = 1;\n"), 1);
    // `for cell in cells` in the world's genesis.
    assert_eq!(resolve(&parse(&with("cell")).unwrap()).unwrap_err().code, "E0111");
    assert!(resolve(&parse(&with("harmless")).unwrap()).is_ok());
}

#[test]
fn a_constant_that_waits_on_a_non_constant_is_reported_at_the_root() {
    assert_eq!(value("    const a = b;\n    const b = nothing;", "1").unwrap_err().0, "E0205");
}

#[test]
fn a_constant_cannot_take_a_reserved_or_top_level_name() {
    let dish = include_str!("../../../examples/dish.em");
    let with = |c: &str| dish.replacen("world Dish {\n", &format!("world Dish {{\n    const {c} = 1;\n"), 1);
    for (name, code) in [("tanh", "E0110"), ("tick", "E0110"), ("below", "E0110"), ("take", "E0110"), ("Tinker", "E0111"), ("Drifter", "E0111"), ("Dish", "E0111"), ("C", "E0111")] {
        assert_eq!(resolve(&parse(&with(name)).unwrap()).unwrap_err().code, code, "{name}");
    }
}

#[test]
fn the_expanded_text_is_plain_and_expands_to_itself() {
    let dish = include_str!("../../../examples/dish.em");
    let src = dish.replacen("world Dish {\n", "world Dish {\n    const side = 2 * 128;\n", 1).replacen("Grid(w: 256, h: 256) holds", "Grid(w: side, h: side + 0) holds", 1);
    let once = emerge_lang::format(&emerge_lang::prepare(&parse(&src).unwrap()).unwrap());
    assert!(!once.contains("const ") && !once.contains("extends"));
    assert!(once.contains("Grid(w: 256, h: 256) holds"));
    let again = emerge_lang::format(&emerge_lang::prepare(&parse(&once).unwrap()).unwrap());
    assert_eq!(once, again);
    assert_eq!(once, dish);
}
