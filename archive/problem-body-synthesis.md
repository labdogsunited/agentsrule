# The problem: body synthesis does not pay

Status: summary, 2026-10-16. Evidence and run directories are in `plan-test9.md`.

**Update (2026-10-16): selection now works.** With `decompose: rot;` (option 1 below, committed), sunlight 320mu, weather body 1mu and `max_age` 1_000 or 2_000, a 1% `Builder` takes over (99%) in 3 of 3 seeds, and a 1% `Drifter` does not invade. Discovery (test 9 with mutation on) is not yet tested.

## What we want

Exit test 9: starting from `Drifter`, which only scavenges, evolution finds body synthesis and keeps it. At least 2% of entities must make body, in every window, for 10_000 ticks (spec §15.1, §16).

## What happens

In 10 seeds of 30_000 ticks, producers stay at 0–0.16%. Almost every entity learns to `steal` (99.5–100%). Body synthesis appears in short bursts (up to 15.8% in one seed) and then disappears.

## What we ruled out

| Idea | Test | Result |
| :-- | :-- | :-- |
| `steal` pays too much | steal limited to light and sugar; steal removed from the menu | producers still 0 |
| Newborn builders divide too early | `divide` needs energy >= 8u as well as body >= 10u | `Builder` still dies out (43% share at tick 50, 0 at 450) |
| Free body from weather and genesis | weather and genesis body set to 0 | `Builder` still dies out (tick ~750) |
| Slow rot lets corpses pile up | rot from 0.5% to 100% per tick | no value works: slow rot, `Builder` is parasitised; fast rot, `Drifter` stops dividing (confounded, see below) |
| Corpses stay in cells | body cap 200mu, corpses vent to `remains: void` | `Builder` lasts longer (gone at tick 1_500) but still dies out |

## Why (our best reading)

1. **Making body costs about 1_000mu of energy per unit. Taking it from a cell costs 5mu.** This favours scavengers when body is easy to find, but it does not decide the contest alone: scavenging also depends on how much body is in reach, competition for it and time spent searching. Synthesis can pay when an entity has spare energy but cannot collect enough body. The current dish does not have that regime: populations are energy-limited, and builders spend extra energy on synthesis.
2. **A builder's body leaks to others.** When a builder dies, its body goes into its cell. Scavengers take it for 5mu. Even with no body anywhere else, `Drifter` grows on `Builder`'s corpses until `Builder` is gone.
3. **The rot sweep does not isolate corpse leakage.** Disposal runs in Restructure, after the `rotting` rule in Transform (spec §7). A fresh corpse can therefore be taken in the next Move, before any rot, even at 100%. And `rotting` acts on all cell body, so fast rot also destroys the weather body before `Drifter` can take it (weather arrives in Move, rot runs in Transform). That explains why `Drifter` stopped dividing: at 100% rot, corpses were its only body. Rot mixes two functions: removing corpse body and removing external body.

So a builder pays the full price, and a scavenger can get the product. Selection removes the builders. This is a property of the dish's incentives, not a bug in the runner.

## Where the dish cannot help

`dish.em` can change numbers, but not these:

- A flat fee on `take` charges every substance the same, so it cannot make body alone expensive.
- On death an entity's substances go to the cell as the same substance. There is no way to turn body into something that scavengers cannot use.

## Options

1. **Death recycling into raw materials (first choice):** during disposal, a dying entity's body goes through the existing `rot` reaction (`body -> carbon + nitrate`) before it reaches the cell. Products follow the normal cap and overflow rules. Elements are conserved and the potential drops. Nobody can take the corpse as body, and external body rain stays untouched. Small runtime change: disposal (`crates/emerge-ref/src/world.rs`, step 4.1) now moves substances unchanged.
2. **Per-substance action fee (second, as a diagnostic):** `take(body)` costs more than `take(light)`. The current fee is an attempt fee, not a price per unit: a high `take` fee also charges empty searches (`take` pays whenever the cell can hold the substance, `world.rs` line 808), and it penalises `Builder`, which still tries `take(body)`. `give` and `steal` pay only when a neighbour is in range (line 821), so their fee bites only in dense populations. A fee per unit moved is easier to read but needs payment and arbitration rules.
3. **Death conversion to a new substance:** body becomes `corpse`, which rots into body slowly. This only delays the same subsidy and adds a rate to tune. Not first.
4. **Let producers keep what they make:** ownership tags. Not needed yet: synthesised body already stays in its maker and goes to its children through division.
5. **Change the test:** accept group-level benefit (kin or neighbourhood) as the mechanism. This is a design decision, not a fix, and the spec (§15.1) asks for an individual benefit. Keep the criterion unchanged for now.

## Next experiment

Option 1, with unchanged uptake fees and the weather body kept on.

1. **Find a regime where body limits reproduction.** We want a scavenger that survives and reproduces slowly, and a similar producer that uses spare energy to reproduce faster. Sweep the weather body supply and the sunlight together. Record energy income and spending, body taken, body made and surviving descendants per lineage, not only population and producer share.
2. **Test selection before discovery.** Mutation off. Genomes that differ only in the synthesis machinery, with several build rates, including rates below `Builder`'s 50mu.

| Run | Shows |
| :-- | :-- |
| `Drifter` alone | the ancestor still survives and reproduces |
| Producer alone | synthesis is viable |
| 1% producers in an established scavenger population | synthesis can spread when rare |
| 1% scavengers in an established producer population | whether losing synthesis can spread |
| Longer mixed runs | production persists, not only a transient |

Let the resident population settle before adding the rare variant; a 50:50 start on fresh stocks answers a different question. Producers need not remove scavengers: stable coexistence above 2% is enough.

3. **Then discovery.** Evolution must also put together a body reaction and `take(nitrate)`; the census already shows body reactions without nitrate uptake. If introduced producers spread but evolved ones never establish, study mutation paths and module expression. If introduced producers die out, changing mutation rates will not help.

## What we do not know

- Whether `Builder` survives long-term at very high rot (only 1_000 ticks were run).
- Whether the 1_000mu vs 5mu gap, not corpse leakage, is the larger cause.
- Whether the dish has any light and body-supply setting where body, not energy, limits reproduction.
- Whether evolved producers, unlike hand-written `Builder`s, would find a niche before they are outcompeted.
