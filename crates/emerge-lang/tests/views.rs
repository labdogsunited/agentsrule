//! View and chart checks (`proposals/visual-language.md` V1): names resolve, values have the kind
//! their place needs, and bindings not computed yet are warnings, so the reference dish stays valid.

use emerge_lang::{check, parse, view};

const DISH: &str = include_str!("../../../examples/dish.em");

/// The dish plus one view; the codes of its errors, and of its warnings.
fn codes(extra: &str) -> (Vec<&'static str>, Vec<&'static str>) {
    let file = parse(&format!("{DISH}\n{extra}\n")).unwrap();
    let errors = check::check(&file).into_iter().map(|p| p.code).collect();
    let warnings = view::check(&file).1.into_iter().map(|p| p.code).collect();
    (errors, warnings)
}

fn layer(source: &str, entries: &str) -> String {
    format!("view Probe for Dish {{\n    layer x = {source} {{\n        {entries}\n    }}\n}}")
}

#[test]
fn the_reference_dish_stays_valid_with_warnings_for_what_is_not_computed() {
    let (errors, warnings) = codes("");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(warnings, vec!["W1201"; 4], "species_id, by: species and two metrics");
}

#[test]
fn wrong_views_are_errors() {
    let cases = [
        ("unknown world", "view Probe for Absent {\n    layer x = field(cells.sugar) {\n        color: value |> palette(greens);\n    }\n}".to_string(), "E0114"),
        ("unknown field", layer("field(soil.sugar)", "color: value |> palette(greens);"), "E0114"),
        ("unknown substance", layer("field(cells.absent)", "color: value |> palette(greens);"), "E0114"),
        ("unknown palette", layer("field(cells.sugar)", "color: value |> palette(absent);"), "E0114"),
        ("a condition as colour", layer("field(cells.sugar)", "color: true;"), "E0205"),
        ("a log scale over zero", layer("field(cells.sugar)", "color: value |> scale(log, range: -5.0..0.0) |> palette(magma);"), "E0204"),
        ("an empty range", layer("field(cells.sugar)", "color: value |> scale(linear, range: 5.0..1.0) |> palette(magma);"), "E0204"),
        ("an identity on a sequential palette", layer("agents(where: true)", "color: lineage_id |> palette(viridis);"), "E0205"),
        ("an amount compared with a number", layer("agents(where: body > 5.0)", "glyph: dot();"), "E0205"),
        ("unknown agent property", layer("agents(where: true)", "size: wingspan;"), "E0114"),
        ("unknown entry", layer("field(cells.sugar)", "height: value;"), "E0115"),
        ("a signal field with a member", layer("field(scent.sugar)", "color: value |> palette(greens);"), "E0114"),
    ];
    for (what, view, code) in cases {
        let (errors, _) = codes(&view);
        assert_eq!(errors, vec![code], "{what}");
    }
}

#[test]
fn good_views_pass() {
    let cases = [
        layer("field(cells.carbon)", "color: value |> scale(linear, range: 0.0..100.0) |> palette(viridis);"),
        layer("field(scent)", "color: value |> scale(linear, range: 0.0..10.0) |> palette(magma);"),
        layer("agents(where: units(energy) > 2.0 and age < 500)", "glyph: arrow(dir: heading);\n        color: lineage_id |> palette(categorical);\n        size: 0.5 + float(expressed_genes) / 8.0;"),
        layer("agents(where: true)", "glyph: dot();\n        color: heading |> palette(phase);"),
    ];
    for v in cases {
        let (errors, warnings) = codes(&v);
        assert!(errors.is_empty(), "{v}: {errors:?}");
        assert_eq!(warnings.len(), 4, "only the dish's own warnings");
    }
}
