// Game of Life as a state field (proposals/field-rules.md, step 1): a Gosper glider gun on a
// bounded 64 × 64 grid. The world has no lineages; the field changes on its own.
// Draw it with: emerge-server render examples/life.em --lineages "" --tick 120 --view Board

world Life {
    space: Continuous2D(w: 64.0, h: 64.0, wrap: false, max_interaction_radius: 1.0);

    element E;

    second_law { min_drop: 1; }

    substance energy = E potential 1;
    substance heat = E potential 0;

    field cells : Grid(w: 64, h: 64) holds [energy: 1u, heat: 1u];
    field life : Grid(w: 64, h: 64) states [dead, alive];

    sink void : any;

    waste heat -> void;
    remains: void;

    upkeep {
        pay: energy;
        becomes: heat;
        vent: void;
        base: 1mu;
    }

    limits {
        max_entities: 10;
        inventory_cap: 1u;
    }

    rule conway on life {
        life_like(born: [3], survives: [2, 3]) @life;
    }

    genesis {
        pattern(
            life,
            x: 2,
            y: 2,
            rle: "24bo$22bobo$12b2o6b2o12b2o$11bo3bo4b2o12b2o$2o8bo5bo3b2o$2o8bo3bob2o4bobo$10bo5bo7bo$11bo3bo$12b2o!",
            to: alive
        ) @gun;
    }
}

view Board for Life {
    layer board = field(life) {
        color: value |> scale(linear, range: 0.0..1.0) |> palette(magma);
    }
}
