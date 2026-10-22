// Views for the dish-based presets (`examples/presets/`), in addition to `examples/views/dish.em`.
// Fixed scales, so two runs can be compared frame by frame.

// What limits growth, plainly: nitrate in the cells on a fixed log scale, creatures as dots.
// No glow: this is the resource, not a rendering effect.
view Nitrate for Dish {
    layer ground = field(cells.nitrate) {
        color: value |> scale(linear, range: 0.0..40.0) |> palette(blues);
    }
    layer life = agents(where: true) {
        glyph: dot();
        color: 0.9 |> palette(grays);
        size: 1.6;
    }
}

// Light in the cells (the Long Night's cause), creatures coloured by the energy they carry.
view Light for Dish {
    layer ground = field(cells.light) {
        color: value |> scale(log, range: 0.5..400.0) |> palette(cividis);
    }
    layer life = agents(where: true) {
        glyph: dot();
        color: units(energy) |> scale(linear, range: 0.0..20.0) |> palette(inferno);
        size: 1.8;
    }
}
