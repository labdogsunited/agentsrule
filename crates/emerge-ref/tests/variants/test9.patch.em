patch Dish {
    set space.max_interaction_radius = 1.0;
    add after remains: decompose: rot;
    set max_age = 1_000;
    set actions.steal.max = 0u..1u;
    set actions.steal.cost = 200mu;
    add after actions.steal: action guard(s: any substance) cost 20mu;
    set rules.sunlight@transfer_light.amount = 320mu;
}

patch Drifter {
    set mutation.duplicate_gene = 0.02;
}
