//! Literals, line breaking, trivia, diagnostics, and the examples in `language.md`.

use emerge_lang::{format, parse};

/// Formats `src` and checks that formatting again changes nothing.
fn fmt(src: &str) -> String {
    let once = format(&parse(src).unwrap_or_else(|d| panic!("{d}")));
    let twice = format(&parse(&once).unwrap_or_else(|d| panic!("reformatting: {d}\n{once}")));
    assert_eq!(once, twice, "formatting is not idempotent");
    once
}

#[test]
fn amounts_print_in_units_when_whole() {
    let out = fmt("world W {\n    upkeep {\n        a: 0.5u;\n        b: 2000mu;\n        c: 0mu;\n        d: 1500mu;\n    }\n}\n");
    assert!(out.contains("a: 500mu;"), "{out}");
    assert!(out.contains("b: 2u;"), "{out}");
    assert!(out.contains("c: 0u;"), "{out}");
    assert!(out.contains("d: 1_500mu;"), "{out}");
}

#[test]
fn amounts_must_be_whole_micro_units() {
    let err = parse("world W { max_age: 0.0005u; }").unwrap_err();
    assert_eq!(err.code, "E0101");
    assert_eq!((err.line, err.col), (1, 20));
}

#[test]
fn numbers_of_four_digits_or_more_are_grouped() {
    let out = fmt("world W {\n    limits {\n        a: 999;\n        b: 1000;\n        c: 65536;\n        d: 5000.0;\n    }\n}\n");
    assert!(out.contains("a: 999;") && out.contains("b: 1_000;"), "{out}");
    assert!(out.contains("c: 65_536;") && out.contains("d: 5_000.0;"), "{out}");
}

#[test]
fn negative_range_bounds_and_formulas_in_lists() {
    // A config block with one entry is written on one line (language.md §2.2).
    let src = "world W {\n    reaction_space { exclude: [light -> energy, 2 a -> b + c]; }\n\n    regulation {\n        gene weigh(bias: -16.0..16.0) inputs 1..4 weight -16.0..16.0;\n    }\n}\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn a_long_line_breaks_its_widest_outermost_group() {
    // The call's argument list is the widest group, so it breaks one argument per line.
    let src = "world W {\n    rule r on cells {\n        transfer(carbon, from: rock, to: self, amount: 20mu, overflow: void, extra_argument_name: 1u) @t;\n    }\n}\n";
    let out = fmt(src);
    let expected = "world W {\n    rule r on cells {\n        transfer(\n            carbon,\n            from: rock,\n            to: self,\n            amount: 20mu,\n            overflow: void,\n            extra_argument_name: 1u\n        ) @t;\n    }\n}\n";
    assert_eq!(out, expected);
}

#[test]
fn comments_and_blank_lines_survive() {
    let src = "// header\n\n// about W\nworld W {\n    element C; // carbon\n\n    // energy next\n    element E;\n}\n\n// trailing note\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn blank_lines_collapse_to_one_and_none_open_a_block() {
    let src = "world W {\n\n    element C;\n\n\n    element E;\n}\n";
    assert_eq!(fmt(src), "world W {\n    element C;\n\n    element E;\n}\n");
}

#[test]
fn a_missing_semicolon_is_reported_where_it_is_missing() {
    let err = parse("world W {\n    element C\n    element E;\n}\n").unwrap_err();
    assert_eq!(err.code, "E0001");
    assert_eq!((err.line, err.col), (3, 5));
    assert!(err.message.contains("`;`"), "{}", err.message);
}

#[test]
fn comparisons_do_not_chain() {
    assert_eq!(parse("world W { rule r on c { when 1 < 2 < 3 { } } }").unwrap_err().code, "E0004");
}

#[test]
fn language_md_examples_parse_and_are_canonical() {
    // From language.md §9.2, §9.3 and §10.3.
    let src = r#"view Main for Dish {
    layer ground = field(cells.sugar) {
        color: value |> scale(log, range: 1.0..5_000.0) |> palette(greens);
    }
    layer life = agents(where: units(body) > 5.0) {
        glyph: arrow(dir: heading);
        color: species_id |> palette(categorical);
        size: 1.0 + float(expressed_genes) / 4.0;
    }
}

explore ChemistryAtlas {
    base: Dish;
    vary Dish.reaction_space.generate.max_coeff in [1, 2, 3];
    vary Dish.rules.rotting@react_rot.rate in logspace(from: 10bp, to: 500bp, steps: 8);
    vary seed in 1..4;
    ticks: 50_000;
    batch: 32;
    measure: [activity, species, chain_depth, novelty];
    keep: top(12, by: novelty);
    atlas: atlas(view: Main, every: 5_000);
}

patch Dish base 41 {
    set upkeep.base = 25mu;
    set rules.sunlight@transfer_light.amount = 3u;
    replace rules.smell {
        self.value = fade(scent.here, keep: 0.8);
    }
    set reaction_space.exclude = [];
    rename rules.rotting to decompose;
    remove substances.ozone move_to: void;
}

lineage L in Dish {
    seed genome {
        signal hunger keep 0.9;

        module metabolism {
            reaction photosynthesis rate 1u <- hunger @photosynthesis;
            dormant reaction build rate 250mu @build;
            action turn(angle_deg: 30.0) <- steer threshold -2.0 @turn;
            regulate weigh(curve: tanh) <- [-2.5 * hunger, steer] -> drive @weigh_drive;
        }
    }
}
"#;
    assert_eq!(fmt(src), src);
}

#[test]
fn a_signal_field_may_declare_how_writes_combine() {
    let src = "world W {\n    field scent : Grid(w: 8, h: 8) signal combine max;\n}\n";
    assert_eq!(fmt(src), src);
}
