# Runner findings for the next spec revision

What building and testing `emerge-ref` shows about the current spec and `core.md`. Each item says what the spec says, what the runner does, and what the spec should say. When an item is settled in the spec, remove it here.

## Open

Spec v0.7.5 and v0.7.6 settled everything found before mutation (Appendices F and G). Found while implementing mutation (`src/mutate.rs`), where the spec leaves choices open:

1. **Draws (spec §8.1, §10.2).** "Fixed draw indices per pass as in v0.3" is not defined in this spec. The runner uses pass `mutate = 3`, stream = the child's ID, and numbers draws sequentially as the operators consume them, in the order of §8.1. Every operator decision is one draw (`chance`), so the indices depend on earlier outcomes; that is deterministic on CPU and GPU alike, but a GPU kernel must run the operators in the same order.
2. **Point mutations (§8.1 step 1).** The rate is per gene: a gene that mutates changes **one** of its fields, chosen uniformly (`bias`, `curve`, one weight; a parameter or the threshold; the substance, `max` or threshold of `take`/`drop`; for reaction genes, the reaction or the rate). A float moves by `N(0, param_sigma × (hi − lo))` and is clamped to the menu's range; amounts are rounded to whole µ. Thresholds have no range: they move by `N(0, param_sigma)` and are clamped to `±16`. `keep` factors mutate per signal at `rate`, by `N(0, param_sigma)`, within `0..1`. Expression bits flip at `expression_flip` per gene and per module. Normal draws use Box–Muller from two uniform draws.
3. **Reaction IDs (§4.3).** "Neighbouring indices are similar reactions" is defined as lexicographic order of canonical `(inputs, outputs)` term lists (substance order, then coefficient). A reaction mutation moves half the time by `1..8` entries up or down, otherwise to a uniformly random entry, and lowers the gene's rate to the new reaction's maximum if needed. A generated reaction equal to a declared but not included one uses the declared reaction's ID.
4. **Wiring (§8.1 step 2).** `rewire` moves one input to a uniformly chosen signal; for an action or reaction gene's source the constant `1.0` is one more choice. `diverge` points one output at a fresh signal (`keep 0.0`), if the profile has room.
5. **Growth (§8.1 step 3).** Gene operators run per gene in genome order (duplicate, insert, delete), then module operators per module (duplicate, delete, split, merge). An inserted gene is drawn uniformly over the gene kinds the menus offer and the runner supports, with parameters uniform in their ranges; regulation genes read uniformly chosen existing signals. A duplicated module's **private** signals are those no gene outside the module uses. Afterwards, signals no gene uses are removed and the rest renumbered in order. `delete_module` never removes the last module; a genome may lose all its genes and then dies.
6. **Profiles (§5.7).** A lineage with `mutation` needs a `profile`; with several, the runner uses the first.
7. **Carriers (§16, exit test 9).** "Carried by ≥ 10% of the population" is measured as the share of entities with an active reaction gene that produces `body`.

Found while implementing contact (`src/near.rs`) and the remaining senses:

8. **`neighbour(s, rank)` (§5.5).** The spec lists it but never defines it. The runner reports the amount of `s`, in units, carried by the `rank`-th nearest other entity within `max_interaction_radius` (rank `0` is the nearest), or `0.0` if there are fewer. Nearness is the same as for `give` and `steal`: squared distance in `f32`, wrapped, ties to the lower ID.
9. **`crowd(radius)` (§5.5).** Defined by the runner as the number of other entities within `radius`, counting only as far as `max_interaction_radius`. Dish's menu offers `radius: 0.0..6.0` with a radius of `4.0`, and so does the spec's example: either the compiler should reject a range beyond the radius, or the spec should say it is clamped, as the runner does.
10. **`probe(q, angle_deg, dist)` (§5.5).** Defined by the runner as the amount in the cell under the point `dist` from the entity along its heading turned `angle_deg` to the left, wrapped like movement. Only quantities of the conserved field; signal fields are not run.
11. **`clock(period)` (§5.5).** Undefined in the spec. The runner reports `sin(2π × (tick mod period) / period)`.
12. **Contact targets (§5.5, §7.2).** Each `give` or `steal` gene targets the nearest other entity, so several contact genes all act on the same one. If the target is dying this tick, the request is dropped in Move but the attempt fee is kept, as for any intent whose transfer comes to nothing.
13. **Menu items the runner does not implement (§5.5).** The spec says a world must not list them. The runner instead reports each as a `W1001` warning of a partial run, since a gene could never use it and mutation never produces it. Dish lists `emit`, which needs signal fields.

## Not yet covered by the runner

The `channel` sense; `delay` and arithmetic regulation kinds; `emit` and field signals (so mutation cannot insert `emit` genes yet); rewrite rulesets; `rate: evolvable`; recombination; derived edges; `charge` laws in their general form (only upkeep); catalysts; the birth log and attribution counters.
