# Briefing: producers lose to thieves in an evolving world

Written 2026-10-17 to ask for outside ideas. It stands alone: no knowledge of the project is needed.

## The setting

`.em` is a small language for describing artificial worlds in which things evolve. We run it with a reference simulator (Rust, deterministic: same seed gives the same bytes). The world is a grid of cells holding substances, with entities moving on it.

- **Matter is conserved.** Substances only move or react; nothing appears from nothing. Energy comes from "light" arriving in the cells.
- **Entities have genomes.** A genome is a list of genes: senses (read amounts), small signal "minds", actions (`move`, `take`, `drop`, `give`, `steal`, `divide`) and chemical reactions. Genomes mutate when entities divide.
- **Every action pays a fee** in energy (an "attempt fee"), and entities pay upkeep every tick. Entities die of old age (`max_age` 1_000 ticks) or when energy runs out.
- **Dead bodies rot** back into raw materials (`carbon`, `nitrate`).
- **Contact actions** (`give`, `steal`) work on the nearest other entity within an interaction radius. Entities may overlap: there is no repulsion.
- **Senses report amounts, not changes.**

Substances that matter here: `light` (energy), `sugar`, `body` (what an entity needs to divide), `carbon`, `nitrate`. Making `body` from `carbon` and `nitrate` costs about 1_000mu of energy per unit. Taking `body` that lies in a cell costs 5mu. (1u = 1000mu.)

## The test we want to pass ("exit test 9")

Start from a population (`Drifter`) that only scavenges body from the environment. Evolution, with mutation on, must discover **body synthesis** and keep it. Pass means: **at least 2% of entities are producers** (they made body in the last 1_000-tick window) **in every window for 10_000 ticks in a row**. Production must also show a positive flow.

Coexistence is enough; producers do not have to win.

## The problem

When `steal` is on the menu (take a substance straight from a neighbouring entity), almost every entity learns it (98–100%). Producers pay the full cost of making body, and thieves take it from them. Producers rise in bursts (up to about 11%) and then fade to near 0.

## What we have found

1. **Selection itself works.** Hand-written producers (`Builder`) take over from scavengers (99% in 3 of 3 seeds) once corpses rot into raw materials instead of leaving usable body. So the economics can favour making body.
2. **Without `steal`, producers take over in 7 of 10 seeds** (74–90%). So `steal` is the one blocker. (In those runs most seeds find synthesis too late for a 10_000-tick streak within 30_000 ticks; only 1 of 10 passes. Time matters too.)
3. **Longer runs do not fix it.** 60_000 ticks, 6 seeds, `steal` unchanged: no seed passes. Bursts reach 11% and the longest stretch at or above 2% is 7_000 ticks.
4. **Burst producers are also thieves.** They make body with generated multi-input reactions (not a hand-written `build`) and also steal. They do not use any change-detecting circuit.
5. **Pricing `steal` (max 1u per action, fee 200mu) gets 1 of 10 seeds through.** In that seed, producers reached 60–78% and kept it while steal spread to 98%: most entities both make body and steal. Another 7 seeds ended with producers near 0, and 2 went extinct. Same price, 10 seeds, 40_000 ticks.
6. **Limiting `steal` to light and sugar** (so body cannot be stolen) did not help: thieves live on stolen light and sugar and producers stay at 0–0.8%.
7. **Adding a `delta` sense** (own amount now minus at the previous reading) with the same price: 6 of 10 seeds are already stopped as unable to pass; the other 4 were still running when this was written (check `proposals/plan-test9.md` for the final count). Producers sit at 0–2%. It does not look like a fix.
8. **Things ruled out earlier:** making `steal` pay less; stopping newborns from dividing early; removing free body from weather; the rot rate; capping body per cell.
9. **A fee asymmetry we noticed:** `take` pays its fee whenever the cell can hold the substance, but `steal` pays only when a neighbour is actually in range. So a failed search for a victim is free.

## What we think we can still do

Principle: costs should come from general physics, not from rules written for theft. Rules aimed at the outcome are a last resort.

1. **Raise the price of `steal`** (for example 500mu or max 0.5u). A number change. Most likely to move the 1-in-10 result, but the dish has already had many rounds of tuning.
2. **Make `steal` pay its fee on every attempt**, like `take`. Small and general.
3. **Bound substances.** `body` could not be pulled out of an entity by `take`, `steal` or `give`; only reactions or division move it. We expect little alone (see finding 6).
4. **Space.** Soft repulsion, or a grid with exclusive occupancy, so a thief must stay next to its victim and the victim can leave. Kin groups, territories and fleeing could then evolve. It is the most promising principled option and the most work (a new force in the movement phase).
5. **Rules aimed at theft** (guards, a contest formula for who wins, a cost for chasing). Last resort.
6. **Change the test** to accept benefit at the group level (kin or neighbourhood). A design decision; the spec asks for individual benefit.

## Questions for you

- Which mechanisms in real or artificial-life systems keep a costly producer alive next to free-riders when contact is unrestricted? We know of kin selection, spatial structure, policing and reciprocity, and we are looking for others that need no rule written for the occasion.
- Is the single passing seed (finding 5) a sign that a different price would pass often, or a lucky early takeover that happens before theft spreads?
- Is there a cheap change to the dish, not the language, that we have missed?
- Could the test be restated so it measures what we care about (evolution finds and keeps a costly skill) without being defeated by free-riding?

## Where the evidence lives

Details, run folders and numbers: `proposals/plan-test9.md`, `proposals/theft-consequences.md`, `proposals/problem-body-synthesis.md`.
