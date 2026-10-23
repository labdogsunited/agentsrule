// Reusable operators (language.md §6.9, proposals/reusable-operators.md): a definition with its parameters and the rule
// it writes, and two instances of it. `emerge expand` prints the rules they write out; `emerge fmt` keeps them as written.
// The world has no creatures: nutrients flow in from the rock, spread between cells and rot back.

world Soil {
    space: Continuous2D(w: 128.0, h: 128.0, wrap: true, max_interaction_radius: 1.0);

    element C;
    element N;

    second_law { min_drop: 1; }

    substance carbon = C potential 0;
    substance nitrate = N potential 0;
    substance humus = C + N potential 1;

    field cells : Grid(w: 64, h: 64) holds [carbon: 500u, nitrate: 500u, humus: 500u];

    source rock : [carbon, nitrate, humus];
    sink void : any;

    reaction rot : humus -> carbon + nitrate;

    // What one inflow is: a source, the field it fills, a substance, how much each cell gets each tick, and where the
    // excess goes.
    operator NutrientSupply version 1 {
        param from: source;
        param into: field;
        param what: substance;
        param ration: amount = 5mu;
        param spill: sink = void;

        rule supply on into {
            transfer(what, from: from, to: self, amount: ration, overflow: spill) @supply;
        }
    }

    rule spread on cells {
        diffuse_conserved([carbon, nitrate, humus], rate: 200bp) @spread;
    }

    rule rotting on cells {
        react(rot, rate: 50bp) @rot;
    }

    use NutrientSupply version 1 as carbon_in(from: rock, into: cells, what: carbon, ration: 20mu);
    use NutrientSupply version 1 as humus_in(from: rock, into: cells, what: humus);

    genesis {
        for cell in cells {
            transfer(humus, from: rock, to: cell, amount: 10u) @seed;
        }
    }
}

view Ground for Soil {
    layer ground = field(cells.humus) {
        color: value |> scale(log, range: 0.1..600.0) |> palette(greens);
    }
}
