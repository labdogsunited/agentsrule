// Reference world for Phase 0 (spec v0.6 §15).
// Also the parser fixture: parse -> fmt -> parse must give the same AST,
// and `emerge fmt` must leave this file unchanged.
//
// Runs:
//   evolution:         emerge run dish.em --lineages Drifter
//   baseline:          emerge run dish.em --lineages Drifter --mutation off
//   positive control:  emerge run dish.em --lineages Builder --mutation off

world Dish {
    space: Continuous2D(w: 512.0, h: 512.0, wrap: true, max_interaction_radius: 4.0);

    element C;
    element N;
    element E;

    second_law { min_drop: 1; }

    substance light   = E      potential 10;
    substance energy  = E      potential 8;
    substance heat    = E      potential 0;
    substance sugar   = C + E  potential 9;
    substance carbon  = C      potential 0;
    substance nitrate = N      potential 0;
    substance body    = C + N  potential 5;

    field cells : Grid(w: 256, h: 256) holds [light: 2_000u, sugar: 2_000u, carbon: 2_000u,
                                              nitrate: 2_000u, body: 2_000u];
    field scent : Grid(w: 256, h: 256) signal;

    source sun   : [light];
    source spark : [energy];
    source rock  : [carbon, nitrate, body];
    sink   void  : any;

    waste heat -> void;
    remains: void;

    reaction photosynthesis : carbon + light            -> sugar;
    reaction respiration    : sugar                     -> carbon + energy;
    reaction build          : carbon + nitrate + energy -> body + heat;
    reaction rot            : body                      -> carbon + nitrate;

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
        per_brain_node: 1mu;
        per_carried_u: 1mu;
    }

    limits {
        max_entities: 200_000;
        max_reaction_space: 65_536;
        inventory_cap: 100u;
    }

    max_age: 20_000;

    senses {
        sense probe(q: any quantity, angle_deg: -180.0..180.0, dist: 0.0..8.0);
        sense gradient(q: any quantity);
        sense inventory(s: any substance);
        sense crowd(radius: 0.0..6.0);
        sense neighbour(s: any substance, rank: 0..3);
        sense clock(period: 1..1_000);
    }

    actions {
        action move(speed: 0.0..2.0)                    cost 20mu per distance;
        action turn(angle_deg: 0.0..30.0)               cost 10mu;
        action take(s: any substance, max: 0u..10u)     cost 5mu;
        action drop(s: any substance, max: 0u..10u)     cost 0mu;
        action give(s: any substance, max: 0u..10u, to: neighbour)    cost 5mu;
        action steal(s: any substance, max: 0u..10u, from: neighbour) cost 30mu;
        action emit(sig: scent, max: 0.0..10.0)         cost 2mu;
        action divide(split: 0.0..1.0)                  cost 0mu requires body >= 10u;
    }

    // World rules run in this order. Each rule holds one kind of effect.
    rule sunlight on cells {
        transfer(light, from: sun, to: self, amount: 2u, overflow: void);
    }

    rule weather on cells {
        transfer(carbon,  from: rock, to: self, amount: 20mu, overflow: void);
        transfer(nitrate, from: rock, to: self, amount: 10mu, overflow: void);
        transfer(body,    from: rock, to: self, amount: 1mu,  overflow: void);
    }

    rule rotting on cells {
        react(rot, rate: 50bp);
    }

    rule spread on cells {
        diffuse_conserved([sugar, carbon, nitrate, body], rate: 300bp);
    }

    rule smell on scent {
        self.value = fade(blur(scent, amount: 0.2), keep: 0.9);
    }

    genesis {
        for cell in cells {
            transfer(carbon,  from: rock, to: cell, amount: 50u);
            transfer(nitrate, from: rock, to: cell, amount: 20u);
            transfer(body,    from: rock, to: cell, amount: 1u);
        }
    }
}

ruleset Tinker {
    rule wrap  weight 0.5 : Add($a, $b) -> Mul(Add($a, $b), Sin($a));
    rule prune weight 0.2 : Mul($x, Const($c)) where abs($c) < 1e-3 -> 0.0;
    rule nudge weight 0.3 : sense $kind(dist: $d) -> sense $kind(dist: jitter($d, sigma: 0.1));
}

// Must discover body synthesis: a body-producing reaction plus take(nitrate).
lineage Drifter in Dish {
    profile fast {
        senses: 8;
        reactions: 4;
        actions: 8;
        brain: cgp(nodes: 32);
        registers: 4;
    }

    mutation {
        rate: 0.02;
        param_sigma: 0.1;
        expression_flip: 0.01;
        rules: Tinker;
    }

    seed genome {
        sense gradient(q: cells.light) @light_dir;
        sense inventory(s: energy) @energy;
        sense inventory(s: body) @body;

        reaction photosynthesis rate 1u;
        reaction respiration    rate 1u;

        action move(speed: 1.0) @go;
        action turn(angle_deg: 30.0) @steer;
        action take(s: light,  max: 1u) @eat_light;
        action take(s: carbon, max: 1u) @eat_carbon;
        action take(s: body,   max: 1u) @eat_body;
        action divide(split: 0.5) threshold 0.5 @split;

        brain {
            act[0] = 1.0;
            act[1] = Tanh(Scale(sense[0].y, 10.0));
            act[2] = 1.0;
            act[3] = 1.0;
            act[4] = 1.0;
            act[5] = Gt(sense[2], 10.0);
        }
    }

    genesis {
        count: 2_000;
        position = uniform(x: 0.0..512.0, y: 0.0..512.0);
        transfer(energy, from: spark, to: self, amount: 5u);
    }
}

// Positive control: body synthesis already works. Run with mutation off.
lineage Builder in Dish {
    profile fast {
        senses: 8;
        reactions: 4;
        actions: 8;
        brain: cgp(nodes: 32);
        registers: 4;
    }

    mutation {
        rate: 0.02;
        param_sigma: 0.1;
        expression_flip: 0.01;
        rules: Tinker;
    }

    seed genome {
        sense gradient(q: cells.light) @light_dir;
        sense inventory(s: energy) @energy;
        sense inventory(s: body) @body;

        reaction photosynthesis rate 1u;
        reaction respiration    rate 1u;
        reaction build          rate 250mu;

        action move(speed: 1.0) @go;
        action turn(angle_deg: 30.0) @steer;
        action take(s: light,   max: 1u) @eat_light;
        action take(s: carbon,  max: 1u) @eat_carbon;
        action take(s: nitrate, max: 250mu) @eat_nitrate;
        action divide(split: 0.5) threshold 0.5 @split;

        brain {
            act[0] = 1.0;
            act[1] = Tanh(Scale(sense[0].y, 10.0));
            act[2] = 1.0;
            act[3] = 1.0;
            act[4] = 1.0;
            act[5] = Gt(sense[2], 10.0);
        }
    }

    genesis {
        count: 2_000;
        position = uniform(x: 0.0..512.0, y: 0.0..512.0);
        transfer(energy, from: spark, to: self, amount: 5u);
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
