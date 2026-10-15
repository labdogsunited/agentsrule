# Runner findings for the next spec revision

What building and testing `emerge-ref` showed about spec v0.7.3 and `core.md`. Each item says what the spec says, what the runner does, and what the spec should say. When an item is settled in the spec, remove it here.

## Wrong claims

1. **Rot equilibrium (spec §15.1).** The spec says an ungrazed cell "cycles between `199mu` and `200mu`". In fact it ends **every tick at exactly `199mu`**: the rain brings it to `200mu` in Move, and rot removes `floor(200 × 50bp) = 1mu` in Transform. It holds `200mu` only between those phases. Test: `world_reactions_run_after_transfers`.
2. **Seed and diffusion (spec §15.1).** The spec says a seed takes "about 99%" of a `200mu` cell. Before rounding, its share is `1_000 / 1_004` (99.6%), and after rounding it gets **all `200mu`**: its fractional part (0.203) beats each neighbour's (0.199), so it gets the leftover µ. In general a seed loses at most about 0.4% of what it would take alone. Test: `a_seed_shares_a_cell_with_diffusion`.

## Gaps the runner had to fill

3. **Request order (spec §7.2, §10.2).** Remainders go to the largest fractional part, and ties are broken by Philox. But the spec does not define an order of requests, or which Philox draw each request gets, so ties are not yet determined. The runner breaks ties by request order: entity intents (entity order, then genome order), then transfer rules (declaration order, then cell order, then neighbour order). The spec should fix a request order and either use it for ties or say exactly how Philox draws map to requests.
4. **`max_age` boundary (spec §7.5).** "Has reached `max_age`" is ambiguous. The runner disposes of an entity in the tick in which the number of ticks it has lived, counting this one, reaches `max_age`, so `max_age: 1` means it dies in its first tick. Test: `disposal_comes_before_division_and_fills_the_cell_first`.
5. **Venting dying entities (spec §7.3).** "Every inventory" is vented. The runner vents dying entities too, before their disposal, so their waste goes to the waste sink, not to `remains`. Both conserve; the spec should say which.

## Not yet covered by the runner

Minds and signals (every gene fires every tick), senses, movement and positions, mutation, Philox, derived edges, `charge` laws in their general form (only upkeep), catalysts, the birth log and attribution counters.

## Editorial

- Spec status line: "widens the Phase 0 GPU spike" should be "widened".
