# Proposal: rounds (for spec v0.7.8)

Status: proposed, not implemented. When it is folded into the spec, `core.md` §6 and `language.md`, move this file to `archive/`.

## Problem

Every organism acts once per tick. Substeps (`core.md` §6.3, `mind_steps`) let a mind think several times per tick, but a node still moves, takes and drops only once. So a world cannot have a fast predator that sees its prey move and turns to follow it within one tick. A larger `move(speed: …)` covers more distance, but it is blind between its start and its end.

Actions cannot simply repeat inside substeps. They touch shared stores, so every repetition needs its own arbitration.

## Proposal

### 1. Rounds per tick

A world declares how many **rounds** a tick has:

```
rounds: 3;      // world setting, default 1, at most 16
```

A tick then runs:

1. Set and Move, once per round, for rounds `1..R`;
2. Transform, once;
3. Restructure and rewrite, once.

Each round is a complete Set and Move with its own arbitration. Charges (2.1, such as upkeep) are settled only in round 1. A node that fails a charge is failing for all rounds of the tick.

### 2. Who acts in which round

A profile says how many rounds its lineage acts in. A profile already sets how a genome is executed (spec §5.7), so this belongs there:

```
profile fast { genes: 32; signals: 16; modules: 8; rounds: 3; }   // default 1
```

In the core, a kind declares it next to its substeps: `owned rules (…) substeps 2 rounds 3;`.

A node with `rounds: k` acts in round `r` when `floor(r × k / R) > floor((r − 1) × k / R)`. This spreads its rounds evenly and always includes the last round:

| R | k = 1 | k = 2 | k = 3 | k = 4 |
| :-- | :-- | :-- | :-- | :-- |
| 3 | 3 | 2, 3 | 1, 2, 3 | — |
| 4 | 4 | 2, 4 | 2, 3, 4 | 1, 2, 3, 4 |

World rules (laws) act once per tick, in the last round, by default. A rule can declare more rounds, and its amounts are then per round:

```
rule spread on cells rounds 3 { … }
```

So `rounds: 1` everywhere behaves exactly as today.

### 3. Each round has its own snapshot

At the start of each round, the state is captured again: positions, headings, balances and cells. Derived edges and each node's cell are computed from it. Within a round, everything reads that **round snapshot**, as a tick reads its snapshot today. Signals carry over between rounds. The mind runs its `mind_steps` in every round in which its node acts. A node does nothing in a round in which it does not act, and its signals stay as they are.

Paid sets (2.4, such as movement) are applied at the end of each round, so the next round sees the new positions. This is the purpose of rounds.

### 4. Amounts are per action

`take(max: …)`, `move(speed: …)` and every fee apply each time the action runs. A node that acts in 3 rounds can take and move up to 3 times as much per tick, and pays 3 times the fees. Upkeep is per tick. A world can price speed with a new upkeep term:

```
upkeep { … per_round: 10mu; }   // charged once per round the node acts in
```

### 5. What stays once per tick

- **Transform.** Reactions run once, on the balances after the last round. How fast a node's chemistry runs is set by its reaction rates, not by how often it acts.
- **Restructure.** Death, disposal, division and mutation happen once. Conditions read the values after Transform, as today.
- Age, venting and the birth log count ticks, not rounds.

### 6. Determinism

Arbitration draws in round `r` use `tick × R + (r − 1)` as the tick word of the Philox counter (spec §10.2). Every other pass keeps `tick`. With `R = 1` this is `tick`, so existing runs and fixtures do not change. A run is limited to `2^32 / R` ticks.

Each round's arbitration is the existing two-stage arbitration, on the balances the previous round left. No new atomics are needed, and CPU and GPU loop over rounds in the same way.

### 7. Changes to `core.md` §6.2

- Rows 1.1 to 2.4 read the **round snapshot** where they now read the snapshot, and run once per round. Row 2.1 runs in round 1 only.
- Rows 3 and 4 read the values after the last round.
- A write in 2.3 or 2.4 is visible to the next round.

### 8. Cost

Set and Move cost grows with the number of (node, round) pairs in which a node acts, plus one rebuild of the spatial index per round. Nodes are binned by `k`, so a node skips the rounds it does not act in at no cost. A world where only one lineage is fast pays mostly for that lineage.

### 9. Language

- World setting `rounds` (`language.md` §6.6), profile field `rounds` (§8.1), and a `rounds k` clause on rules (§7.2). `rounds` is a contextual keyword.
- `per_round` in `upkeep`.
- Errors: a profile or rule with `rounds` outside `1..R` is E0204. `R` above 16 is E0204.

### 10. Tests that make it done

1. With `rounds: 1`, the Dish fingerprints in `tests/compile.rs` do not change.
2. The schedule matches the table in item 2.
3. In a world with `rounds: 2`, a node with `rounds: 2` drops a substance in round 1, and a node with `rounds: 1`, which acts only in round 2, takes it in the same tick.
4. A pursuer with `rounds: 3` catches a target that a pursuer with `rounds: 1` and `move(speed: 3.0)` misses.
5. Upkeep is charged once per tick and `per_round` once per active round. Conservation is exact with `R = 3`.

## Companion: slow rules

The opposite case is rules that run less often, such as seasons or slow diffusion:

```
rule seasons on cells every 100 ticks { … }
```

The rule runs in ticks where `tick mod N = 0`, in the last round. Its amounts are per run, and nothing else changes. It saves compute, and it is easy to add with rounds.

## Open questions

- **Evolvable speed.** Should `rounds` be able to mutate, so speed is a trait under selection? That needs `k` per node rather than per profile. It is cheap on CPU, but on GPU binning would follow genomes. Leave it until a world needs it.
- **Reactions per round.** This proposal runs Transform once. A world where a fast node must turn food into energy between rounds would need Transform in every round. No current world needs that.
