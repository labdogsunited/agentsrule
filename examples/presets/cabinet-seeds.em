// Automata Cabinet (proposals/world-presets.md §9): births on 2, nothing survives.
// Two-state `life_like` on a wrapping 64 x 64 grid, seeded random scatter. CPU only. Names for the
// patterns are not claimed; change one count and compare from the same seed.

world Life {
    space: Continuous2D(w: 64.0, h: 64.0, wrap: true, max_interaction_radius: 1.0);

    field life : Grid(w: 64, h: 64) states [dead, alive];

    rule conway on life {
        life_like(born: [2], survives: []) @life;
    }

    genesis {
        scatter(life, density: 0.1, to: alive) @soup;
    }
}

view Board for Life {
    layer board = field(life) {
        color: value |> scale(linear, range: 0.0..1.0) |> palette(magma);
    }
}
