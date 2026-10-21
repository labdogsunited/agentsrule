// Views for the reference world (`examples/dish.em`) and every dish built from it.
// Draw one with: emerge-server render examples/dish.em --lineages Drifter,Builder --tick 300 --views examples/views/dish.em --view Lineages

// Where the food is, and who is where: body in the cells on a log scale, agents by founding lineage.
view Lineages for Dish {
    layer ground = field(cells.body) {
        color: value |> scale(log, range: 0.05..20.0) |> palette(magma);
    }
    layer life = agents(where: true) {
        glyph: arrow(dir: heading);
        color: lineage_id |> palette(categorical);
        size: 2.6;
    }
}

// Who is fed: carbon in the cells, agents coloured by the energy they carry.
view Energy for Dish {
    layer ground = field(cells.carbon) {
        color: value |> scale(linear, range: 0.0..150.0) |> palette(cividis);
    }
    layer life = agents(where: true) {
        glyph: dot();
        color: units(energy) |> scale(linear, range: 0.0..20.0) |> palette(inferno);
        size: 1.6 + float(expressed_genes) / 12.0;
    }
}

// Which way they go: headings on a cyclic palette over the sugar left by the dead.
view Headings for Dish {
    layer ground = field(cells.sugar) {
        color: value |> scale(log, range: 0.5..2_000.0) |> palette(grays);
    }
    layer life = agents(where: true) {
        glyph: arrow(dir: heading);
        color: heading |> palette(phase);
        size: 2.8;
    }
}

// Exit test 9's subject: who makes body. Nitrate in the cells (what a producer must take), every
// agent as a faint dot, and producers (agents whose own reactions made body in the window) bright,
// by how much they made. `made` needs a window: render with --made body --window 1000.
view Producers for Dish {
    layer ground = field(cells.nitrate) {
        color: value |> scale(log, range: 0.5..60.0) |> palette(blues);
    }
    layer everyone = agents(where: true) {
        glyph: dot();
        color: 0.35 |> palette(grays);
        size: 1.2;
    }
    layer producers = agents(where: made > 0mu) {
        glyph: dot();
        color: units(made) |> scale(log, range: 0.05..20.0) |> palette(inferno);
        size: 2.4;
    }
}
