# Runner findings for the next spec revision

What building and testing `emerge-ref` shows about the current spec and `core.md`. Each item says what the spec says, what the runner does, and what the spec should say. When an item is settled in the spec, remove it here.

## Open

1. **Reading units (spec §5.5, §10.1).** The spec says amounts are reported to senses "in units as `f32`", but not how they are converted. Division is not portable (WGSL division is accurate only to 2.5 ULP), so the runner computes `amount as f32 × 0.001`. `0.001` is not exact in `f32`, so `10.5u` reads as `10.500001`. The spec should define the conversion as `f32(amount) × 0.001f32`, and say that a gate at an exact amount (such as `bias: -10.0` on `inventory(body)`) may open one µ early or late. Test: `each_regulation_gene_adds_one_mind_step_of_delay`.
2. **Float products with amounts (spec §5.2, §5.5, §3.3).** Action sizes `floor(clamp(o, 0, 1) × max)`, reaction caps `floor(rate × clamp(s, 0, 1))` and the retention cost `ceil(per_signal_keep × keep)` mix a float and an amount. The runner computes them in `f32`, as a GPU would. The spec should say so, and bound the amounts involved by `2^24` µ (about `16_777u`) so the amount converts to `f32` exactly; menu ranges already stay far below that.

3. **Seeds never turn right (`examples/dish.em`, spec §15.1).** The seeds' `turn` gene reads `steer` (a `tanh`, in `−1..1`) with the default threshold `0.0`, and an action fires only when its signal is *above* its threshold (§5.2, §5.5). So a seed turns left toward food on its left, but food on its right gives a negative `steer` and no turn at all. §15.1's "`turn` steers up the gradient" holds only for half the directions, and its cost estimate assumes a turn fee every tick. Fix in `dish.em`: `action turn(angle_deg: 30.0) <- steer threshold -1.0`, which fires every tick with a signed angle. Test: `a_negative_signal_does_not_fire_a_turn_with_the_default_threshold`.
4. **Undefined geometry (spec §5.5, §6.1, §7.2 step 4).** The runner had to choose, and the spec should state:
   - `cell_at(pos)`: `floor(pos × (grid size / space size))` per axis, in `f32`;
   - `gradient`: central differences over the four neighbouring cells, `(east − west) × grid_w / (2 × w)` and the same for `y`, then projected onto the heading and its left `(−hy, hx)`; cell `(x, y − 1)` is north;
   - turning: rotate the heading by the angle, then renormalize it to unit length (rotation alone drifts);
   - wrapping: `rem_euclid`, with a result equal to the size mapped to `0`.
5. **Repulsion is unspecified (spec §7.2 step 4).** Movement includes "a soft repulsion force", with no formula, range or strength. The runner has none.
6. **Genesis randomness (spec §10.2).** Lineage genesis draws positions and headings, but no pass is defined for it. The runner uses pass `genesis = 5`, stream = the new entity's ID, draws 0 and 1 for `x` and `y` and draw 2 for the heading angle, each as `(draw >> 8) × 2^−24`.

## Not yet covered by the runner

Senses other than `inventory`, the entity's own cell and `gradient`; `delay` and arithmetic regulation kinds; repulsion; `give`, `steal`, `emit` and field signals; mutation; derived edges; `charge` laws in their general form (only upkeep); catalysts; the birth log and attribution counters.
