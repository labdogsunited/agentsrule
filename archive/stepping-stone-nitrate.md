# Proposal: a world where `take(nitrate)` pays on its own

Status: archived 2026-10-21; phase 1 ran and the stepping stone does not work, so phase 2 was not run (result at the end). Written before any run (question 19 in `archive/questions-2026-10-21.md`). A new experiment, not a retune of test 9: the test-9 result (13 of 20 seeds) stands as recorded in spec §15.1.

## Problem

Producers need two gene pieces at once: a reaction that makes `body` (`build`: `energy + carbon + nitrate -> heat + body`) and `take(nitrate)` to feed it. Each piece alone costs upkeep and fees and gains nothing, so a lineage must switch both on before either pays. Evidence: with `build` already in every genome, 20 of 20 seeds sustain producers (`Halfway`, spec §15.1); `share_gene` failed at step 0 (carriers of the two pieces are rarely in contact); cheaper idle genes made things worse (`plan-next.md` item 1).

**Only one of the two pieces can pay alone, by the chemistry.** Nitrogen enters an entity only through `take`, `steal` or `give`: `nitrate` and `body` are the only substances with `N`, and inside one entity `rot` followed by `build` turns energy into heat and leaves body unchanged. So `build` without nitrate intake can never pay, in any variant that keeps this chemistry. `take(nitrate)` can pay alone if nitrate does something useful in the holder. The existing mechanism for that is a catalyst (spec §4.5): `respiration catalyze nitrate(...)`. That was tried once (`archive/catalyst-dish.em`) and did not help, because the dish is not short of energy: in failed test-9 seeds mean energy was 45–88u and entities were short of body (`archive/plan-test9.md`). A faster respiration only pays where energy is scarce.

**Hypothesis.** In a dish where energy limits growth and nitrate catalyses respiration, `take(nitrate)` pays on its own. A population that keeps `take(nitrate)` for that reason then needs only one more piece (`build`) to make producers, as `Halfway` needed only `take(nitrate)`, so producers arise and hold in more seeds.

## Phase 1: does each piece pay alone? (no evolution)

Head-to-head runs with mutation off, from the same start: 2_000 of each lineage (each lineage's `genesis`), `--partial`, 5_000 ticks, a row every 500. Lineages (`archive/stepping-dish.em`, the test-9 dish plus two lineages):

- `Drifter`: neither piece.
- `NitrateOnly`: `Drifter` plus `take(nitrate, max: 250mu)`, as `Builder` takes it.
- `BuildOnly`: `Drifter` plus `reaction build rate 50mu`, as `Builder` runs it (the same genome as `Halfway`).
- `Builder`: both.

Each runs against `Drifter`: three pairs. Worlds, fixed now (`--set` on the dish, nothing else changed):

| World | `respiration` | Sunlight per cell per tick |
| :-- | :-- | :-- |
| W0 | uncatalysed (the test-9 dish) | 320mu |
| W1 | `catalyze nitrate(half: 1u, boost: 4.0)` (as the catalyst dish) | 320mu |
| W2 | the same catalyst | 80mu |
| W3 | the same catalyst | 40mu (the reference dish's level before test 9) |

Seeds 1–3: 4 worlds × 3 pairs × 3 seeds = 36 runs, CPU, default mode, at most 12 at once.

**Measure:** the challenger's share of the population at tick 5_000 (column `share_X` of `emerge run`), and the population, so a world where everything dies is visible.

**A piece pays alone in a world** if its lineage's share at tick 5_000 is at least 0.6 in all three seeds, with a living population. **It costs** if the share is at most 0.4 in all three. Anything else is unclear.

**Expectations, written before the runs:** `BuildOnly` costs in every world (it cannot pay, above). `NitrateOnly` costs in W0 (nitrate does nothing there). In W1 it may cost or be unclear (the catalyst dish's result). Whether it pays in W2 or W3 is the question.

**Stop rule for phase 1.** If `NitrateOnly` does not pay in any of W1–W3, record it and stop: the catalyst stepping stone does not work in this chemistry at these light levels, and phase 2 is not run. If `BuildOnly` pays anywhere, stop as well and look for the mistake before going on, because the chemistry says it cannot.

## Phase 2: evolution (only if phase 1 finds a world)

The world: the W1–W3 world in which `NitrateOnly` pays, choosing the one with the highest sunlight if several do (the least change from the test-9 dish).

- **Treatment:** unseeded `Drifter`, seeds 1–20, 40_000 ticks, test 9's flags (`--partial --flux body --producers 0.02 --census 1 --stop-when-sustained --stop-when-hopeless`), CPU, default mode.
- **Control:** if the world is W1, the control is the existing test-9 batch (`~/test9dup/`, the same dish without the catalyst: 13 of 20). If it is W2 or W3, the same 20 seeds in the same world without the catalyst, run alongside: a lower light level changes much else, and only the catalyst is the treatment.
- **Also measured:** the share of entities carrying an expressed `take(nitrate)` every 1_000 ticks before the first producers (from `--census`), in treatment and control. The hypothesis needs it to be higher in the treatment; if it is not, any change in the count has another cause.

**Rule, fixed now:** the stepping stone works if the treatment sustains producers in at least 5 more seeds than its control **and** the `take(nitrate)` share before producers is higher in the treatment in at least two thirds of the seeds. It does not work if the treatment is not above the control. Anything else is unclear: record it, with no second batch. No retuning between phases or after phase 2.

## What this does not cover

- Other routes for `build` to pay alone need a new mechanism (substance effects, `proposals/substance-effects.md`) or a different chemistry. They are left for a later proposal if phase 1 shows the catalyst stone cannot work.
- Phase 1 says whether a piece pays when every entity of a lineage has it from the start. Under evolution a piece appears in a few entities at a time, so a piece that pays in phase 1 may still be lost by drift. Phase 2 measures that.
- The GPU could run phase 1 faster, but `emerge-gpu run` reports no share per lineage yet, and the baseline batch holds the GPU. Phase 1 runs on the CPU.

## Result of phase 1 (2026-10-21): the stepping stone does not work; phase 2 not run

36 runs, `~/stepping/phase1/` (commit `b46d497` for the dish and rules). Challenger's share at tick 5_000, seeds 1–3:

| | W0 (320mu) | W1 (320mu, catalyst) | W2 (80mu, catalyst) | W3 (40mu, catalyst) |
| :-- | :-- | :-- | :-- | :-- |
| `NitrateOnly` | 0.500, 0.505, 0.497 | 0.500, 0.505, 0.497 | 0.011, 0.012, 0.011 | 0.000, 0.000, 0.000 |
| `BuildOnly` | 0.498, 0.498, 0.496 | 0.498, 0.498, 0.496 | 0.500, 0.498, 0.497 | 0.332, 0.333, 0.362 |
| `Builder` | 1.000 (all three) | 1.000 (all three) | 0.066, 0.078, 0.068 | 0.000 (all three) |

By the rules fixed before the runs: `NitrateOnly` pays alone in none of W1–W3 (unclear in W1, costs in W2 and W3), so the stop rule applies and phase 2 is not run. `BuildOnly` pays nowhere, as the chemistry says it cannot.

Findings, recorded as found (no retuning follows from them):

- **The respiration catalyst almost never binds.** W0 and W1 give the same shares and populations in all nine pairs; the only difference is the second decimal of mean energy in two `NitrateOnly` runs. A seed genome takes at most 1u of light per tick, so it makes at most 1u of sugar, and respiration's cap is already 1u: raising the cap changes nothing. This also explains the archived catalyst dish's failure better than "light-limited".
- **At 320mu each piece alone is about neutral, not a cost** (shares 0.50 after 5_000 ticks). The proposal and `plan-next.md` item 1 said each piece alone is a net cost; at this light level upkeep and fees do not decide who leaves descendants (energy is plentiful, body is short). Costs show only where energy is scarce: at 80mu `NitrateOnly` falls to 1%, at 40mu `BuildOnly` to a third.
- **Where energy is scarce, both pieces together lose too** (`Builder` 7% at 80mu, 0% at 40mu): making body does not pay when energy is short. A world where `take(nitrate)` pays alone would need a benefit other than energy, which existing mechanics do not give (substance effects, `proposals/substance-effects.md`, are the candidate).
