# Emergent: a Rule-Based Language for Open-Ended Evolution

**Architecture & Design Specification — v0.5**

Status: draft. Supersedes v0.4. This version applies the v0.4 review (`archive/spec-v0.4-review.md`). The main changes are a new **dissipation law** based on substance potentials (§3.2), exact units (§3.5), transactional arbitration and reactions (§7), and a reference world that can actually evolve (§15). Appendix A lists every change.

Conventions:
- **MUST** / **MUST NOT** mark normative requirements.
- **[P0]** / **[P1]** / **[P2]** mark the phase in which a feature arrives (§16).
- `emerge` is a placeholder name for the CLI and engine; source files use the `.em` extension.
- In prose, `µ` is a micro-unit. In source code it is spelled `mu` (§3.5).

---

## 0. Product decisions

| Topic | Decision |
| :-- | :-- |
| Primary user | One researcher/tinkerer (the author), working with AI assistants |
| First experiment | Evolving creatures |
| Goals, in priority order | 1. Open-ended surprise · 2. Beautiful visuals · 3. Goal-directed evolution · 4. Understanding rule spaces |
| What an entity is | An abstract thing whose every capability is defined by rules, all of which can evolve |
| Fixed laws (unbreakable) | Conservation · Dissipation · Cost of living · The world rules the user writes |
| Not fixed | Locality: non-local interaction is allowed when a world enables it |
| Flexibility | The language provides mechanisms and each experiment chooses policies: space, interaction types, and scale vs. freedom |
| Metabolism | Abstract, user-defined "chemistry": substances with potentials plus transformation rules, evolvable |
| Ruliology | All three meanings: the world is made of local rules; rules rewrite programs; rule spaces are explored systematically |
| Authoring | Node graphs plus AI. **AI-friendly matters more than human-friendly** |
| Live changes | Rules can be hot-swapped while a world runs |
| AI roles | Author, observer, experimenter, live director |
| Platform | Browser front end; engine runs natively on the desktop CPU/GPU |
| Exact replay | Nice to have, not a gate. Tracked in P0 but does not block it (§16) |
| Team and timeline | Solo plus AI, over several months |

---

## 1. Design principles

1. **Everything is a rule.** World physics, chemistry, senses, actions, decisions and mutation are all expressed as rules over local state. There is no hard-coded "creature".
2. **Four laws, enforced by the runtime.** Evolved or hot-swapped rules may do anything except:
   - create or destroy substance (conservation);
   - run a reaction uphill (dissipation);
   - avoid paying upkeep (cost of living);
   - change the world rules.

   These checks happen in the compiler and runtime (§3), not by convention.
3. **Mechanisms, not policies.** The world declares *what is possible*: substances, potentials, actions, senses, channels and space. Entities evolve *which of those possibilities they use and how*.
4. **Open-endedness first.** Defaults favour diversity and novelty: variable genomes, neutral drift, rich action menus, generated chemistry. Built-in metrics report whether anything new is happening.
5. **AI-native.** The text format, API, diagnostics and views are designed so that an AI can author, run, observe and steer worlds without a human in the loop (§12).
6. **Watchable.** Every world can be visualized natively; visualization is part of the language (§11).
7. **Explicit contracts.** Every quantity has a unit, every phase has a defined order, and every limit is declared. Two correct implementations of this spec must agree on the ledger.

---

## 2. Core concepts

| Concept | Meaning |
| :-- | :-- |
| **World** | One running simulation: spaces, substances, world rules, entities |
| **Space** | Where things are: `Continuous2D`, `Grid2D`, `Graph`, and later `Continuous3D` |
| **Field** | A grid laid over a space. Each cell holds substances (conserved) or signals (free information) |
| **Element** | An abstract, indivisible unit of conservation (for example `C`, `N`, `E`) |
| **Substance** | A named composition of elements with a **potential** (for example `sugar = C + E potential 9`). Amounts are `u32` µ |
| **Potential** | An integer "free energy" per µ of a substance. Reactions must lower the total potential (§3.2) |
| **Signal** | Non-conserved `f32` information: smells, messages, internal registers |
| **Store** | Anything inside the world that holds substances: a field cell or an entity's inventory |
| **Source / sink** | A **ledger boundary** outside the world. Sources emit substances into stores; sinks absorb them. They are not stores and hold no balance |
| **World rule** | A user-authored rule the world runs every tick. Evolution cannot change it; the user or AI can hot-swap it |
| **Entity** | An abstract agent with a stable ID, a position, an inventory, registers and a **genome** |
| **Genome** | The entity's evolvable rules: sense genes, reaction genes, action genes, a brain program, and optional bond and message genes |
| **Lineage** | A template that seeds entities. "Species" are detected from genome similarity at runtime, not declared |

---

## 3. The four fixed laws

### 3.1 Conservation

- **The ledger counts elements, not substances.** A reaction may turn substances into other substances only if the element totals on both sides are equal. For every element `e`, after every tick:

  ```
  Σ_stores amount_e  −  emitted_e  +  absorbed_e  ==  0
  ```

  `emitted_e` and `absorbed_e` are cumulative `u64` totals at the source and sink boundaries. Sources and sinks hold no balance, so nothing is counted twice.
- **Fixed-point arithmetic.** Amounts are `u32` µ (`1u = 1_000 µ`). Totals and products use 64-bit arithmetic built from 32-bit halves, so shaders never need native 64-bit integers or float atomics.
- **Caps are per substance.** Every store declares a cap for each substance it holds, at most `2^31 µ`. A store that does not list a substance cannot hold it.
- **Nothing is silently lost.** A transfer that does not fit stays at its origin, unless the transfer names an `overflow` sink that accepts that substance (§7.2).
- **Initial stock** comes only from sources, in `genesis`.
- **Audits.** The ledger is checked every tick in debug builds and every `K` ticks in release builds. An imbalance halts the world and reports which element drifted and in which tick. The runtime also halts before any cumulative total would exceed `2^63`.
- **Hot-swaps** (§13) that would remove stores or substances must say where the contents go. Otherwise the swap is rejected.

### 3.2 Dissipation

Element conservation alone treats `light`, `energy` and `heat` as interchangeable, because all three are one `E`. That allows perpetual motion, such as `heat → energy`. The dissipation law prevents it.

- **Every substance declares an integer potential**, per µ: `substance sugar = C + E potential 9;`. A substance without a potential is a compile error.
- **Reaction potential change.** For a reaction with integer coefficients, `ΔG = Σ_out coeff·potential − Σ_in coeff·potential`.
- **Every reaction MUST be downhill:** `ΔG ≤ −min_drop`. `min_drop` is declared once per world (`second_law { min_drop: 1 }`, default `1`) and is at least `1`. This applies to hand-written reactions, generated reactions, world reactions, and the upkeep conversion (§3.3). The compiler rejects an uphill reaction with a diagnostic that shows its `ΔG`.
- **An uphill step must be coupled to a downhill one in the same reaction.** For example, `carbon + nitrate → body` is uphill and is rejected, while `carbon + nitrate + energy → body + heat` is downhill and is allowed. This is how a world says "building costs energy".
- **Transfers, diffusion and movement are not reactions** and do not change potential.

What dissipation does and does not do:

- It **removes free shortcuts**: any chain of reactions that would return to its start with net potential gained.
- It **does not remove shortcuts that are downhill.** If a chain `light → sugar → energy` is allowed, then the direct reaction `light → energy` is also downhill, because potential is path-independent. Such shortcuts are legitimate physics in the world. A world that wants to discourage them uses kinetics (`generated_max_rate`, §4.3) or excludes them explicitly (`exclude`, §4.3).

### 3.3 Cost of living

Every entity pays **upkeep** every tick. The world declares what upkeep is paid in, what it becomes, and where that goes:

```
upkeep { pay: energy, becomes: heat, vent: void,
         base: 20mu, per_gene: 5mu, per_brain_node: 1mu, per_carried_u: 1mu }
```

- **Upkeep is a reaction run by the runtime:** `pay → becomes`, which **MUST** have the same element composition and be downhill (§3.2). The product goes straight to the `vent` sink, which must accept it.
- **What is counted.** `per_gene` applies to expressed genes only. `per_brain_node` applies to active brain nodes only (§5.4). `per_carried_u` applies to each whole `1u` carried, over all substances, rounded down.
- **Rounding.** The total is computed in `u64` and is never fractional, because every coefficient is an integer number of µ.
- **Order.** Upkeep is charged first in Arbitrate (§7.2), before any action cost, so a genome cannot skip it.
- **Failure.** An entity that cannot pay in full is marked **dying** at once (§7.2) and does nothing else that tick.
- **Changes** to upkeep by hot-swap apply to every entity from the next tick (§13).

Because cost grows with expressed genes and active nodes, bloat is selected against, while dormant genes and introns stay free and keep neutral drift affordable.

### 3.4 World rules are immutable to evolution

Evolution only changes genomes. World rules, the menus of senses and actions, substances, potentials, elements, limits and upkeep all live outside genomes. Mutation operators have no way to reach them. Only the user or an AI can change them, through a hot-swap (§13), and every such change is recorded in the run journal.

### 3.5 Units and arithmetic

Every number in an `.em` file has an explicit unit. The compiler rejects a bare number where a unit is expected.

| Quantity | Spelling | Stored as |
| :-- | :-- | :-- |
| Substance amount | `5u` (units) or `250mu` (µ); `1u = 1_000mu`. `0.5u` is allowed only if it is a whole number of µ | `u32` µ |
| Per-tick fraction (rates of decay, diffusion, world reactions) | `50bp` (basis points; `10_000bp = 100%`) | `u32` bp |
| Reaction rate of a gene | `500mu` per tick of reaction extent | `u32` µ |
| Potential | plain integer after `potential` | `i32` |
| Space distance, speed, angle | `f32` with the unit in the parameter name (`speed`, `angle_deg`, `dist`) | `f32` |

- **Amounts from fractions round down.** `amount × bp / 10_000` uses `u64` and floors. The remainder stays where it was.
- **Costs never round in the entity's favour.** Costs that depend on an `f32` (for example `move` per distance) are rounded up to whole µ.
- **Senses report amounts in units** as `f32` (`inventory(body)` returns `12.5` for `12_500 µ`).

---

## 4. Chemistry: substances and reactions

### 4.1 Declaring a chemistry

```
element C, N, E;                  // E is energy

second_law { min_drop: 1 }

substance light   = E      potential 10;
substance energy  = E      potential 8;
substance heat    = E      potential 0;
substance sugar   = C + E  potential 9;
substance carbon  = C      potential 0;
substance nitrate = N      potential 0;
substance body    = C + N  potential 5;
```

A substance's composition counts elements per µ: `1 µ` of `sugar` holds `1 µ` of `C` and `1 µ` of `E`.

### 4.2 Reactions

```
reaction photosynthesis : carbon + light            -> sugar;          // ΔG = −1
reaction respiration    : sugar                     -> carbon + energy; // ΔG = −1
reaction build          : carbon + nitrate + energy -> body + heat;    // ΔG = −3
reaction rot            : body                      -> carbon + nitrate; // ΔG = −5
```

- The compiler **MUST** reject a reaction whose element totals don't balance, or whose `ΔG > −min_drop`.
- Every reaction has a **stable ID**. Generated reactions get IDs derived from their canonical form, so the same reaction has the same ID across compilations.
- A reaction runs in a **place**: a field cell (world rule) or an entity's inventory (gene).
- **Extent.** A reaction runs in whole steps of **extent**. One step of extent consumes `coeff` µ of each input and produces `coeff` µ of each output.
- **World reactions** are world rules with a declared rate in `bp`, for example `react(rot, rate: 50bp)`. The extent is `floor(amount of the limiting input × rate / 10_000 / coeff)`.
- **Entity reactions** are genes. Each reaction gene names a reaction and has an evolvable rate in µ of extent per tick, capped by the reaction's `max_rate`. The scheduling rules are in §7.3.

### 4.3 Evolvable chemistry

Each world declares a **reaction space**, the set of reactions that entity genes may point to:

```
reaction_space {
    include: [photosynthesis, respiration, build];        // hand-written reactions
    generate: balanced(max_inputs: 3, max_outputs: 3, max_coeff: 2);
    max_rate: 1u;                                         // cap for included reactions
    generated_max_rate: 250mu;                            // cap for generated reactions
    exclude: [];                                          // reactions that may not be generated
}
```

- `generate: balanced(...)` enumerates every reaction over the declared substances that balances its elements **and** passes the dissipation law. `generate: none` restricts entities to the `include` list.
- **Kinetics.** `generated_max_rate` lets a world make its hand-written reactions faster than generated ones, like enzymes. Evolution can still use any generated reaction, but the curated chemistry has an advantage. Omitted, it equals `max_rate`.
- **Exclusions.** `exclude` lists reactions, by name or by formula (`light -> energy`), that are never generated.
- The compiler enumerates the space into an indexed table and orders it so that neighbouring indices are similar reactions. The table size **MUST NOT** exceed `limits.max_reaction_space` (§5.6); a larger space is a compile error that reports its size.
- A mutation of a reaction gene moves its index to a nearby entry or a random one. Evolution can discover reactions the user never wrote, while every reaction stays balanced and downhill by construction.

---

## 5. Entities and genomes

### 5.1 Menus: the world declares, the genome chooses

The world declares menus of possible **senses** and **actions**. Genes select items from the menus and set their parameters.

```
senses {
    sense field(f: any field, angle_deg: -180..180, dist: 0..8);  // value of f at a point relative to heading
    sense gradient(f: any field);                                 // 2 lanes
    sense inventory(s: any substance);                            // in units
    sense crowd(radius: 0..6);                                    // number of neighbours
    sense neighbour(s: any substance, rank: 0..3);                // nth-nearest neighbour's amount of s, in units
    sense channel(ch: any channel);                               // [P1] most recent message value
    sense clock(period: 1..1000);                                 // sin(2π · tick / period)
}

actions {
    action move(speed: 0..2.0)                    cost energy 20mu per distance;
    action turn(angle_deg: -30..30)               cost energy 10mu;
    action take(s: any substance, max: 0u..10u)   cost energy 5mu;   // from the entity's field cell
    action drop(s: any substance, max: 0u..10u)   cost energy 0mu;   // into the entity's field cell
    action give(s: any substance, max: 0u..10u, to: neighbour)   cost energy 5mu;
    action steal(s: any substance, max: 0u..10u, from: neighbour) cost energy 30mu;
    action emit(sig: any signal, max: 0..10.0)    cost energy 2mu;
    action send(ch: any channel, value)           cost energy 10mu;  // [P1]
    action divide(split: 0..1)                    cost energy 0mu requires body >= 10u;
    action bond(to: neighbour)                    cost energy 50mu;  // [P2]
}
```

- An action gene fires when its brain output is above its threshold (a gene parameter, default `0`).
- The **amount** of a firing action is `clamp(output, 0, 1) × max`, rounded down to whole µ.
- An experiment that doesn't want an action (for example `steal`) simply leaves it out. A world **MUST NOT** list a menu item from a later phase than the one the engine implements.

### 5.2 Gene types

| Gene | Selects | Produces |
| :-- | :-- | :-- |
| **Sense gene** | A sense from the menu, plus its parameters | Two brain input lanes (the second is `0` for one-lane senses) |
| **Reaction gene** | A reaction ID from the reaction space, plus a rate | Chemistry inside the entity's inventory |
| **Action gene** | An action from the menu, a threshold, and fixed parameters | When it fires, an intent with an amount derived from its output lane |
| **Brain** | A CGP program (§5.4) | Maps input lanes to output lanes |
| **Mutation gene** [P2] | A rewrite rule and its weight | Lets the mutation operators themselves evolve ("evolution of evolvability") |

Each gene has an `expressed` bit. Unexpressed genes are inherited and mutated but cost no upkeep, produce nothing, and a single mutation can switch them back on. An unexpressed sense gene writes `0` to its lanes; an unexpressed action gene emits no intent; an unexpressed reaction gene does not run.

### 5.3 Execution profiles: scale or freedom

Each lineage chooses a profile. Several profiles can coexist in one world.

| | `profile fast` [P0] | `profile free` [P1] |
| :-- | :-- | :-- |
| Genome shape | Fixed number of slots per gene type, fixed brain size | Variable length; genes can be duplicated, deleted and inserted; the brain can grow up to the CGP bound (§5.4) |
| Lane wiring | Fixed: slot `i` owns its lanes whether expressed or not | Positional: lanes shift when genes are inserted or deleted |
| Mutation operators | Point mutation, expression flips, rewrite rules | All of `fast`, plus duplication, deletion, insertion and transposition |
| Execution | GPU lockstep, agents binned by genome hash (v0.3 §6) | GPU with arena-allocated genomes, binned by length and shape; CPU fallback |
| Target scale | 100k+ entities | 1k–20k entities |
| Best for | Visual spectacle, statistics, large ecologies | Open-ended surprise, new capabilities appearing over time |

`fast` and `free` share the same gene types, the same menus and the same four laws. A genome can migrate from `fast` to `free` (it is padded) but not the other way.

**Lane layout in `fast`.** With `S` sense slots, `A` action slots and `R` registers:

- inputs `in[2i]` and `in[2i+1]` belong to sense slot `i`, for `0 ≤ i < S`;
- inputs `in[2S + r]` are registers `r`, for `0 ≤ r < R`;
- outputs `out[j]` drive action slot `j`, for `0 ≤ j < A`;
- outputs `out[A + r]` write registers `r`.

**Unused slots.** When a seed genome fills fewer slots than the profile has, the remaining slots are filled with **unexpressed** genes drawn uniformly from the menus and the reaction space, using the lineage's Philox stream. A single expression flip can then switch one on.

### 5.4 Brain: positional CGP

The brain is the CGP design from v0.3 §5:

- Inputs and outputs are scalar lanes.
- Each node is 8 bytes (`opcode: u8`, `src_a/b/c: u8`, `param: f32`), using 16 protected, branch-free opcodes. NaN and ±Inf become `0`, and values are clamped as in v0.3 §5.3.
- Output genes, an active bitmask, and no compaction (so homology between genomes is preserved).
- **Bound:** with `I` input lanes and `N` nodes, `I + N ≤ 256`, and `N ≤ 64` (the width of the active mask). The compiler rejects a profile that exceeds either. A wider encoding for `free` is an open question (§17).
- A node is **active** if it is reachable from an output lane read by an expressed action gene or by a register.

In the `free` profile, node insertion and deletion keep the graph feed-forward by renumbering operand references.

### 5.5 Registers and memory

Each entity has `R` persistent `f32` registers (default 4). Register outputs computed in tick `t` are committed at the end of Think and read as inputs in tick `t + 1`. This gives entities memory and internal state without loops.

### 5.6 Limits

Every world declares its limits. The compiler checks static limits and the runtime enforces dynamic ones.

```
limits {
    max_entities: 200_000;         // division fails when reached
    max_genome_bytes: 4_096;       // [P1] for `free`
    max_reaction_space: 65_536;    // reaction IDs fit in u16
    inventory_cap: 100u;           // per substance, per entity
}
```

- Intents per entity are bounded by the number of action slots.
- A division that would exceed `max_entities`, or whose child cannot be allocated, does not happen. The parent's inventory and genome stay unchanged, and the failure is counted in the stats.

---

## 6. Space and interaction

### 6.1 Spaces

| Space | Positions | Neighbour query | Phase |
| :-- | :-- | :-- | :-- |
| `Continuous2D(w, h, wrap)` | `vec2` positions plus a heading | Spatial hash; radius at most the world's `max_interaction_radius` | P0 |
| `Grid2D(w, h, occupancy)` | `ivec2` cells (`Exclusive` or `Shared`) | Moore or von Neumann neighbourhood | P1 |
| `Graph(nodes, edges)` | Entities on graph nodes | Adjacent nodes | P2 |
| `Continuous3D` | `vec3` positions | Spatial hash | Later |

Fields are grids laid over any space. An entity's cell is `cell_at(pos)`, and signals can be sampled bilinearly.

### 6.2 Interaction mechanisms (each world enables what it wants)

| Mechanism | How it works | Locality | Phase |
| :-- | :-- | :-- | :-- |
| **Stigmergy** | `emit` and `drop` into fields; other entities sense them later | Local | P0 |
| **Contact** | `give`, `steal` and neighbour senses within the interaction radius | Local | P0 |
| **Channels** | `send` and `sense channel`. Channels are declared as `local(radius)`, `global`, or `addressed`, in which case an entity can reply to the ID of the last sender it heard | Local or non-local | P1 |
| **Exchanges** | A world-declared market where offers are matched and substances swapped atomically | Non-local | P2 |
| **Bonds** | Entities link into composites that share inventory flow and move together; a composite can divide as a unit | Local | P2 |

Messages are signals: they are free to copy, but sending them costs energy. Substances only move through `take`/`drop`/`give`/`steal`, through exchanges, through world-rule transfers and diffusion, and through reactions. That keeps conservation intact even across non-local interactions.

---

## 7. Tick pipeline

```
┌ 1 Sense ─┬ 2 Think ─┬ 3 Arbitrate ──┬ 4 React ──┬ 5 World ─┬ 6 Life ──┐
│ snapshot │ brains   │ upkeep, fees, │ entity    │ world    │ dispose, │
│ reads    │ → intents│ transfers,    │ reactions,│ rules,   │ divide,  │
│          │          │ movement      │ vent waste│ sources  │ mutate   │
└──────────┴──────────┴───────────────┴───────────┴──────────┴──────────┘
           hot-swaps and director actions are applied only between ticks
```

All ordering below is part of the contract. A CPU reference executor (§16) implements it literally, and GPU kernels must match its ledger and, in deterministic mode, its state hash.

### 7.1 Sense and think

Each entity evaluates its sense genes against a read-only snapshot of tick `t`, runs its brain, and turns firing action genes into intents, in action-slot order. This step is read-only, so it is free of races. Register writes are committed at its end (§5.5).

### 7.2 Arbitrate

The steps run in this order:

1. **Upkeep.** Each entity's upkeep is computed (§3.3). If its `pay` balance covers it, the upkeep reaction runs and the product is vented. If not, the entity is marked **dying**: it pays nothing, its intents are cancelled, it runs no reactions, it cannot divide, and it is excluded from every transfer this tick, as source or destination. Its inventory is disposed of in Life.
2. **Action fees.** Intents are charged in action-slot order. An intent whose fee the entity can't afford at that point is dropped and pays nothing. A fee is an **attempt fee**: it is paid in full even if the action later moves less than requested.
3. **Transfers.** All substance-moving intents (`take`, `drop`, `give`, `steal`) are resolved together in two stages, using the balances at the start of this step. Incoming amounts therefore cannot fund outgoing amounts in the same step.
   - **Stage 1, per origin and substance:** if requests exceed the origin's balance, each request is granted pro-rata.
   - **Stage 2, per destination and substance:** if the stage-1 grants exceed the destination's headroom (cap minus balance at the start of the step), each grant is reduced pro-rata. Outflows during this step do not create headroom.
   - **Commit:** each transfer moves its stage-2 amount. Anything not granted stays at its origin.
   - **Remainders:** each stage floors the pro-rata shares and then hands out the leftover µ one at a time, largest fractional part first, with ties broken by Philox (`pass = arbitrate`).

   This may leave some capacity unused when stage 2 reduces a grant that stage 1 limited. That is accepted for P0.
4. **Movement.** Each entity moves by its intent plus a soft repulsion force, computed as a gather from the snapshot so no atomics are needed. Dying entities do not move.

### 7.3 React

**Entity reactions.** Each living entity runs its expressed reaction genes **in slot order**, one after another. For each gene:

```
extent = min( rate,
              min over inputs  i: floor(balance_i / coeff_i),
              min over outputs o: floor(headroom_o / coeff_o) )
```

The reaction then consumes and produces `extent × coeff` µ of each substance atomically. Two genes that want the same input are served in slot order, and a gene can use products made by an earlier gene in the same tick (**same-tick chaining is allowed**). Running genes in a fixed order within one entity needs no atomics.

**Venting.** After the reactions, every substance the world marks as `waste` (declared on the sink: `sink void : any; waste heat -> void;`) is moved from each inventory to its sink.

### 7.4 World

World rules run **in declaration order**. Within a rule, all cells read the state left by the previous rule.

- **Transfers** in world rules (`transfer(...)`) use the same two-stage arbitration as §7.2. A transfer that names an `overflow` sink sends any rejected amount there instead of leaving it at its origin. The overflow sink must accept that substance.
- **World reactions** use the extent rule in §4.2, limited by output headroom as in §7.3.
- **Diffusion** (`diffuse_conserved`) is a set of transfers: each cell offers `floor(amount × rate / 10_000)` of each listed substance, split equally between its four von Neumann neighbours (the remainder stays). These flows go through two-stage arbitration, so a full cell rejects inflow and the rejected amount stays at its origin.
- **Source inflows** are world-rule transfers from a source.

### 7.5 Life

1. **Disposal.** An entity dies if it is dying (§7.2) or has reached `max_age`. Each substance in its inventory goes to its field cell, as far as the cell holds that substance and has headroom. Everything else goes to the world's `remains` sink. The compiler **MUST** check that `remains` accepts every substance that cannot always reach a field cell.
2. **Division.** An entity divides if its `divide` intent survived Arbitrate, the action's `requires` condition holds now, and `max_entities` is not reached. The child gets `floor(split × amount)` of every substance (the remainder stays with the parent), a new stable entity ID, a mutated copy of the genome (§8), and a position next to the parent. Children are created in parent-ID order, so IDs are deterministic.
3. **Compaction and binning** of the entity buffer (not of genomes) prepare the next tick.

---

## 8. Evolution

### 8.1 Variation

Variation is applied to the child's genome at division, in this order:

1. **Point mutations** of gene parameters, gene choices, reaction IDs, brain nodes and expression bits. The rate is per gene and set per lineage. Rates can optionally evolve too (`mutation_rate: evolvable`).
2. **Structural mutations** (`free` profile only): duplicate, delete, insert or transpose a gene, and insert or delete a brain node, within `max_genome_bytes` and the CGP bound.
3. **Rewrite rulesets:** user-authored pattern rules on genomes (§9.2).
4. **Recombination** [P1]: a mate's genome is crossed over, uniformly or by aligned gene types, when the world enables `divide(mate: true)`.
5. **Horizontal transfer** [P2]: an optional `share_gene` action copies one gene to a neighbour, at a cost.

All randomness comes from Philox, keyed by `(seed, tick, entity_id, pass)`, with fixed draw indices per pass as in v0.3.

### 8.2 Selection

Selection is implicit: an entity that can't pay upkeep dies, and one that gathers enough resources divides. Optional `fitness` blocks [P1] add goal-directed pressure. They **only** gate or weight `divide`; they never kill entities or move substances.

### 8.3 Measuring open-endedness

These metrics are built into the runtime and streamed as `observe` columns:

- **Species:** clusters of similar genomes (hash buckets, refined by gene-level edit distance on a sample), plus counts of births and extinctions.
- **Evolutionary activity** (Bedau–Packard): how long new genes persist, and whether new adaptive genes keep appearing. Activity is reported next to a **neutral control** (the same world with selection replaced by random survival), because persistence alone does not show adaptation.
- **Complexity:** expressed genes, active brain nodes, distinct reactions in use, and **chain depth**.
- **Chain depth.** Build the graph of substances and reactions with positive entity flux over a window. Collapse its cycles (strongly connected components) into single nodes. Chain depth is the longest path, counted in reactions, in that acyclic graph that starts at a substance taken from the world. The number and size of collapsed cycles are reported separately.
- **Reaction flux:** net µ per tick produced by entity genes for each reaction, and for each substance. "Body synthesis flux" is the net `body` produced by entity reactions.
- **Behavioural novelty:** distance between entities' behaviour descriptors (movement statistics, action mix, chemistry use) and an archive of past descriptors.
- **Ecology:** trophic structure (who takes from whom), and flows between species.

The observer API (§12) summarizes these metrics so an AI can report "something new is happening" or "this world is stuck".

---

## 9. Ruliology

### 9.1 The world is made of rules

World rules can be written as local, cell-wise rules over field neighbourhoods, in the style of cellular automata:

```
rule sand on Dish.cells {
    // falling-sand style: move 1u of `grain` down if the cell below has room
    when grain > 0u and below.grain < cap(grain) {
        transfer(grain, from: self, to: below, amount: 1u);
    }
}

rule life on Board.cells (signal alive: f32) {
    let n = sum(moore.alive);
    alive = select(1.0, 0.0, n == 3 or (alive > 0.5 and n == 2));
}
```

Rules that move substances are arbitrated like any other transfer (§7.4), so conservation still holds. Signal-only rules such as `life` are pure cellular automata.

### 9.2 Rules rewrite programs

`ruleset` blocks hold pattern → replacement rules applied to genomes during variation (§8.1), with weights and `where` guards (from v0.3 §5.5):

```
ruleset Tinker {
    rule wrap     (weight = 0.5) : Add($a, $b) -> Mul(Add($a, $b), Sin($a));
    rule prune    (weight = 0.2) : Mul($x, Const($c)) where abs($c) < 1e-3 -> Const(0.0);
    rule swapgene (weight = 0.3) : Gene(sense, $p) -> Gene(sense, jitter($p, 0.1));
}
```

A rewrite that would produce a genome outside the menus, the reaction space or the profile's shape is discarded, and the genome is left as it was.

In the `free` profile [P2], rulesets can themselves be genes (§5.2), so the way an entity mutates can evolve.

### 9.3 Exploring rule spaces

`explore` blocks run many variants of a world and rank the results:

```
explore ChemistryAtlas {
    base: Dish;
    vary reaction_space.max_coeff in [1, 2, 3];
    vary rules.decay.rot_rate in logspace(10bp, 500bp, 8);
    vary seed in 1..4;
    run 50_000 ticks, batch: 32;           // 32 small worlds share one GPU at a time
    measure activity, species, chain_depth, novelty;
    keep top 12 by novelty;
    atlas view Main every 5_000 ticks;     // grid of thumbnails, one per variant
}
```

- **Batched worlds:** several small worlds run in one set of GPU buffers, each with its own world index, so a sweep keeps the GPU busy.
- **Enumeration:** `vary rule in enumerate(space)` walks a whole discrete rule space, for example all 256 elementary cellular automata.
- **Classification:** each variant gets built-in measures (entropy, compressibility of its spacetime diagram, how quickly it reaches a cycle or fixed state, activity, diversity), so results can be grouped into classes.
- **Output:** a results table (CSV/Parquet), plus a visual **atlas** in the browser and as PNG files.

---

## 10. Determinism and replay (best effort)

- All randomness uses Philox-4x32-10, keyed by `(seed, tick, entity_id, pass)`. Accumulation uses integers. There are no float atomics.
- **Same seed, same device, same journal ⇒ same run** (`--deterministic`). The journal (§13.3) records everything needed to reproduce the run.
- The conservation ledger is exact everywhere and is never relaxed, in every mode.
- Bit-identical results across different GPUs are a non-goal.
- Features may opt out of determinism for speed when they say so explicitly (for example a `fast_unordered` movement mode). Doing so marks the run as non-replayable.

---

## 11. Visualization

Visualization is native to the language, as in v0.3 §11, adapted to a browser front end.

- **`view` and `chart` blocks** are part of the source and type-checked. View code is read-only by construction: it cannot transfer, emit intents, or use the simulation's random streams.
- **Rendering in the browser:** the engine streams only the buffers that open views read (positions, chosen fields, selected stats) as binary frames over a localhost WebSocket. The browser renders them with WebGPU, using WGSL compiled from the view.
- **Rendering in the engine:** the same WGSL runs offscreen for CLI and AI snapshots (PNG and video).
- **Encodings and layers:**
  - encodings like `read(energy) |> scale(log, 1..10_000) |> palette(magma)`, with automatic legends;
  - `field` and `agents` layers [P0];
  - contours, vectors, trails, events, substance-flow particles, bonds and lineage links, spacetime diagrams and custom shader functions [P1].
- **Evolution views:**
  - genome inspector with expressed genes, active brain nodes and live wire values;
  - histogram of each wire's value across a species;
  - parent/child diff;
  - phylogeny and Muller plots;
  - a chemistry graph of which reactions are in use, their `ΔG`, and the flows between species [P1].
- **Interaction:** selecting and following entities, play/pause/step, rewind (checkpoints plus re-simulation, in deterministic mode), and side-by-side comparison of runs [P1].
- **AI-readable views:** every view can also return a snapshot as a PNG plus a JSON summary of what is on screen (counts, ranges, notable regions). This is for multimodal AIs (§12).

---

## 12. AI-native design

### 12.1 Language rules for machine authoring

- **One canonical form.** `emerge fmt` produces the only accepted formatting, and every construct has exactly one spelling.
- **Explicit everything.** Arguments are named, and every quantity has a unit (§3.5). `emerge fmt --explicit` prints every default.
- **A small grammar** with a machine-readable definition. `emerge docs --llm` prints a single reference file under 30k tokens, written for language models.
- **Stable IDs.** Every rule, reaction, gene type, view and node has a stable ID, so edits are **patches against IDs** (`add`, `replace`, `remove`, `set`), not text diffs. The node editor, the AI and hot-swaps all use the same patch format.
- **Structured diagnostics:** JSON with an error code, location, explanation and machine-applicable suggested fixes. Chemistry errors include each reaction's element balance and `ΔG`.
- **Explain.** `explain genome <id>` returns a plain-language summary ("senses light ahead; moves toward it; takes carbon and light; respires; divides at body ≥ 10u") plus a simplified expression of the active brain.

### 12.2 One API for everything

The engine exposes a single protocol: JSON-RPC over WebSocket, plus binary frames for bulk data. The browser UI, the CLI and an MCP server all use it, and **anything the UI can do, the API can do**.

| Role | API surface |
| :-- | :-- |
| **Author** | `world.create`, `world.patch`, `world.validate`, `docs.query` |
| **Observer** | `stats.query`, `entities.query` (filter, sample, aggregate), `genome.explain`, `view.snapshot`, `metrics.openendedness`, `events.subscribe` |
| **Experimenter** | `explore.run`, `explore.status`, `explore.results`, `run.fork`, `run.compare` |
| **Live director** | `world.hotswap`, `source.adjust`, `entity.inject` (spawn designed genomes, endowed from a source), `schedule` (timed events), `run.pause`/`run.step`/`run.resume` |

### 12.3 Guardrails for AI control

- Every patch and hot-swap is validated (types, units, element balance, dissipation, routes, caps, limits) and compiled before it is applied, and applied only between ticks (§13.2).
- Every AI action is written to the run journal with who did it, when and why (a free-text `reason` field), so runs can be reviewed and replayed.
- Optional budgets limit how often a director may intervene (for example at most one hot-swap per 1,000 ticks), so experiments stay interpretable.

---

## 13. Hot-swapping

### 13.1 What can be swapped live

| Swappable | When it takes effect |
| :-- | :-- |
| World rules, reactions in world rules, rates | Next tick |
| Sources and sinks (rate, location) | Next tick |
| Upkeep costs | Next tick, for every entity |
| Limits | Next tick. Lowering `max_entities` below the population blocks division but kills nobody |
| Menus (adding senses or actions) | Next tick. Existing genes keep their indices; new items become reachable by mutation |
| Reaction space, including potentials | Next tick. Reactions keep stable IDs. A removed or now-uphill reaction becomes a **tombstone**: genes that point at it become unexpressed but keep its ID, so it can never silently point at a different reaction |
| Rulesets, mutation rates | At births from the next tick |
| Views and charts | Instant; no effect on the simulation |
| Adding substances or fields | New stores start empty, or are filled from a source |
| Removing substances or fields | The swap **MUST** name a destination for the contents (another store or a sink that accepts them). Reactions, genes, senses and views that use the substance are tombstoned or removed in the same swap |
| Space type or size | Not swappable. Fork the run instead |

### 13.2 How a swap works

1. **Stage:** apply the patch to a copy of the world definition.
2. **Compile and validate:** compile the whole candidate, including GPU pipelines, and check every dependency, unit, law, route, cap and limit. This runs while the world keeps running.
3. **Commit:** at the next tick boundary, pause, migrate stores, remap genes, and switch configuration and pipelines together.
4. **Resume.**

If any step before commit fails, the world keeps its old state and the swap is rejected with diagnostics. Commit itself only moves data that validation has already checked, so it cannot fail halfway. The **pause** target (commit only) is under 200 ms for a rule change. Compile time is measured and reported separately.

### 13.3 Journal

The run journal is an append-only log of:

- the engine version, compiler version, backend and device identity, and mode flags (`--deterministic`, any opt-outs);
- the seed and the full initial source (or its content hash plus the file);
- every swap, director action and `entity.inject`, as ID patches, in order, each with its tick.

A run can be reproduced (in deterministic mode, on the same device) from its journal alone.

---

## 14. Architecture

```
 Browser (TypeScript)                      Desktop engine (Rust)
 ┌───────────────────────────┐   WS     ┌──────────────────────────────────┐
 │ Node-graph editor ⇄ text  │◄───────► │ API server (JSON-RPC + binary)   │
 │ View renderer (WebGPU)    │ localhost│ Compiler: .em → IR → WGSL        │
 │ Charts, inspectors, atlas │          │ Runtime: wgpu compute, ledger,   │
 │ AI chat / journal panel   │          │   arbitration, evolution          │
 └───────────────────────────┘          │ CPU reference executor           │
                                        │ Explore scheduler (batched worlds)│
 CLI ─────────────────────────────────► │ Journal, checkpoints, telemetry  │
 MCP server (for Claude etc.) ────────► └──────────────────────────────────┘
```

- **Engine:** a Rust workspace with `emerge-lang` (parser, type checker, unit and chemistry checks, IR, patch format), `emerge-ref` (CPU reference executor), `emerge-gpu` (WGSL generation and kernels), `emerge-runtime` (scheduler, ledger, journal), `emerge-server` (API) and `emerge-cli`. **No UI dependencies.**
- **Reference executor:** a plain, single-threaded implementation of §7. It is the definition of correct behaviour for tests. GPU kernels are checked against it on small worlds.
- **UI:** a browser app served by the engine at `localhost`. The node graph and the text are two views of the same IR, and edits are sent as ID patches. Node layout is stored in a `.meta.json` sidecar file.
- **MCP server:** a thin adapter mapping MCP tools onto the API in §12.2.
- **No Wasm engine build and no Tauri.** The browser is only the front end.

---

## 15. Reference world (Phase 0)

The file below is complete: it compiles on its own and uses only P0 features. All numbers are starting values, to be calibrated with the reference executor.

```
world Dish {
    space Continuous2D(w: 512, h: 512, wrap: true, max_interaction_radius: 4.0);

    element C, N, E;
    second_law { min_drop: 1 }

    substance light   = E      potential 10;
    substance energy  = E      potential 8;
    substance heat    = E      potential 0;
    substance sugar   = C + E  potential 9;
    substance carbon  = C      potential 0;
    substance nitrate = N      potential 0;
    substance body    = C + N  potential 5;

    field cells : Grid(256, 256) holds [light, sugar, carbon, nitrate, body] (cap: 2_000u each);
    field scent : Grid(256, 256) signal f32;

    source sun   : light;
    source spark : energy;                            // only used to endow new lineages
    source rock  : carbon, nitrate, body;             // body arrives as a slow "detritus rain"
    sink   void  : any;
    waste  heat -> void;
    remains: void;                                    // energy and heat of the dead, and cell overflow

    reaction photosynthesis : carbon + light            -> sugar;            // ΔG = −1
    reaction respiration    : sugar                     -> carbon + energy;  // ΔG = −1
    reaction build          : carbon + nitrate + energy -> body + heat;      // ΔG = −3
    reaction rot            : body                      -> carbon + nitrate; // ΔG = −5

    reaction_space {
        include: [photosynthesis, respiration, build, rot];
        generate: balanced(max_inputs: 3, max_outputs: 3, max_coeff: 2);
        max_rate: 1u;
        generated_max_rate: 250mu;
        exclude: [];
    }

    upkeep { pay: energy, becomes: heat, vent: void,
             base: 20mu, per_gene: 5mu, per_brain_node: 1mu, per_carried_u: 1mu }
    max_age: 20_000;

    limits { max_entities: 200_000; max_reaction_space: 65_536; inventory_cap: 100u; }

    // world rules, run in this order
    rule sunlight on cells { transfer(light, from: sun, to: self, amount: 2u, overflow: void); }
    rule weather  on cells { transfer(carbon,  from: rock, to: self, amount: 20mu, overflow: void);
                             transfer(nitrate, from: rock, to: self, amount: 10mu, overflow: void);
                             transfer(body,    from: rock, to: self, amount: 1mu,  overflow: void); }
    rule decay    on cells { react(rot, rate: 50bp);
                             diffuse_conserved([sugar, carbon, nitrate, body], rate: 300bp); }
    rule smell    on scent { scent = decay(diffuse(scent, rate: 0.2), 0.9); }

    senses {
        sense field(f: any field, angle_deg: -180..180, dist: 0..8);
        sense gradient(f: any field);
        sense inventory(s: any substance);
        sense crowd(radius: 0..6);
        sense neighbour(s: any substance, rank: 0..3);
        sense clock(period: 1..1000);
    }

    actions {
        action move(speed: 0..2.0)                     cost energy 20mu per distance;
        action turn(angle_deg: -30..30)                cost energy 10mu;
        action take(s: any substance, max: 0u..10u)    cost energy 5mu;
        action drop(s: any substance, max: 0u..10u)    cost energy 0mu;
        action give(s: any substance, max: 0u..10u, to: neighbour)    cost energy 5mu;
        action steal(s: any substance, max: 0u..10u, from: neighbour) cost energy 30mu;
        action emit(sig: scent, max: 0..10.0)          cost energy 2mu;
        action divide(split: 0..1)                     cost energy 0mu requires body >= 10u;
    }

    genesis {
        for cell in cells { transfer(carbon,  from: rock, to: cell, amount: 50u);
                            transfer(nitrate, from: rock, to: cell, amount: 20u);
                            transfer(body,    from: rock, to: cell, amount: 1u); }
    }
}

ruleset Tinker {
    rule wrap     (weight = 0.5) : Add($a, $b) -> Mul(Add($a, $b), Sin($a));
    rule prune    (weight = 0.2) : Mul($x, Const($c)) where abs($c) < 1e-3 -> Const(0.0);
    rule swapgene (weight = 0.3) : Gene(sense, $p) -> Gene(sense, jitter($p, 0.1));
}

lineage Drifter in Dish {
    profile fast { senses: 8, reactions: 4, actions: 6, brain: cgp(nodes: 32), registers: 4 }
    mutation { rate: 0.02, param_sigma: 0.1, expression_flip: 0.01, rules: Tinker }

    seed genome {                                     // a minimal hand-written starting point
        sense gradient(f: cells.light);               // slot 0 → in[0], in[1]
        sense inventory(s: energy);                   // slot 1 → in[2]
        sense inventory(s: body);                     // slot 2 → in[4]
        reaction photosynthesis rate 1u;
        reaction respiration    rate 1u;
        action move(speed: 1.0)          on out[0];
        action take(s: light,  max: 1u)  on out[1];
        action take(s: carbon, max: 1u)  on out[2];
        action take(s: body,   max: 1u)  on out[3];
        action divide(split: 0.5)        on out[4] > 0.5;
        brain {
            out[0] = Scale(in[0], 1.0);
            out[1] = Const(1.0);
            out[2] = Const(1.0);
            out[3] = Const(1.0);
            out[4] = Gt(in[4], 10.0);
        }
    }

    genesis(count: 2_000) {
        pos = philox_uniform(0..512, 0..512);
        transfer(energy, from: Dish.spark, amount: 5u);    // endowment via a source
    }
}

view Main for Dish {
    layer ground = field(cells.sugar) color: read |> scale(log, 1..5_000) |> palette(greens);
    layer life   = agents(all)
        glyph: arrow(dir: heading)
        color: species_id |> palette(categorical)
        size:  1 + expressed_genes / 4;
}

chart Health {
    line(population by species);
    line(metrics.chain_depth);
    line(metrics.flux.body_synthesis);
    ledger(elements);
}
```

### 15.1 Why this world can evolve

- **Seeds can live.** Each tick a seed takes light and carbon, and in the same tick (React follows Arbitrate) runs photosynthesis and then respiration, which gives up to `1u` of energy. Its costs are about `20mu` base, `5mu × 10` expressed genes, a few active brain nodes, `20mu` for moving, `15mu` in take fees, and `1mu` per unit carried: under `150mu` per tick at first, and under `250mu` once carbon and energy build up to the `100u` cap. The `5u` endowment covers more than 30 ticks without income.
- **Seeds can reproduce before discovering `build`.** `body` arrives from the detritus rain (`1mu` per cell per tick, plus `1u` per cell at genesis) and from dead entities. Rot removes it at `50bp` per tick, so a cell settles near `200mu`. A seed must therefore travel and collect `10u` to divide. This limits the population, and divisions are where mutation happens.
- **Discovering body synthesis is the milestone.** The two unused reaction slots hold unexpressed random reactions (§5.3). Any expressed reaction that produces `body` frees a lineage from the detritus limit: `build`, or a generated one such as `carbon + nitrate + light → body + heat` (ΔG = −5) or `sugar + nitrate → body + heat` (ΔG = −4). Using one also needs `take(nitrate)`, so the milestone takes at least two changes. The test (§16) measures **body synthesis flux**, not a particular reaction.
- **No free lunch.** The dissipation law blocks `heat → energy` (ΔG = +8), `carbon + nitrate → body` without energy (ΔG = +5), and reversed respiration `carbon + energy → sugar` (ΔG = +1).
- **Shortcuts that remain are physics.** `light → energy` (ΔG = −2) is downhill, so it is generated. As a generated reaction it is capped at `250mu` per tick, while the curated photosynthesis–respiration chain runs at up to `1u`. To forbid it, add `exclude: [light -> energy]`.

### 15.2 Ledger check

Every reaction balances elements, and `void` accepts every substance, so every overflow and disposal route type-checks. `energy` and `heat` cannot be stored in `cells`, so they go to `remains: void` when an entity dies.

---

## 16. Roadmap

### Phase 0: evolving creatures, end to end (about 10–12 weeks)

1. **Contracts first:** the CPU reference executor for §7, with unit tests for contention (two origins into one destination, two genes on one input), full stores, rounding and remainders, dying entities, and the dissipation law.
2. **Language:** elements, substances with potentials, reactions (balance and dissipation checks), the reaction space (`include`, `generate: balanced`, `exclude`, kinetics), units, limits, fields, sources and sinks, waste venting, upkeep, world rules (transfer, react, diffuse, decay), menus, lineages with `profile fast`, seed genomes, rulesets, `view` (field and agent layers), and `chart` (line and ledger).
3. **Runtime (GPU):** `Continuous2D` with a spatial hash, the 6-step tick pipeline matched against the reference executor, element ledger audits, two-stage arbitration, death and division, point mutation, rewrite rulesets. Benchmark heterogeneous genomes, dense contact and view streaming early.
4. **Engine API and MCP:** author, observer and live-director basics (`world.create`/`patch`/`hotswap`, `stats.query`, `genome.explain`, `view.snapshot`).
5. **Browser UI:** live view, charts, entity inspector, a hot-swap editor (text; the node graph comes later), and a journal panel.
6. **Open-endedness metrics:** species counts, complexity, chain depth, reaction flux.

### Phase 0 exit criteria

| # | Test | Pass condition |
| :-- | :-- | :-- |
| 1 | Ledger | Zero element drift over 100k ticks, including 20 random valid hot-swaps, on GPU and on the reference executor |
| 2 | Contracts | The GPU runtime matches the reference executor's ledger on every contract test, and its full state in deterministic mode |
| 3 | Dissipation | The compiler rejects every uphill reaction in a test suite, and the generated reaction space of §15 contains no reaction with `ΔG > −1` |
| 4 | Cost of living | An entity with no income, `S` µ of `pay` substance and minimum upkeep `u` µ per tick dies no later than tick `ceil(S / u) + 1`. A bloated variant loses to a lean one in head-to-head runs |
| 5 | Evolution | In §15, at least 5 of 10 seeds reach entity body synthesis flux above zero, sustained over 10k ticks and carried by ≥ 10% of the population, within 200k ticks. A control with mutation off never does |
| 6 | Hot-swap | Changing a world rule mid-run: no restart, commit pause under 200 ms, ledger intact, view updates within 1 s. A swap that fails validation or compilation leaves the world unchanged |
| 7 | AI loop | Through MCP only, Claude creates a world from a plain description, runs it, reads stats and a snapshot, hot-swaps a rule, and explains a genome. Scripted end-to-end test |
| 8 | Scale | 100k `fast` entities plus 2 fields at ≥ 30 ticks/s on a named GPU and workload (recorded with the result); browser view at ≥ 30 fps |

**Tracked, not blocking:** in `--deterministic` mode, same journal ⇒ same state hash on the same machine. Once this passes, a regression is a bug.

### Phase 1: freedom and exploration

`profile free` with structural mutation, channels, `Grid2D`, recombination, `fitness` blocks, `explore` with batched worlds and atlases, the node-graph editor, full open-endedness metrics (activity with neutral controls, novelty), the full set of visual layers, rewind, and the experimenter API.

### Phase 2: richer societies

Bonds and composite entities, exchanges and markets, the `Graph` space, horizontal gene transfer, evolvable mutation rulesets, and an AI director with budgets.

---

## 17. Open questions

- **Potential values:** whether hand-tuned integer potentials are enough, or whether worlds need tools to derive them (for example from a list of reactions that must be downhill).
- **Kinetics beyond rate caps:** whether generated reactions need catalysts (a substance that must be present but is not consumed) to make chemistry richer than "fast curated, slow generated".
- **Gene ↔ lane wiring in `free`:** whether positional wiring (shifts on insertion) gives better evolvability than labelled wiring (genes carry a lane tag).
- **CGP encoding for `free`:** the 8-bit operands and 64-bit active mask cap the brain at 64 nodes. A versioned node format with 16-bit operands and a dynamic mask would lift this.
- **Arbitration fairness:** whether the capacity left unused by two-stage arbitration matters in dense worlds, and whether a second pass should redistribute it.
- **Reaction space size:** the §15 space must be counted once the compiler exists. `limits.max_reaction_space` stops a runaway, but a locality-preserving order matters for mutation either way.
- **Species detection cost:** gene-level edit distance over 100k genomes is too expensive every tick, so it will need hashing with periodic sampling.
- **Batched worlds vs. one big world:** how GPU memory is split between them, and whether a world can run alone while others are batched.
- **AI director autonomy:** how much an AI may change unattended overnight, and how changes are summarised for the human in the morning.
- **Node-graph shape for genomes:** whether to show genes as cards and the brain as a DAG, or everything as one graph.

---

## Appendix A: what changed since v0.4

Numbers in brackets refer to the items in `archive/spec-v0.4-review.md`.

- **Dissipation law [6]:** every substance has a potential, and every reaction (including upkeep) must lower it (§3.2). This removes free-energy shortcuts while keeping generated chemistry. Downhill shortcuts remain and are handled with kinetics (`generated_max_rate`) or `exclude` (§4.3). "Fixed laws" in §0 now lists four laws.
- **Units [10]:** every quantity has an explicit unit (`u`, `mu`, `bp`), and rounding rules are defined (§3.5). Costs are integer µ, so no fractional residuals are needed.
- **Sources and sinks [2]:** they are ledger boundaries, not stores. Sinks declare which substances they accept. Upkeep is an explicit `energy → heat` reaction plus venting. `waste` substances are vented after reactions. Every overflow and death route must accept its substance.
- **Arbitration [3]:** two stages (origin, then destination headroom) on start-of-step balances, with defined remainder handling. Caps are per substance.
- **Death [4]:** failed upkeep marks an entity dying at once, cancels its intents, reactions and division, and excludes it from transfers. Action fees are attempt fees, charged in slot order.
- **Reactions [5]:** genes run in slot order with an integer extent limited by rate, inputs and output headroom. Same-tick chaining is allowed. World rules run in declaration order; diffusion is arbitrated.
- **Limits [7]:** new `limits` block; division fails cleanly; CGP bound stated (`I + N ≤ 256`, `N ≤ 64`).
- **Hot-swaps [8]:** stage → compile and validate → atomic commit. Upkeep and limits apply next tick; mutation settings at births. Reactions are tombstoned, never retargeted. Compile time is excluded from the pause target.
- **Metrics and exit criteria [9]:** chain depth is defined on the condensed reaction graph; activity is reported against a neutral control; the starvation test is bounded by stock and upkeep; the evolution test measures body synthesis flux; replay is tracked but not blocking; new contract and dissipation tests.
- **Lanes and dormancy [10]:** fixed lane layout in `fast`; unexpressed genes produce zeros or nothing; unused slots hold random unexpressed genes; register commit timing is defined. `per_brain_node` counts active nodes only.
- **Journal [10]:** records engine, compiler, device and mode identity plus ordered ID patches.
- **Reference world [1, 2, 10]:** self-contained and P0-only (no channels, `Tinker` declared, menus listed). Seeds can take light, carbon and body, so they live and reproduce before discovering body synthesis. Detritus rain supplies scarce `body`. Units and routes fixed.
- **Architecture:** new `emerge-ref` CPU reference executor; Phase 0 starts with it.
