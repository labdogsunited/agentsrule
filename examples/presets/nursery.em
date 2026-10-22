// Sunlit Nursery (proposals/world-presets.md §2): the reference dish with Builder only, whose seed
// genome already makes body and takes nitrate. Run with mutation off: the point is metabolism and
// resource limits, not evolution. Nothing here was discovered by the population; it was supplied.
// Controls: `amount` of the sunlight and weather transfers (light and mineral supply), `rate` of
// `spread` (diffusion). Initial population (`count`) and grid size need a restart.

world Dish {
    space: Continuous2D(w: 512.0, h: 512.0, wrap: true, max_interaction_radius: 4.0);

    element C;
    element N;
    element E;

    second_law { min_drop: 1; }

    substance light = E potential 10;
    substance energy = E potential 8;
    substance heat = E potential 0;
    substance sugar = C + E potential 9;
    substance carbon = C potential 0;
    substance nitrate = N potential 0;
    substance body = C + N potential 5;

    field cells : Grid(w: 256, h: 256) holds [
        light: 1u,
        sugar: 2_000u,
        carbon: 2_000u,
        nitrate: 2_000u,
        body: 2_000u
    ];

    source sun : [light];
    source spark : [energy];
    source rock : [carbon, nitrate, body];
    sink void : any;

    waste heat -> void;
    remains: void;

    reaction photosynthesis : light + carbon -> sugar;
    reaction respiration : sugar -> energy + carbon;
    reaction build : energy + carbon + nitrate -> heat + body;
    reaction rot : body -> carbon + nitrate;

    reaction_space {
        include: [photosynthesis, respiration, build, rot];
        generate: balanced(max_inputs: 3, max_outputs: 3, max_coeff: 2);
        max_rate: 1u;
        generated_max_rate: 250mu;
        exclude: [];
    }

    upkeep {
        pay: energy;
        becomes: heat;
        vent: void;
        base: 20mu;
        per_gene: 5mu;
        per_signal: 1mu;
        per_carried_u: 1mu;
    }

    limits {
        max_entities: 200_000;
        max_reaction_space: 65_536;
        inventory_cap: 100u;
    }

    max_age: 20_000;
    child_offset: 0.5;
    mind_steps: 1;

    senses {
        sense probe(q: any quantity, angle_deg: -180.0..180.0, dist: 0.0..8.0);
        sense gradient(q: any quantity);
        sense inventory(s: any substance);
        sense crowd(radius: 0.0..6.0);
        sense neighbour(s: any substance, rank: 0..3);
        sense clock(period: 1..1_000);
    }

    actions {
        action move(speed: 0.0..2.0) cost 20mu per distance;
        action turn(angle_deg: 0.0..30.0) cost 10mu;
        action take(s: any substance, max: 0u..10u) cost 5mu;
        action drop(s: any substance, max: 0u..10u) cost 0u;
        action give(s: any substance, max: 0u..10u, to: neighbour) cost 5mu;
        action steal(s: any substance, max: 0u..10u, from: neighbour) cost 30mu;
        action divide(split: 0.0..1.0) cost 0u requires body >= 10u;
    }

    regulation {
        gene weigh(
            bias: -16.0..16.0,
            curve: [linear, step, sigmoid, tanh]
        ) inputs 1..4 weight -16.0..16.0;
    }

    // World rules. Each holds one kind of effect, which sets its phase (spec §7.4).
    rule sunlight on cells {
        transfer(light, from: sun, to: self, amount: 40mu, overflow: void) @transfer_light;
    }

    rule weather on cells {
        transfer(carbon, from: rock, to: self, amount: 20mu, overflow: void) @transfer_carbon;
        transfer(nitrate, from: rock, to: self, amount: 10mu, overflow: void) @transfer_nitrate;
        transfer(body, from: rock, to: self, amount: 1mu, overflow: void) @transfer_body;
    }

    rule rotting on cells {
        react(rot, rate: 50bp) @react_rot;
    }

    rule spread on cells {
        diffuse_conserved([sugar, carbon, nitrate, body], rate: 300bp) @diffuse_conserved;
    }

    genesis {
        for cell in cells {
            transfer(carbon, from: rock, to: cell, amount: 50u) @transfer_carbon;
            transfer(nitrate, from: rock, to: cell, amount: 20u) @transfer_nitrate;
            transfer(body, from: rock, to: cell, amount: 1u) @transfer_body;
        }
    }
}

ruleset Tinker {
    rule sharpen weight 0.3 : regulate weigh(curve: sigmoid) -> regulate weigh(curve: step);
    rule nudge weight 0.3 : sense $kind(dist: $d) -> sense $kind(dist: jitter($d, sigma: 0.1));
}

// Seed genome with body synthesis and nitrate uptake already working. Run with mutation off.
lineage Builder in Dish {
    profile fast {
        genes: 32;
        signals: 16;
        modules: 8;
    }

    mutation {
        rate: 0.02;
        param_sigma: 0.1;
        expression_flip: 0.01;
        rewire: 0.01;
        diverge: 0.005;
        duplicate_gene: 0.005;
        insert_gene: 0.002;
        delete_gene: 0.005;
        duplicate_module: 0.002;
        delete_module: 0.001;
        split_module: 0.001;
        merge_modules: 0.001;
        rules: Tinker;
    }

    seed genome {
        module forage {
            sense gradient(q: cells.body) -> food_x, food_y @food_dir;
            regulate weigh(curve: tanh) <- [10.0 * food_y] -> steer @weigh_steer;
            action move(speed: 1.0) @go;
            action turn(angle_deg: 30.0) <- steer threshold -2.0 @turn_to_food;
        }

        module metabolism {
            reaction photosynthesis rate 1u @photosynthesis;
            reaction respiration rate 1u @respiration;
            reaction build rate 50mu @build;
            action take(s: light, max: 1u) @eat_light;
            action take(s: carbon, max: 1u) @eat_carbon;
            action take(s: nitrate, max: 250mu) @eat_nitrate;
        }

        module reproduce {
            sense inventory(s: body) -> fullness @inventory_fullness;
            regulate weigh(bias: -10.0, curve: step) <- [fullness] -> ready @weigh_ready;
            action take(s: body, max: 1u) @eat_body;
            action divide(split: 0.5) <- ready threshold 0.5 @split;
        }
    }

    genesis {
        count: 2_000;
        position = uniform(x: 0.0..512.0, y: 0.0..512.0);
        transfer(energy, from: spark, to: self, amount: 5u) @transfer_energy;
    }
}

view Main for Dish {
    layer ground = field(cells.sugar) {
        color: value |> scale(log, range: 1.0..5_000.0) |> palette(greens);
    }
    layer life = agents(where: true) {
        glyph: arrow(dir: heading);
        color: species_id |> palette(categorical);
        size: 1.0 + float(expressed_genes) / 4.0;
    }
}

chart Health for Dish {
    line(population, by: species);
    line(metrics.chain_depth);
    line(metrics.flux.body_synthesis);
    ledger(elements);
}
