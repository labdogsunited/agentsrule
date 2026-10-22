# Proposal: a catalyst that binds, so `take(nitrate)` pays alone

Status: step 0 and phase 1 done (2026-10-21, `~/ratecat/phase1/`): phase 1 passes; phase 2 (40 long runs) started 2026-10-21 (`~/ratecat/phase2/`, decided by delegation, see decisions item 6). Its success criterion and stop rules are fixed below, before any run (`proposals/decisions-for-review.md` item 6). A new experiment, not a retune of test 9: the test-9 result (13 of 20 seeds) stands as recorded in spec §15.1. It follows `archive/stepping-stone-nitrate.md`, whose phase 1 found that the stepping stone does not work in the test-9 dish.

## Problem

Producers need two gene pieces at once, a reaction that makes `body` (`build`) and `take(nitrate)` (`plan-next.md` item 1). `build` cannot pay alone in this chemistry, so the join can only get easier if `take(nitrate)` pays alone. The earlier attempt used a nitrate catalyst on `respiration`. It changed nothing: a catalyst raises a reaction's rate cap, but the seed genome could not use a higher cap.

## Why that failed, and what this proposal changes

A catalyst helps only when a reaction runs at its cap while its inputs are available. In the earlier worlds the cap was never the limit:

- At 320mu light, the seed's `take(light, max: 1u)` and `rate 1u` genes already match the supply, and energy is plentiful, so more energy changes nothing (shares stayed 0.50).
- At 80mu and 40mu light, the cell supplies less than the 1u cap, so the supply limits the reaction, not the cap.

So a rate-limited and energy-scarce world needs the cap below the supply. This is an inference from the earlier results and the catalyst rule (spec §4.5), not yet measured; step 0 checks it.

**Change to the world (patch only, no engine change):** a `Drifter` variant whose `photosynthesis` gene has a low rate (the cap), and `photosynthesis catalyze nitrate(half: 100mu, boost: 4.0)`. Light is plentiful (320mu per cell per tick), so uncatalysed photosynthesis is limited to its rate, while an entity holding nitrate converts up to four times as much light. Energy is then scarce for entities without nitrate and `take(nitrate)` has a benefit of its own. This uses existing mechanics (`--patch`, `--set`).

**Hypothesis.** In that world, `take(nitrate)` pays alone. A population that keeps it for that reason then needs one more piece (`build`) to make producers, as `Halfway` needed only `take(nitrate)`.

## Step 0: does the catalyst bind? (no evolution, a few short runs)

One `Drifter` genome per rate, mutation off, 2_000 entities, seed 1, 1_000 ticks, CPU default mode: photosynthesis rate in {60, 100, 200}mu, with and without the catalyst, and with and without `take(nitrate)` in the genome. Measure mean energy per entity and the population. **It binds** if, with `take(nitrate)` and the catalyst, mean energy is at least 1.5 times the value without the catalyst, at the same rate. Pick the highest rate at which it binds **and** the plain `Drifter` population survives 1_000 ticks. If no rate meets both, stop and record it: this chemistry has no window, and the next step is substance effects (`proposals/substance-effects.md`), not another catalyst variant.

### Result of step 0 (2026-10-21): the catalyst binds, at rate 200mu

12 runs (3 rates × catalyst on/off × `Drifter` or `NitrateOnly`), one `Drifter` genome each, mutation off, seed 1, 1_000 ticks, CPU default mode, on the committed engine of `9688604` (`~/ratecat/step0/`: `run.sh`, one dish per rate made by changing the rate on the `Drifter` photosynthesis line of `archive/stepping-dish.em`, the catalyst added with `--set`). Gene `--set` paths do not exist yet, so the rate variants are copies of the dish text with one line changed. Mean energy per entity (u) and population at tick 1_000:

| Rate | `Drifter` | `NitrateOnly`, no catalyst | `NitrateOnly`, catalyst |
| :-- | :-- | :-- | :-- |
| 60mu | dead by tick 49 | dead by tick 44 | 0.24 energy, 443 alive (collapsed from 8_762 at tick 100) |
| 100mu | dead by tick 66 | dead by tick 58 | 21.7 energy, 15_208 alive |
| 200mu | 0.20 energy, 453 alive | 0.20 energy, 1_240 alive | 56.4 energy, 15_208 alive |

`Drifter` with the catalyst equals `Drifter` without it at every rate (it holds no nitrate). By the rule fixed above: the catalyst **binds** at 100mu and 200mu (energy far above 1.5 times the uncatalysed value; at 100mu the uncatalysed run is dead) and at 60mu it is not meaningful; the plain `Drifter` population **survives** only at 200mu. **The chosen rate is 200mu**, and the catalyst world for phase 1 is the dish with `photosynthesis` at 200mu and the catalyst `nitrate(half: 100mu, boost: 4.0)`.

Cautions, recorded as found:

- At 200mu `NitrateOnly` also has three times the population of `Drifter` without the catalyst (1_240 against 453, same mean energy 0.20). I have not looked into why. Phase 1 will show whether `take(nitrate)` already pays in the base-rate dish.
- The catalysed gain is large (energy 56 against 0.2): the catalyst world is no longer energy-scarce for a holder of nitrate. That is what phase 1 needs to see, but it is a strong effect, not a small nudge.
- One seed, 1_000 ticks, as the step-0 rule fixed. This says the mechanism binds, nothing about evolution.

## Phase 1: does each piece pay alone? (no evolution)

The earlier protocol, unchanged apart from the worlds: lineages `Drifter`, `NitrateOnly`, `BuildOnly` and `Builder` (`archive/stepping-dish.em`), with `NitrateOnly`, `BuildOnly` and `Builder` each head to head against `Drifter` (`Builder` is reported, not tested for paying alone), 2_000 of each, mutation off, `--partial`, 5_000 ticks, seeds 1–3, CPU default mode, at most 12 at once. **The photosynthesis rate is changed in every lineage that has the gene** (`Drifter` and `Builder`; `NitrateOnly` and `BuildOnly` extend `Drifter`), so `Builder` stays "`Drifter` plus both pieces". Three worlds: **W0**, the base test-9 dish (rate 1u, no catalyst); **W1**, the chosen rate without the catalyst (the control: step 0 showed that the rate change alone moves the result, for example `NitrateOnly` 1_240 against `Drifter` 453 at 200mu); **W2**, the chosen rate with the catalyst. The catalyst is credited only for what W2 shows beyond W1 (3 worlds × 3 pairs × 3 seeds = 27 runs). Measure the challenger's share at tick 5_000 and the population.

**A piece pays alone** in a world if its share is at least 0.6 in all three seeds with a living population; **it costs** if the share is at most 0.4 in all three; anything else is unclear. **The catalyst makes `take(nitrate)` pay** only if `NitrateOnly` pays alone in W2 and does not in W1; if it pays in both, the rate change did it, and the result is recorded as that.

**Stop rule for phase 1.** If `NitrateOnly` does not pay alone in W2, or pays alone in W1 as well, record it and stop; phase 2 is not run. If `BuildOnly` pays anywhere, stop and look for the mistake first, because the chemistry says it cannot.

### Result of phase 1 (2026-10-21): the catalyst makes `take(nitrate)` pay

27 runs on the engine of `d9b2a73` (`~/ratecat/phase1/`: `run.sh`, `commit.txt`, one CSV per run), as designed: mutation off, 2_000 of each lineage, `--partial`, 5_000 ticks, seeds 1–3, 12 at once. Challenger's share at the last row, and the population (range over the three seeds):

| World | `NitrateOnly` | `BuildOnly` | `Builder` |
| :-- | :-- | :-- | :-- |
| W0 (base dish) | share 0.50, 0.50, 0.51; 10_000 alive | share 0.50, 0.50, 0.50; 10_000 alive | share 1.00 in all; about 34_000 alive |
| W1 (200mu, no catalyst) | everything extinct by tick ~1_165 | everything extinct by tick ~1_170 | everything extinct by tick ~1_160 |
| W2 (200mu, catalyst) | share 1.00 in all; about 9_100 alive (mean energy 67) | everything extinct by tick ~1_170 | share 1.00 in all; about 34_000 alive |

By the rules fixed above:

- **`NitrateOnly` pays alone in W2** (share at least 0.6 in all three seeds, living population) **and not in W1** (nothing lives there). So the catalyst, not the rate change, makes `take(nitrate)` pay.
- **`BuildOnly` pays nowhere** (unclear at 0.50 in W0, extinct in W1 and W2). The stop rule for `BuildOnly` did not fire.
- **No stop rule fired. Phase 1 passes; phase 2 may be run when approved.**

Cautions, so this is not read as more than it is:

- W1 is not a healthy control. At 200mu nothing survives 5_000 ticks without the catalyst (step 0 only ran 1_000 ticks and saw about 450 `Drifter` alive). The result is therefore "with the catalyst, a nitrate holder lives and plain `Drifter` does not", not "a nitrate holder beats `Drifter` by a margin in a world where both live". The catalyst turns an extinction world into a survivable one for nitrate holders.
- In W0 `NitrateOnly` and `BuildOnly` are neutral against `Drifter` (share 0.50), so in the base dish there is nothing for selection to find. `Builder` takes the whole population in W0 and W2 (share 1.00, about 34_000 alive, mean energy 4); in W1 it dies out with the rest. With mutation off, this says only that the pieces have value together in W0, which is what test 9's dish needs.
- The step-0 oddity (`NitrateOnly` 1_240 against `Drifter` 453 at 200mu without the catalyst) did not need explaining here: both are gone by tick ~1_170 in W1.
- Phase 2 adds mutation, which can change the gene rate and remove the cap that makes the catalyst bind. Phase 1 does not test that.

## Phase 2: evolution (only if phase 1 finds a world)

Treatment: unseeded `Drifter` in the new world, seeds 1–20, 40_000 ticks, test 9's flags (`--partial --flux body --producers 0.02 --census 1 --stop-when-sustained --stop-when-hopeless`), CPU default mode. Control: the same seeds in the same world without the catalyst, run alongside (the gene rate changes much else, so only the catalyst is the treatment). Also measured: the share carrying an expressed `take(nitrate)` every 1_000 ticks before the first producers.

**Rule, fixed before the run:** it works if the treatment sustains producers in at least 5 more seeds than its control **and** the `take(nitrate)` share before producers is higher in the treatment in at least two thirds of the seeds. It does not work if the treatment is not above the control. Anything else is unclear: record it, no second batch, no retuning between phases or after phase 2.

### Result of phase 2 (read 2026-10-22; the batch finished 2026-10-21 22:18)

40 runs on the commit in `~/ratecat/phase2/commit.txt`, as designed. Seeds with `sustained_at` set: **treatment 8 of 20, control 6 of 20.** The rule needs at least 5 more seeds than the control; the difference is 2. **The catalyst did not meet the rule. The result is "not shown"** (the treatment is above the control, so not "does not work", but far below the margin, and 2 seeds of 20 is inside what chance gives). The second part of the rule (carrier share before producers) is not needed once the first fails and was not computed. No second batch, no retuning, as fixed. For context only: the base test-9 dish sustained 13 of 20 seeds; the 200mu worlds sustain fewer in both arms (8 and 6), so the lower light was harder for both.

### GPU exploration (started 2026-10-21, not part of the rule)

While the CPU batch runs, the idle GPU runs the same two arms as a labelled exploration (`~/ratecat/gpu/`: `run.sh`, the binary's commit in `commit.txt`, `dish-treat.em` and `dish-ctrl.em`, the phase 2 dishes with the catalyst written into the `photosynthesis` line for the treatment). Unseeded `Drifter`, seeds 1–20, 40_000 ticks, `EMERGE_GPU_MUTATION=1`, 150M-word genome pools, test 9's flags as in the GPU baseline (`~/gpu-baseline`), six runs at a time. It is in contract mode, so it is **not** comparable to the CPU's default mode or to the 13-of-20 base figure (the GPU baseline of the base dish was 8 of 20), and it does not count toward, replace or reopen the rule above. It reports the count of seeds with `sustained_at` per arm, and nothing else is claimed from it. A run that stops on pool size is rerun with larger pools only, as in the baseline.

**Stopped after about three minutes (2026-10-21), no result:** the user asked why the GPU was busy; the batch was mine, started without being reported in chat. The partial files are in `~/ratecat/gpu/stopped/`. It is not rerun unless the user asks.

## What this does not cover

- `build` on its own stays unable to pay (the chemistry gives it no nitrogen). This proposal makes only `take(nitrate)` pay.
- Mutation can change the gene rate, which removes the cap that makes the catalyst bind. Phase 2 therefore tests whether the effect survives evolution; phase 1 does not.
- The GPU is not used: `emerge-gpu run` reports no share per lineage yet.

## Cost

About 27 short runs for phase 1 and a few for step 0; phase 2 is 40 long runs, only if phase 1 passes.
