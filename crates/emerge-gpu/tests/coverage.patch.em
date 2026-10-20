patch Dish {
    add after senses.inventory: sense delta(s: any substance);
    add after actions.steal: action guard(s: any substance) cost 20mu;
    add after regulation.weigh: gene product() inputs 2..2;
}

patch Drifter {
    set mutation.rate = 0.08;
    set mutation.param_sigma = 0.3;
    set mutation.rewire = 0.05;
    set mutation.duplicate_gene = 0.05;
    set mutation.insert_gene = 0.08;
    set mutation.delete_gene = 0.01;
}
