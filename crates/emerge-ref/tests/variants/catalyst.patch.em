patch Dish {
    set space.max_interaction_radius = 1.0;
    add after remains: decompose: rot;
    set reactions.respiration.catalyze = nitrate(half: 1u, boost: 4.0);
    set max_age = 1_000;
    set actions.steal.max = 0u..1u;
    set actions.steal.cost = 200mu;
    add after actions.steal: action guard(s: any substance) cost 20mu;
    set rules.sunlight@transfer_light.amount = 320mu;
}
