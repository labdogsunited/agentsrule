# Proposal: joining the pieces by gene transfer

Status: archived 2026-10-20. **Step 0 failed, so `share_gene` is not built** (result below). Item 1 of `plan-next.md`. The runner has no recombination and no `share_gene` (`crates/emerge-ref/DISCREPANCIES.md` lists recombination as missing). Adding either one is a semantic change, so no code is written before the user agrees.

## Problem

Producers are held back by joining two gene pieces: a reaction that makes `body`, and `take(nitrate)` to feed it. Evidence:

- **Halfway diagnostic** (spec §15.1, `archive/halfway-dish.em`): with the body reaction already in every genome, 20 of 20 seeds sustain producers, against 7 of 20 for guard `Drifter`.
- **Seed 2 replay** (`archive/plan-next-2026-10.md`, item 4): of 191_952 births, 4_353 carried a body reaction, 2_712 carried `take(nitrate)`, and only 117 carried both (about 61 if independent). One family joined them and made 21 of the 28 producers.
- **Catalysis** did not help: a nitrate catalyst for `respiration` left `take(nitrate)` unused, because energy here is limited by light (`archive/catalyst-dish.em`).
- **More duplication helped**: `duplicate_gene` 0.005 → 0.02 gave 13 of 20. More copies give more chances to join, which fits the same picture.

Why the join is hard: each piece alone is a net cost when expressed (5mu per tick per gene, plus fees, and the reaction does nothing without nitrate). New genes start dormant and need an `expression_flip` (0.01 per division) to run. So a lineage must switch on both pieces before either pays. Both pieces already exist in the population, in different entities. Mutation can only join them inside one lineage.

`genetics-advances.md` (the table near its end) says recombination and horizontal transfer need "their own demonstrated problem and controls". The Halfway result and the 117 joint carriers are that problem.

## Idea

One experimental action, **`share_gene`**, in the same spirit as `guard` (experimental, dated, with a record; spec §5.5): an entity copies one of its own metabolism genes into its nearest neighbour. The recipient then holds a gene from a different lineage, which is how two pieces from different entities meet.

Why transfer and not recombination: recombination needs a mating rule in `divide` (who mates, when, how genomes align), which changes reproduction for every lineage. Transfer is one action that a world can leave out, and it touches only entities that fire it.

## Step 0: is contact enough? (read-only, before any design work)

Transfer can only join pieces if a carrier of each is within `max_interaction_radius` (1.0) of the other. Back-of-envelope, **[spec.]**: with about 12_000 entities on 512 × 512, about 280 body-reaction carriers (2.3%) and 170 `take(nitrate)` carriers (1.4%), a uniformly mixed population has about 0.5 carrier pairs in contact per tick. Entities forage toward `body`, so they cluster, which only raises this. The estimate is rough, so measure it.

- **Measure:** in a replay of guard `Drifter` seed 2 (as in `causal-replay.md`), every 100 ticks count pairs (body-reaction carrier, `take(nitrate)` carrier) within radius 1, using the runner's spatial index (`near.rs`). Read-only; the CSV must not change.
- **Threshold, set now:** fewer than 100 such pairs summed over ticks 0–22_000 means contact cannot join the pieces at a useful rate in this dish. Then record that and stop; do not build `share_gene`.
- Also record the carrier counts per window, so the estimate above can be corrected.

## Design

Everything below is for the chosen mechanism only. Alternatives (recombination, substance effects, cheaper idle genes) are left out on purpose; if this fails, the next proposal can pick one.

1. **Action.** `action share_gene() cost 100mu;` in the world's menu; no parameters, no size. The target is the nearest living entity within `max_interaction_radius`, ties to the lower ID (as `give` and `steal`, spec §5.5). No target: the intent is dropped and pays no fee. Point mutation changes its threshold only.
2. **Seeding.** `Drifter` starts with one `share_gene` gene, fired by a `clock` sense so it acts about once per 50 ticks. The experiment tests whether the transfer helps, not whether evolution invents the means to share. Mutation can duplicate or delete the gene like any other. The exact `clock` wiring is fixed when the dish is written.
3. **Which gene moves.** The donor draws one gene uniformly from its **expressed `reaction` genes and expressed `take` genes** (Philox pass `share = 6`, stream = donor ID, one draw per use). Theft (`steal`), `guard`, `move`, `divide`, senses and regulation genes are never copied. That keeps the experiment about the join, and it keeps theft from spreading by this route. If the donor has no such gene, the intent is dropped and pays no fee.
4. **Arrival.** The copy is **expressed** on arrival, as in Halfway, where the seeded reaction was expressed. (The spec says new material starts dormant, §8.1, but a dormant copy would wait for `expression_flip` and the experiment would measure that wait instead.) Reaction and `take` genes read the constant `1.0` and write no signal, exactly like an `insert_gene` result, so no donor signal needs mapping. The copy gets a new persistent gene ID.
5. **Skips.** The transfer is skipped, and the fee is kept, if: the recipient is at its profile's gene capacity (§5.7); the recipient already has a gene with the same kind and parameters; the recipient is dying this tick.
6. **Phase and order.** Applied in Restructure, after division. A child born that tick inherits its parent's genome as it stood at the start of the tick. Donors are processed by ID. A recipient with several donors takes them in that order, and capacity is checked as it fills.
7. **Conservation.** A gene is not matter. The fee is the only debit and goes where action fees go (`heat`, then the sink).
8. **Inheritance.** A received gene is part of the genome, so children inherit it. Execution plans are rebuilt from the genome (`evolutionary-semantics.md` §6), never the other way round.
9. **Event log.** Each transfer is a row: tick, donor ID, recipient ID, gene ID in the donor and in the recipient, kind, substance or reaction, and whether it was skipped and why. The birth log marks a child's genes that came by transfer, so the ancestry of any family can say which genes arrived that way.

## Experiment and stop rule

Set now, before any run. Runs are `emerge run` with the usual flags (`--lineages Drifter --partial --flux body --producers 0.02 --census 1 --stop-when-sustained --stop-when-hopeless`, 40_000 ticks).

- **Base dish:** guard `Drifter` with `duplicate_gene` 0.005 (7 of 20 sustained), plus the seeded `share_gene` gene and the action. This base has the most room to improve; 13 of 20 (`duplicate_gene` 0.02) is the second reference. Halfway (20 of 20) is the ceiling.
- **Screen first, seeds 1–2, 22_000 ticks.** Count joint carriers among births, as in the replay, from the event log. Baseline: 117 of 191_952 (0.06%) for seed 2. If joint carriers are not at least five times that share (0.3%) in either seed, record it and skip the batch.
- **Batch, seeds 1–20 (screen seeds included):**
  - **14 or more sustained**, and in at least two thirds of the sustained seeds the first complete route in the founding family's ancestry arrived by a transfer event (from the log), not by point mutation: gene transfer is a way to join the pieces.
  - **10 or fewer sustained**: it is not. Record it.
  - **Anything else** (11–13, or 14 or more without that attribution): unclear. Record it, with no second batch. A high count without transfer in the founders' ancestry means the extra gene or the fee changed something else.
- **Not covered:** a sham arm (the action present but always skipped) would separate the effect of the gene and the fee from the transfers. The attribution test stands in for it. Run a sham only if the result is unclear.

## Steps

1. Step 0 measurement (replay with pair counts). Stop here if it fails.
2. `share_gene` in `emerge-lang` (menu item) and `emerge-ref` (action, Restructure step, Philox pass 6, event log), with tests: conservation of the fee; capacity and duplicate skips; order with several donors; determinism with the action absent (the CSV must equal the old binary's); a transferred gene is inherited.
3. Differential check against the old binary on the existing dishes (`--dump`, signals, genomes, census), since the action is absent from them.
4. Write the dish, run the screen, then the batch if the screen passes.
5. Record the result in spec §15.1 and `plan-next.md`; move this file to `archive/` if the idea is closed.

## Risks and limits

- **Contact.** Step 0 may kill the idea. That is a cheap answer.
- **Parasites.** An expressed copy can be a cost to the recipient. Selection removes it, but a flood of transfers could add churn. The log shows how many.
- **Seeded means.** A seeded `share_gene` gene is a head start, like Halfway. It is a fair start for a test of the mechanism, but a positive result does not show that evolution would invent sharing unaided.
- **Fee scale.** 100mu once per 50 ticks is a small drain against upkeep of about 20mu per tick. If a screen shows it matters, change the period, not the stop rule.
- **Genome changes during life.** The runner so far changes a genome only at division. A transfer changes a living entity's genome, so caches keyed by genome (execution plans, census hashes) must be rebuilt. This is the main implementation risk.
- **A story is not a proof.** One positive batch shows that this dish, with this seeded gene, joins pieces faster. It says nothing yet about recombination or about other worlds.

## Result of step 0 (2026-10-20)

Guard `Drifter`, seed 2, 22_000 ticks, `emerge run … --contact FILE` (new read-only flag; the main CSV is identical with it on and off). Data: `archive/contact-seed2.csv`, one row per 100 ticks.

- Carriers were far fewer than the estimate above: about 7_700 entities, 10–40 with a body reaction and 26–93 with `take(nitrate)` (estimate: 280 and 170). At most 12 were joint carriers in any sample.
- Pairs (reaction carrier next to a `take(nitrate)` carrier) over 220 samples: **57**. Pairs where a transfer could make a new joint carrier: **2**. The threshold was 100.
- So contact cannot join the pieces at a useful rate in this dish. By the rule set before the run, the stop is final: no `share_gene`, no batch. Recombination would need the same contact (a mate must be a neighbour), so this result argues against it too, unless it works at a distance.
