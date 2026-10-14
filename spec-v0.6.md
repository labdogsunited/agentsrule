# Emergent: a Rule-Based Language for Open-Ended Evolution

**Architecture & Design Specification — v0.6**

Status: draft. Supersedes v0.5. This version applies the v0.5 review (`archive/spec-v0.5-review.md`) and folds in the syntax of `language.md`. The main changes are a conservation route for action fees (§3.3), exact overflow rules (§7.2), one kind of effect per world rule (§7.4), canonical reactions (§4.4), persistent IDs and revisions (§12.1, §13), a numeric contract (§10), and a reference world with a baseline and a positive control (§15). Appendix A lists every change.

Companion documents:
- **`language.md`** defines the syntax and static rules of `.em` files. It is normative for syntax; this document is normative for runtime behaviour.
- **`examples/dish.em`** is the reference world (§15) and the parser fixture.

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
7. **Explicit contracts.** Every quantity has a unit, every phase has a defined order, every limit is declared, and every integer operation has defined overflow behaviour. Two correct implementations of this spec must agree on the ledger.

---

## 2. Core concepts

| Concept | Meaning |
| :-- | :-- |
| **World** | One running simulation: spaces, substances, world rules, entities |
| **Space** | Where things are: `Continuous2D`, `Grid2D`, `Graph`, and later `Continuous3D` |
| **Field** | A grid laid over a space. Each cell holds substances (conserved) or one signal (free information) |
| **Element** | An abstract, indivisible unit of conservation (for example `C`, `N`, `E`) |
| **Substance** | A named composition of elements with a **potential** (for example `sugar = C + E potential 9`). Amounts are `u32` µ |
| **Potential** | An integer "free energy" per µ of a substance. Reactions must lower the total potential (§3.2) |
| **Signal** | Non-conserved `f32` information: smells, messages, internal registers |
| **Store** | Anything inside the world that holds substances: a field cell or an entity's inventory |
| **Source / sink** | A **ledger boundary** outside the world. Sources emit substances into stores; sinks absorb them. They are not stores and hold no balance |
| **World rule** | A user-authored rule the world runs every tick. Evolution cannot change it; the user or AI can hot-swap it |
| **Entity** | An abstract agent with a stable ID, a position, a heading, an inventory, registers and a **genome** |
| **Genome** | The entity's evolvable rules: sense genes, reaction genes, action genes, a brain program, and optional bond and message genes |
| **Lineage** | A template that seeds entities. "Species" are detected from genome similarity at runtime, not declared |
| **Revision** | A number that identifies one version of a running world's definition. Every committed patch increments it (§13) |

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
- **Every debit has a destination.** Every runtime operation that removes substance from a store (transfers, reactions, upkeep, action fees, death) states where it goes. There is no plain subtraction.
- **Initial stock** comes only from sources, in `genesis`.
- **Audits.** The ledger is checked every tick in debug builds and every `K` ticks in release builds. An imbalance halts the world and reports which element drifted and in which tick. The runtime also halts before any cumulative total would exceed `2^63`.
- **Hot-swaps** (§13) that would remove stores or substances, or lower caps below current balances, must say where the contents go. Otherwise the swap is rejected.

### 3.2 Dissipation

Element conservation alone treats `light`, `energy` and `heat` as interchangeable, because all three are one `E`. That allows perpetual motion, such as `heat → energy`. The dissipation law prevents it.

- **Every substance declares an integer potential**, per µ: `substance sugar = C + E potential 9;`. A substance without a potential is a compile error.
- **Reaction potential change.** For a reaction in canonical form (§4.4), `ΔG = Σ_out coeff·potential − Σ_in coeff·potential`.
- **Every reaction MUST be downhill:** `ΔG ≤ −min_drop`. `min_drop` is declared once per world (`second_law { min_drop: 1; }`, default `1`) and is at least `1`. This applies to hand-written reactions, generated reactions, world reactions, and the payment conversion (§3.3). The compiler rejects an uphill reaction with a diagnostic that shows its `ΔG`.
- **An uphill step must be coupled to a downhill one in the same reaction.** For example, `carbon + nitrate → body` is uphill and is rejected, while `carbon + nitrate + energy → body + heat` is downhill and is allowed. This is how a world says "building costs energy".
- **Transfers, diffusion and movement are not reactions** and do not change potential.

What dissipation does and does not do:

- It **removes free shortcuts**: any chain of reactions that would return to its start with net potential gained.
- It **does not remove shortcuts that are downhill.** If a chain `light → sugar → energy` is allowed, then the direct reaction `light → energy` is also downhill, because potential is path-independent. Such shortcuts are legitimate physics in the world. A world that wants to discourage them uses kinetics (`generated_max_rate`, §4.3) or excludes them explicitly (`exclude`, §4.3).

### 3.3 Cost of living

Every entity pays **upkeep** every tick, and pays **fees** for its actions. Both go through one **payment route**, declared in the `upkeep` block:

```
upkeep {
    pay: energy;
    becomes: heat;
    vent: void;
    base: 20mu;
    per_gene: 5mu;
    per_brain_node: 1mu;
    per_carried_u: 1mu;
}
```

- **The payment route** is the reaction `pay → becomes`, followed by moving the product to the `vent` sink. The two substances **MUST** have the same element composition, the reaction **MUST** be downhill (§3.2), and `vent` must accept `becomes`. A payment of `x` µ debits `x` µ of `pay` from the entity, and the ledger records `x` µ of `becomes` absorbed by `vent`.
- **Action fees use the same route** [P0]. Menus write a fee as an amount only (`cost 10mu`), and it is paid in the `pay` substance. Separately declared payment routes for other substances are a later extension.
- **What upkeep counts.** `per_gene` applies to expressed genes only. `per_brain_node` applies to active brain nodes only (§5.4). `per_carried_u` applies to each whole `1u` carried, over all substances, rounded down.
- **`base` MUST be greater than `0mu`**, so no entity lives for free.
- **Rounding.** Totals are computed in `u64`. Costs that depend on an `f32` are rounded up to whole µ (§3.5).
- **Order.** Upkeep is charged first in Arbitrate (§7.2), before any fee, so a genome cannot skip it.
- **Failure.** An entity that cannot pay its upkeep in full is marked **dying** at once (§7.2) and does nothing else that tick.
- **Changes** to upkeep by hot-swap apply to every entity from the next tick (§13).

Because cost grows with expressed genes and active nodes, bloat is selected against, while dormant genes and introns stay free and keep neutral drift affordable.

### 3.4 World rules are immutable to evolution

Evolution only changes genomes. World rules, the menus of senses and actions, substances, potentials, elements, limits and upkeep all live outside genomes. Mutation operators have no way to reach them. Only the user or an AI can change them, through a hot-swap (§13), and every such change is recorded in the run journal.

### 3.5 Units and arithmetic

Every number in an `.em` file has an explicit unit where one applies (`language.md` §2).

| Quantity | Spelling | Stored as |
| :-- | :-- | :-- |
| Substance amount | `5u` (units) or `250mu` (µ); `1u = 1_000mu`. `0.5u` is allowed only if it is a whole number of µ | `u32` µ |
| Per-tick fraction for conserved quantities (decay, diffusion, world reactions) | `50bp` (basis points; `10_000bp = 100%`) | `u32` bp |
| Reaction rate of a gene | an amount: µ of reaction extent per tick | `u32` µ |
| Potential | integer after `potential` | `i32` |
| Signal values, brain values, distances, speeds, angles | `f32` literal with a `.` (`1.0`), with the unit in the parameter name (`angle_deg`, `speed`) | `f32` |

- **Amounts from fractions round down.** `amount × bp / 10_000` uses `u64` and floors. The remainder stays where it was.
- **Costs never round in the entity's favour.** Costs that depend on an `f32` (for example `move` per distance) are rounded up to whole µ.
- **Senses report amounts in units** as `f32` (`inventory(body)` returns `12.5` for `12_500 µ`).
- **Integer and float edge cases** (overflow, division by zero, NaN) follow the numeric contract in §10.1.

---

## 4. Chemistry: substances and reactions

### 4.1 Declaring a chemistry

```
element C;
element N;
element E;

second_law { min_drop: 1; }

substance light   = E      potential 10;
substance energy  = E      potential 8;
substance heat    = E      potential 0;
substance sugar   = C + E  potential 9;
substance carbon  = C      potential 0;
substance nitrate = N      potential 0;
substance body    = C + N  potential 5;
```

A substance's composition counts elements per µ: `1 µ` of `sugar` holds `1 µ` of `C` and `1 µ` of `E`. The order of substance declarations is their **stable substance order**, used for canonical reactions (§4.4).

### 4.2 Reactions

```
reaction photosynthesis : carbon + light            -> sugar;            // ΔG = −1
reaction respiration    : sugar                     -> carbon + energy;  // ΔG = −1
reaction build          : carbon + nitrate + energy -> body + heat;      // ΔG = −3
reaction rot            : body                      -> carbon + nitrate; // ΔG = −5
```

- The compiler **MUST** reject a reaction whose element totals don't balance, whose `ΔG > −min_drop`, or that is not in canonical form (§4.4). A non-canonical hand-written reaction gets a diagnostic with the canonical form as a suggested fix.
- Every reaction has a **persistent ID** (§12.1). Generated reactions get IDs derived from their canonical formula, so the same reaction has the same ID across compilations.
- A reaction runs in a **place**: a field cell (world rule) or an entity's inventory (gene).
- **Extent.** A reaction runs in whole steps of **extent**. One step of extent consumes `coeff` µ of each input and produces `coeff` µ of each output.
- **World reactions** are world rules with a declared rate in `bp`, for example `react(rot, rate: 50bp);`. The extent is `floor(amount of the limiting input × rate / 10_000 / coeff)`, then limited by output headroom (§7.3).
- **Entity reactions** are genes. Each reaction gene names a reaction and has an evolvable rate in µ of extent per tick, capped by the reaction's `max_rate`. The scheduling rules are in §7.3.

### 4.3 Evolvable chemistry

Each world declares a **reaction space**, the set of reactions that entity genes may point to:

```
reaction_space {
    include: [photosynthesis, respiration, build];
    generate: balanced(max_inputs: 3, max_outputs: 3, max_coeff: 2);
    max_rate: 1u;
    generated_max_rate: 250mu;
    exclude: [light -> energy];
}
```

- `generate: balanced(...)` enumerates every canonical reaction over the declared substances that balances its elements **and** passes the dissipation law. `generate: none;` restricts entities to the `include` list.
- **Kinetics.** `generated_max_rate` lets a world make its hand-written reactions faster than generated ones, like enzymes. Evolution can still use any generated reaction, but the curated chemistry has an advantage. Omitted, it equals `max_rate`.
- **Exclusions.** `exclude` lists reactions, by name or by formula. Formulas are canonicalized before matching, so `2 light -> 2 energy` excludes the same reaction as `light -> energy`.
- **Included reactions win.** If generation produces a reaction whose canonical form equals an included reaction, the included one keeps its name, ID and `max_rate`, and the generated copy is dropped.
- **Size.** The compiler counts the space combinatorially before enumerating it, reports the count, and rejects a space larger than `limits.max_reaction_space` (§5.6).
- The compiler orders the space so that neighbouring indices are similar reactions. A mutation of a reaction gene moves its index to a nearby entry or a random one. Evolution can discover reactions the user never wrote, while every reaction stays balanced and downhill by construction.

### 4.4 Canonical reactions

Every reaction, hand-written or generated, is put into **canonical form** before it is hashed, excluded, checked for `ΔG`, or given a rate:

1. **Aggregate:** repeated terms on one side are merged (`sugar + sugar` → `2 sugar`).
2. **Cancel:** a substance on both sides is removed from both by its smaller coefficient (`carbon + light → carbon + energy` → `light → energy`).
3. **Reduce:** all coefficients are divided by their greatest common divisor (`2 light → 2 energy` → `light → energy`).
4. **Sort:** terms on each side follow stable substance order (§4.1).

A reaction that cancels to nothing on either side is rejected. Because cancellation removes substances that appear on both sides, P0 has **no implicit catalysts**: a substance that is needed but not consumed cannot be expressed. Explicit catalysts are an open question (§17).

---

## 5. Entities and genomes

### 5.1 Menus: the world declares, the genome chooses

The world declares menus of possible **senses** and **actions**. Genes select items from the menus and set their parameters.

```
senses {
    sense probe(q: any quantity, angle_deg: -180.0..180.0, dist: 0.0..8.0);
    sense gradient(q: any quantity);
    sense inventory(s: any substance);
    sense crowd(radius: 0.0..6.0);
    sense neighbour(s: any substance, rank: 0..3);
    sense channel(ch: any channel);                               // [P1]
    sense clock(period: 1..1_000);
}

actions {
    action move(speed: 0.0..2.0)                    cost 20mu per distance;
    action turn(angle_deg: 0.0..30.0)               cost 10mu;
    action take(s: any substance, max: 0u..10u)     cost 5mu;
    action drop(s: any substance, max: 0u..10u)     cost 0mu;
    action give(s: any substance, max: 0u..10u, to: neighbour)    cost 5mu;
    action steal(s: any substance, max: 0u..10u, from: neighbour) cost 30mu;
    action emit(sig: any signal, max: 0.0..10.0)    cost 2mu;
    action send(ch: any channel, max: 0.0..1.0)    cost 10mu;   // [P1]
    action divide(split: 0.0..1.0)                  cost 0mu requires body >= 10u;
    action bond(to: neighbour)                      cost 50mu;   // [P2]
}
```

**Senses.** Two-lane senses report in the entity's own frame: `gradient` returns `.x` along the heading and `.y` to the left, in units of the quantity per unit distance. `probe` returns the quantity's value at a point relative to the heading. One-lane senses fill `.y` with `0.0`.

**How a brain output becomes an action.** An action gene fires when its output lane is above its threshold (default `0.0`). The output `o` then sets the action's size:

| Action | Size when it fires |
| :-- | :-- |
| `move` | distance `clamp(o, 0, 1) × speed` along the heading |
| `turn` | angle `clamp(o, −1, 1) × angle_deg`; positive turns left |
| `take`, `drop`, `give`, `steal` | amount `floor(clamp(o, 0, 1) × max)` µ |
| `emit` | signal `clamp(o, 0, 1) × max` |
| `send` [P1] | value `clamp(o, −1, 1) × max` |
| `divide`, `bond` | no size; the gene's parameters apply |

**Several genes for the same action.**
- For `move`, `turn` and `divide`, only the **first firing gene in slot order** counts. Later ones are dropped before fees and pay nothing. A parent divides at most once per tick.
- Substance actions (`take`, `drop`, `give`, `steal`) and `emit` all count. Each is a separate request with its own fee.

**Fees.** Fees are attempt fees (§7.2). `per distance` fees are charged on the **requested** distance, before movement is resolved.

**Requirements.** `requires` is a condition on the entity's own inventory (`language.md` §5). It is checked when the action takes effect: for `divide`, in Life (§7.5).

An experiment that doesn't want an action (for example `steal`) simply leaves it out. A world **MUST NOT** list a menu item from a later phase than the one the engine implements.

### 5.2 Gene types

| Gene | Selects | Produces |
| :-- | :-- | :-- |
| **Sense gene** | A sense from the menu, plus its parameters | Two brain input lanes (`.y` is `0.0` for one-lane senses) |
| **Reaction gene** | A reaction ID from the reaction space, plus a rate | Chemistry inside the entity's inventory |
| **Action gene** | An action from the menu, a threshold, and fixed parameters | When it fires, an intent sized from its output lane (§5.1) |
| **Brain** | A CGP program (§5.4) | Maps input lanes to output lanes |
| **Mutation gene** [P2] | A rewrite rule and its weight | Lets the mutation operators themselves evolve ("evolution of evolvability") |

Each gene has an `expressed` bit. Unexpressed genes are inherited and mutated but cost no upkeep, produce nothing, and a single mutation can switch them back on. An unexpressed sense gene writes `0.0` to its lanes; an unexpressed action gene emits no intent; an unexpressed reaction gene does not run.

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

**Lane layout in `fast`.** With `S` sense slots, `A` action slots and `R` registers, the brain has `I = 2S + R` input lanes and `A + R` output lanes:

- inputs `2i` and `2i + 1` are sense slot `i` (`sense[i].x`, `sense[i].y` in source);
- inputs `2S + r` are register `r` as read (`reg[r]`);
- outputs `j` drive action slot `j` (`act[j]`);
- outputs `A + r` are register `r` as written.

**Unused slots.** When a seed genome fills fewer slots than the profile has, the remaining slots are filled with **unexpressed** genes drawn uniformly from the menus and the reaction space, using the lineage's Philox stream. A single expression flip can then switch one on.

### 5.4 Brain: positional CGP

The brain is the CGP design from v0.3 §5:

- Inputs and outputs are scalar lanes.
- Each node is 8 bytes (`opcode: u8`, `src_a/b/c: u8`, `param: f32`), using 16 protected, branch-free opcodes. The source-level signatures of the opcodes are listed in `language.md` §8.4.
- Output genes, an active bitmask, and no compaction (so homology between genomes is preserved).
- **Bound:** with `I` input lanes and `N` nodes, `I + N ≤ 256`, and `N ≤ 64` (the width of the active mask). The compiler rejects a profile that exceeds either.
- A node is **active** if it is reachable from an output lane read by an expressed action gene, or from a register output.
- Every value is sanitized after every node, as in v0.3 §5.3 and §10.1.

In the `free` profile, node insertion and deletion keep the graph feed-forward by renumbering operand references.

### 5.5 Registers and memory

Each entity has `R` persistent `f32` registers (default 4).

- **Read:** the input lane `reg[r]` holds the value committed at the end of the previous tick (`0.0` at birth).
- **Write:** the output lane for register `r` is committed at the end of Think and becomes next tick's read value.
- **Hold:** a register the brain does not write keeps its value. In CGP terms its output gene points at its own input lane.

This gives entities memory and internal state without loops.

### 5.6 Limits

Every world declares its limits. The compiler checks static limits and the runtime enforces dynamic ones.

```
limits {
    max_entities: 200_000;
    max_genome_bytes: 4_096;       // [P1] for `free`
    max_reaction_space: 65_536;    // reaction indices fit in u16
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

Messages are signals: they are free to copy, but sending them costs a fee. Substances only move through `take`/`drop`/`give`/`steal`, through exchanges, through world-rule transfers and diffusion, through reactions, and through the payment route. That keeps conservation intact even across non-local interactions.

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

All ordering below is part of the contract. The CPU reference executor (§14) implements it literally, and GPU kernels must match it as described in §10.2.

### 7.1 Sense and think

Each entity evaluates its sense genes against a read-only snapshot of tick `t`, runs its brain, and turns firing action genes into intents, in action-slot order, applying the rules for duplicate actions (§5.1). This step is read-only, so it is free of races. Register writes are committed at its end (§5.5).

### 7.2 Arbitrate

The steps run in this order:

1. **Upkeep.** Each entity's upkeep is computed (§3.3). If its `pay` balance covers it, it is paid through the payment route. If not, the entity is marked **dying**: it pays nothing, its intents are cancelled, it runs no reactions, it cannot divide, and it is excluded from every transfer this tick, as origin or destination. Its inventory is disposed of in Life.
2. **Action fees.** Intents are charged in action-slot order, through the payment route. An intent whose fee the entity can't afford at that point is dropped and pays nothing. A fee is an **attempt fee**: it is paid in full even if the action later moves less than requested.
3. **Transfers.** All substance-moving intents (`take`, `drop`, `give`, `steal`) are resolved together in two stages, using the balances at the start of this step. Incoming amounts therefore cannot fund outgoing amounts in the same step.
   - **Stage 1, per origin and substance:** if requests exceed the origin's balance, each request is granted pro-rata. A source origin has no balance, so stage 1 grants every request from a source in full.
   - **Stage 2, per destination and substance:** if the stage-1 grants exceed the destination's headroom (cap minus balance at the start of the step), each grant is reduced pro-rata. Outflows during this step do not create headroom. A sink destination has no cap.
   - **Commit:** each transfer **delivers** its stage-2 amount. If it names an overflow sink, it also **overflows** `stage-1 grant − stage-2 grant` to that sink. The origin is debited `delivered + overflowed`; a source origin records the same sum as emitted. Anything not delivered or overflowed stays at its origin, and an amount the origin could not supply in stage 1 is simply never moved.
   - **Remainders:** each stage floors the pro-rata shares and then hands out the leftover µ one at a time, largest fractional part first, with ties broken by Philox (`pass = arbitrate`).

   Some capacity may be left unused when stage 2 reduces a grant that stage 1 limited. That is accepted for P0.
4. **Movement.** Each entity turns and then moves by its intents plus a soft repulsion force, computed as a gather from the snapshot so no atomics are needed. Dying entities do not move.

### 7.3 React

**Entity reactions.** Each living entity runs its expressed reaction genes **in slot order**, one after another. For each gene:

```
extent = min( rate,
              min over inputs  i: floor(balance_i / coeff_i),
              min over outputs o: floor(headroom_o / coeff_o) )
```

The reaction then consumes and produces `extent × coeff` µ of each substance atomically. Because reactions are canonical (§4.4), no substance is both an input and an output. Two genes that want the same input are served in slot order, and a gene can use products made by an earlier gene in the same tick (**same-tick chaining is allowed**). Running genes in a fixed order within one entity needs no atomics.

**Venting.** After the reactions, every substance the world marks as `waste` (`waste heat -> void;`) is moved from each inventory to its sink.

### 7.4 World

World rules run **in declaration order**. Each rule reads the state left by the previous rule and commits all its effects before the next rule starts.

Each rule holds **one kind of effect**. The compiler rejects a rule that mixes kinds (`language.md` §7):

| Kind | Statements | Execution within the rule |
| :-- | :-- | :-- |
| **Transfer rule** | `transfer`, `diffuse_conserved` | All transfers of all cells are arbitrated together as in §7.2 step 3, against the state at the start of the rule. Diffusion offers `floor(amount × rate / 10_000)` of each listed substance per cell, split equally between its four von Neumann neighbours, with the remainder staying put. Overflow follows §7.2 |
| **Reaction rule** | `react` | In each cell, reactions run in statement order with the extent rule of §4.2, limited by inputs and output headroom. Cells are independent |
| **Signal rule** | signal assignments | Every cell reads signal values from the start of the rule and writes new values, which are committed together |

Conditions (`when`) may appear in any kind of rule and read the state at the start of the rule.

**Source inflows** are transfer rules with a source origin.

### 7.5 Life

1. **Disposal.** An entity dies if it is dying (§7.2) or has reached `max_age`. Each substance in its inventory goes to its field cell, as far as the cell holds that substance and has headroom. Everything else goes to the world's `remains` sink. The compiler **MUST** check that `remains` accepts every substance that cannot always reach a field cell.
2. **Division.** An entity divides if its first firing `divide` intent survived Arbitrate, the action's `requires` condition holds now, and `max_entities` is not reached. The child gets `floor(split × amount)` of every substance (the remainder stays with the parent), a new stable entity ID, a mutated copy of the genome (§8), its parent's heading, and a position next to the parent. Children are created in parent-ID order, so IDs are deterministic.
3. **Compaction and binning** of the entity buffer (not of genomes) prepare the next tick.

---

## 8. Evolution

### 8.1 Variation

Variation is applied to the child's genome at division, in this order:

1. **Point mutations** of gene parameters, gene choices, reaction IDs, brain nodes and expression bits. The rate is per gene and set per lineage. Rates can optionally evolve too (`rate: evolvable;`).
2. **Structural mutations** (`free` profile only): duplicate, delete, insert or transpose a gene, and insert or delete a brain node, within `max_genome_bytes` and the CGP bound.
3. **Rewrite rulesets:** user-authored pattern rules on genomes (§9.2).
4. **Recombination** [P1]: a mate's genome is crossed over, uniformly or by aligned gene types, when the world enables `divide(mate: true)`.
5. **Horizontal transfer** [P2]: an optional `share_gene` action copies one gene to a neighbour, at a cost.

All randomness comes from Philox, keyed by `(seed, tick, entity_id, pass)`, with fixed draw indices per pass as in v0.3. A run may switch variation off for control experiments (`--mutation off`).

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
rule sand on cells {
    when self.grain > 0u and below.grain < cap(grain) {
        transfer(grain, from: self, to: below, amount: 1u);
    }
}

rule life on board {
    let n = sum(moore.value);
    self.value = select(when: n == 3.0 or (self.value > 0.5 and n == 2.0), then: 1.0, else: 0.0);
}
```

Rules that move substances are arbitrated like any other transfer (§7.4), so conservation still holds. Signal rules such as `life` are pure cellular automata.

### 9.2 Rules rewrite programs

`ruleset` blocks hold pattern → replacement rules applied to genomes during variation (§8.1), with weights and `where` guards (from v0.3 §5.5):

```
ruleset Tinker {
    rule wrap  weight 0.5 : Add($a, $b) -> Mul(Add($a, $b), Sin($a));
    rule prune weight 0.2 : Mul($x, Const($c)) where abs($c) < 1e-3 -> 0.0;
    rule nudge weight 0.3 : sense $kind(dist: $d) -> sense $kind(dist: jitter($d, sigma: 0.1));
}
```

A rewrite that would produce a genome outside the menus, the reaction space or the profile's shape is discarded, and the genome is left as it was.

In the `free` profile [P2], rulesets can themselves be genes (§5.2), so the way an entity mutates can evolve.

### 9.3 Exploring rule spaces

`explore` blocks run many variants of a world and rank the results:

```
explore ChemistryAtlas {
    base: Dish;
    vary Dish.reaction_space.generate.max_coeff in [1, 2, 3];
    vary Dish.rules.rotting.react[0].rate in logspace(from: 10bp, to: 500bp, steps: 8);
    vary seed in 1..4;
    ticks: 50_000;
    batch: 32;
    measure: [activity, species, chain_depth, novelty];
    keep: top(12, by: novelty);
    atlas: atlas(view: Main, every: 5_000);
}
```

- **Batched worlds:** several small worlds run in one set of GPU buffers, each with its own world index, so a sweep keeps the GPU busy.
- **Enumeration:** `vary rule in enumerate(space)` walks a whole discrete rule space, for example all 256 elementary cellular automata.
- **Classification:** each variant gets built-in measures (entropy, compressibility of its spacetime diagram, how quickly it reaches a cycle or fixed state, activity, diversity), so results can be grouped into classes.
- **Output:** a results table (CSV/Parquet), plus a visual **atlas** in the browser and as PNG files.

---

## 10. Numbers, determinism and replay

### 10.1 Numeric contract

Every operation has defined results, so no behaviour depends on the backend.

| Operation | Result |
| :-- | :-- |
| Amount `+` amount | saturates at `2^32 − 1` µ (stores are capped far below this, so it only matters inside expressions) |
| Amount `−` amount | saturates at `0` |
| Amount `×` rate, amount `/` int | computed in `u64`, floored |
| Integer `+`, `−`, `×` | `i32`, saturating |
| Division by zero (integer or amount) | `0` |
| Float result NaN or ±Inf | `0.0` |
| Float magnitude | clamped to `±1e6` after every brain node, signal assignment and sense; brain outputs clamped to `±1e4` (v0.3 §5.3) |
| `amount(s, units: x)` | `floor(x × 1_000)` µ; NaN or negative gives `0`; values above `2^31` µ give `2^31` µ |
| Ledger totals | `u64`; the runtime halts before any total exceeds `2^63` |

Denormal floats may be flushed to zero. Fused multiply-add is allowed only where the source says `MulAdd`.

### 10.2 Determinism

- All randomness uses Philox-4x32-10, keyed by `(seed, tick, entity_id, pass)`. Accumulation uses integers. There are no float atomics.
- **Same seed, same device, same journal ⇒ same run** (`--deterministic`). The journal (§13.3) records everything needed to reproduce the run.
- The conservation ledger is exact everywhere and is never relaxed, in every mode.
- Bit-identical results across different GPUs are a non-goal.
- **CPU and GPU agree exactly only on integer contracts.** Transcendental functions (`Sin`, `Tanh`, `Sqrt`) and movement differ between CPU and GPU, and a tiny float difference can change which cell an entity is in. So the reference executor is compared with GPU kernels in two ways (§16):
  - **Contract fixtures** use fixed positions and brains made only of exact operations (`Add`, `Sub`, `Mul` of small integers, `Gt`, `Select`, constants). On these, every integer in the state must match exactly.
  - **Full worlds** must conserve exactly on both, and their statistics (population, flux, births) must agree within a tolerance across seeds.
- Features may opt out of determinism for speed when they say so explicitly (for example a `fast_unordered` movement mode). Doing so marks the run as non-replayable.

---

## 11. Visualization

Visualization is native to the language, as in v0.3 §11, adapted to a browser front end.

- **`view` and `chart` blocks** are part of the source and type-checked. View code is read-only by construction: it cannot transfer, emit intents, or use the simulation's random streams.
- **Rendering in the browser:** the engine streams only the buffers that open views read (positions, chosen fields, selected stats) as binary frames over a localhost WebSocket. The browser renders them with WebGPU, using WGSL compiled from the view.
- **Rendering in the engine:** the same WGSL runs offscreen for CLI and AI snapshots (PNG and video).
- **Encodings and layers:**
  - encodings like `units(energy) |> scale(log, range: 1.0..10_000.0) |> palette(magma)`, with automatic legends;
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

- **One canonical form.** `emerge fmt` produces the only accepted formatting, and every construct has exactly one spelling (`language.md` §1).
- **Explicit everything.** Arguments are named except for the one subject argument of designated built-ins, and every quantity has a unit (§3.5). `emerge fmt --explicit` prints every default.
- **A small grammar** with a machine-readable definition (`language.md` §11). `emerge docs --llm` prints a single reference file under 30k tokens, written for language models.
- **Paths and persistent IDs.** Every declaration, gene, statement, view and node has a readable **path** (`Dish.rules.rotting`) and a **persistent ID** (a `u64`). Paths can change when items are added or renamed; IDs never change and are never reused.
  - IDs are stored in `.emerge/ids.json` next to the sources, and are kept in version control with them.
  - When sources are reloaded, items are matched to existing IDs by label (`@name`, `language.md` §10), then by named path, then by kind and position. Unmatched items get new IDs, and IDs of vanished items are retired.
  - Edits are **patches** (`add`, `set`, `replace`, `remove`, `rename`) written against paths **at a stated revision**. The engine resolves paths to IDs at that revision and records the IDs in the journal (§13).
- **Structured diagnostics:** JSON with an error code, location, explanation and machine-applicable suggested fixes (`language.md` §14). Chemistry errors include each reaction's element balance, canonical form and `ΔG`.
- **Explain.** `explain genome <id>` returns a plain-language summary ("senses light to the side; turns toward it; takes carbon and light; respires; divides at body ≥ 10u") plus a simplified, infix rendering of the active brain.

### 12.2 One API for everything

The engine exposes a single protocol: JSON-RPC over WebSocket, plus binary frames for bulk data. The browser UI, the CLI and an MCP server all use it, and **anything the UI can do, the API can do**.

| Role | API surface |
| :-- | :-- |
| **Author** | `world.create` (with the lineages to seed), `world.patch`, `world.validate`, `docs.query` |
| **Observer** | `stats.query`, `entities.query` (filter, sample, aggregate), `genome.explain`, `view.snapshot`, `metrics.openendedness`, `events.subscribe` |
| **Experimenter** | `explore.run`, `explore.status`, `explore.results`, `run.fork`, `run.compare` |
| **Live director** | `world.hotswap`, `source.adjust`, `entity.inject` (spawn designed genomes, endowed from a source), `schedule` (timed events), `run.pause`/`run.step`/`run.resume` |

Every call that changes a world takes a `base_revision` and is rejected if it is stale.

### 12.3 Guardrails for AI control

- Every patch and hot-swap is validated (types, units, element balance, dissipation, canonical form, routes, caps, limits) and compiled before it is applied, and applied only between ticks (§13.2).
- Every AI action is written to the run journal with who did it, when and why (a free-text `reason` field), so runs can be reviewed and replayed.
- Optional budgets limit how often a director may intervene (for example at most one hot-swap per 1,000 ticks), so experiments stay interpretable.

---

## 13. Hot-swapping

### 13.1 What can be swapped live

| Swappable | When it takes effect |
| :-- | :-- |
| World rules, reactions in world rules, rates | Next tick |
| Sources and sinks (rate, location) | Next tick |
| Upkeep costs, action fees | Next tick, for every entity |
| Limits | Next tick. Lowering `max_entities` below the population blocks division but kills nobody. Lowering a cap below current balances needs a `move_to` destination for the excess |
| Menus (adding senses or actions) | Next tick. Existing genes keep their indices; new items become reachable by mutation |
| Reaction space, including potentials | Next tick. Reactions keep their IDs. A removed or now-uphill reaction becomes a **tombstone**: genes that point at it become unexpressed but keep its ID, so it can never silently point at a different reaction |
| Rulesets, mutation rates | At births from the next tick |
| Views and charts | Instant; no effect on the simulation |
| Adding substances or fields | New stores start empty, or are filled from a source |
| Removing substances or fields | The swap **MUST** name a destination for the contents (another store or a sink that accepts them). Reactions, genes, senses and views that use the substance are tombstoned or removed in the same swap |
| Space type or size | Not swappable. Fork the run instead |

### 13.2 How a swap works

Each world has a **revision** number, starting at `0` and incremented by every committed swap.

1. **Stage:** the patch states its base revision. If that is not the current revision, the patch is rejected as stale, and the client must rebase it.
2. **Compile and validate:** compile the whole candidate world, including GPU pipelines, and check every dependency, unit, law, route and limit. This runs while the world keeps running, and does not depend on current balances.
3. **Preflight,** at the next tick boundary with the world paused: check the revision again; then check the patch against **current** balances: every migration destination has room or is a sink, and every lowered cap has a destination for its excess. Reserve that room. If anything fails, reject the patch; nothing has changed.
4. **Commit:** migrate stores, remap genes, switch configuration and pipelines, and increment the revision. Commit only moves data into room reserved in step 3, so it cannot fail.
5. **Resume.**

At most one patch commits per tick boundary, in the order received; others wait or become stale. The **pause** target (steps 3 and 4) is under 200 ms for a rule change. Compile time is measured and reported separately.

### 13.3 Journal

The run journal is an append-only log of:

- the engine version, compiler version, backend and device identity, and mode flags (`--deterministic`, `--mutation off`, any opt-outs);
- the seed, the lineages seeded, the full initial source (or its content hash plus the files) and `.emerge/ids.json`;
- every committed swap, director action and `entity.inject`, with its tick, base revision, and operations **resolved to persistent IDs**;
- every rejected swap, with the reason.

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

- **Engine:** a Rust workspace with `emerge-lang` (parser, formatter, type checker, unit and chemistry checks, IR, IDs, patch format), `emerge-ref` (CPU reference executor), `emerge-gpu` (WGSL generation and kernels), `emerge-runtime` (scheduler, ledger, journal, revisions), `emerge-server` (API) and `emerge-cli`. **No UI dependencies.**
- **Reference executor:** a plain, single-threaded implementation of §7 and §10.1. It is the definition of correct behaviour for tests. GPU kernels are checked against it as described in §10.2.
- **UI:** a browser app served by the engine at `localhost`. The node graph and the text are two views of the same IR, and edits are sent as patches. Node layout is stored in a `.meta.json` sidecar file, keyed by persistent ID.
- **MCP server:** a thin adapter mapping MCP tools onto the API in §12.2.
- **No Wasm engine build and no Tauri.** The browser is only the front end.

---

## 15. Reference world (Phase 0)

The reference world is **`examples/dish.em`**. It is complete, uses only P0 features, and is also the parser fixture (§16). All numbers are starting values, to be calibrated with the reference executor.

It contains one world, `Dish`, and two lineages:

| Lineage | Seed genome | Used for |
| :-- | :-- | :-- |
| `Drifter` | Photosynthesis and respiration; moves forward, steers toward light, takes light, carbon and body; divides at `body ≥ 10u` | The evolution test, and the baseline with mutation off |
| `Builder` | As `Drifter`, plus `build` and `take(nitrate)` instead of `take(body)` | The positive control: shows the world supports body synthesis at all |

A run chooses which lineages to seed (`--lineages`, or `world.create` in the API).

The core of `Drifter`'s seed genome:

```
seed genome {
    sense gradient(q: cells.light) @light_dir;
    sense inventory(s: energy) @energy;
    sense inventory(s: body) @body;

    reaction photosynthesis rate 1u;
    reaction respiration    rate 1u;

    action move(speed: 1.0) @go;
    action turn(angle_deg: 30.0) @steer;
    action take(s: light,  max: 1u) @eat_light;
    action take(s: carbon, max: 1u) @eat_carbon;
    action take(s: body,   max: 1u) @eat_body;
    action divide(split: 0.5) threshold 0.5 @split;

    brain {
        act[0] = 1.0;                            // always move forward
        act[1] = Tanh(Scale(sense[0].y, 10.0));  // turn toward light
        act[2] = 1.0;
        act[3] = 1.0;
        act[4] = 1.0;
        act[5] = Gt(sense[2], 10.0);             // divide at body ≥ 10u
    }
}
```

### 15.1 Why this world should evolve

These are design arguments, to be checked by the baseline and control runs (§16), not results.

- **Seeds can live.** Each tick a seed takes light and carbon, and in the same tick (React follows Arbitrate) runs photosynthesis and then respiration, which gives up to `1u` of energy. Its costs are about `20mu` base, `5mu × 11` expressed genes, a few active brain nodes, `20mu` for moving, `10mu` for turning, `15mu` in take fees, and `1mu` per unit carried: about `130mu` per tick at first, and under `250mu` once carbon and energy build up to the `100u` cap. The `5u` endowment covers more than 30 ticks without income.
- **Seeds travel.** `move` always fires at full speed, and `turn` steers toward light. Travel no longer depends on a light gradient pointing along the heading.
- **Seeds can reproduce before discovering body synthesis.** `body` arrives from the detritus rain (`1mu` per cell per tick, plus `1u` per cell at genesis) and from dead entities. Rot removes it at `50bp` per tick, so a cell settles near `200mu`. A seed must therefore travel and collect `10u` to divide. This limits the population, and divisions are where mutation happens.
- **Discovering body synthesis is the milestone.** The profile has two spare reaction slots and two spare action slots, filled with unexpressed random genes (§5.3). Body synthesis needs an expressed reaction that produces `body` (`build`, or a generated one such as `carbon + nitrate + light → body + heat`, ΔG = −5) and an expressed `take(nitrate)`. So it takes at least two changes. The test measures **body synthesis flux**, not a particular reaction.
- **No free lunch.** The dissipation law blocks `heat → energy` (ΔG = +8), `carbon + nitrate → body` without energy (ΔG = +5), and reversed respiration `carbon + energy → sugar` (ΔG = +1).
- **Shortcuts that remain are physics.** `light → energy` (ΔG = −2) is downhill, so it is generated, capped at `250mu` per tick, while the curated chain runs at up to `1u`. To forbid it, add `exclude: [light -> energy];`.

### 15.2 Ledger check

Every reaction balances elements and is canonical. `void` accepts every substance, so every overflow, payment, waste and disposal route type-checks. `energy` and `heat` cannot be stored in `cells`, so they go to `remains: void` when an entity dies. Action fees and upkeep both leave as `heat` through `vent: void`.

---

## 16. Roadmap

### Phase 0: evolving creatures, end to end (about 10–12 weeks)

The first milestone is narrow on purpose: **one canonical world that parses, formats, runs on the CPU, conserves elements through every fee and overflow, reproduces without mutation, and rejects a bad hot-swap without changing state.** Everything else builds on it.

1. **Parser and formatter:** the grammar in `language.md` §11, with `examples/dish.em` as the fixture.
2. **Contracts:** the CPU reference executor for §7 and §10.1, with unit tests for contention (two origins into one destination, two genes on one input, an origin without enough stock into a full destination with overflow), rounding and remainders, dying entities, fees, canonical reactions, and the dissipation law.
3. **Language:** elements, substances with potentials, canonical reactions, the reaction space (`include`, `generate: balanced`, `exclude`, kinetics), units, limits, fields, sources and sinks, waste venting, the payment route, world rules of the three kinds, menus, lineages with `profile fast`, seed genomes, labels and persistent IDs, rulesets, `view` (field and agent layers), and `chart` (line and ledger).
4. **Runtime (GPU):** `Continuous2D` with a spatial hash, the 6-step tick pipeline checked against the reference executor, element ledger audits, two-stage arbitration, death and division, point mutation, rewrite rulesets. Benchmark heterogeneous genomes, dense contact and view streaming early.
5. **Engine API and MCP:** author, observer and live-director basics (`world.create`/`patch`/`hotswap` with revisions, `stats.query`, `genome.explain`, `view.snapshot`).
6. **Browser UI:** live view, charts, entity inspector, a hot-swap editor (text; the node graph comes later), and a journal panel.
7. **Open-endedness metrics:** species counts, complexity, chain depth, reaction flux.

### Phase 0 exit criteria

| # | Test | Pass condition |
| :-- | :-- | :-- |
| 1 | Parser | `examples/dish.em` parses; parse → format → parse gives the same AST; formatting it changes nothing |
| 2 | Ledger | Zero element drift over 100k ticks, including 20 random valid hot-swaps and 20 rejected ones, on GPU and on the reference executor |
| 3 | Contracts | On every contract fixture (§10.2), the GPU runtime matches every integer in the reference executor's state exactly. On `examples/dish.em`, population, births and body flux on GPU and CPU agree within 5% over 10 seeds |
| 4 | Dissipation | The compiler rejects every uphill or non-canonical reaction in a test suite, and the generated reaction space of §15 contains no reaction with `ΔG > −1` and no duplicate canonical forms |
| 5 | Cost of living | An entity with no income, `S` µ of the `pay` substance and minimum upkeep `u` µ per tick dies no later than tick `ceil(S / u) + 1`. A bloated variant loses to a lean one in head-to-head runs |
| 6 | Baseline | `Drifter` with mutation off survives 50k ticks with births every 1,000 ticks, in at least 9 of 10 seeds |
| 7 | Positive control | `Builder` with mutation off survives 50k ticks with body synthesis flux above zero, in at least 9 of 10 seeds |
| 8 | Evolution | `Drifter` with mutation on reaches body synthesis flux above zero, sustained over 10k ticks and carried by ≥ 10% of the population, within 200k ticks, in at least 5 of 10 seeds. The baseline (test 6) never does |
| 9 | Hot-swap | Changing a world rule mid-run: no restart, pause under 200 ms, ledger intact, view updates within 1 s. A swap that fails validation, compilation or preflight, or that is stale, leaves the world and its revision unchanged |
| 10 | AI loop | Through MCP only, Claude creates a world from a plain description, runs it, reads stats and a snapshot, hot-swaps a rule, and explains a genome. Scripted end-to-end test |
| 11 | Scale | 100k `fast` entities plus 2 fields at ≥ 30 ticks/s on a named GPU and workload (recorded with the result); browser view at ≥ 30 fps |

If test 6 or 7 fails, the world is recalibrated before test 8 is attempted: a failing baseline means the world can't support the seeds, and a failing control means it can't support the target metabolism.

**Tracked, not blocking:** in `--deterministic` mode, same journal ⇒ same state hash on the same machine. Once this passes, a regression is a bug.

### Phase 1: freedom and exploration

`profile free` with structural mutation, channels, `Grid2D`, recombination, `fitness` blocks, `explore` with batched worlds and atlases, the node-graph editor, full open-endedness metrics (activity with neutral controls, novelty), the full set of visual layers, rewind, and the experimenter API.

### Phase 2: richer societies

Bonds and composite entities, exchanges and markets, the `Graph` space, horizontal gene transfer, evolvable mutation rulesets, and an AI director with budgets.

---

## 17. Open questions

- **Potential values:** whether hand-tuned integer potentials are enough, or whether worlds need tools to derive them (for example from a list of reactions that must be downhill).
- **Catalysts and kinetics:** canonical form removes implicit catalysts. Explicit catalysts (a substance that must be present but is not consumed, declared separately from the formula) would make chemistry richer than "fast curated, slow generated".
- **Other payment routes:** whether actions should be able to cost substances other than the upkeep payment, each with its own declared conversion and vent.
- **Gene ↔ lane wiring in `free`:** whether positional wiring (shifts on insertion) gives better evolvability than labelled wiring (genes carry a lane tag).
- **CGP encoding for `free`:** the 8-bit operands and 64-bit active mask cap the brain at 64 nodes. A versioned node format with 16-bit operands and a dynamic mask would lift this.
- **Arbitration fairness:** whether the capacity left unused by two-stage arbitration matters in dense worlds, and whether a second pass should redistribute it.
- **Portable transcendental functions:** software implementations of `Sin`, `Tanh` and `Sqrt` with identical results on CPU and GPU would allow exact full-world comparison, at some speed cost.
- **Patch rebasing:** whether the engine should rebase simple stale patches automatically (for example a `set` on an item no other patch touched) instead of rejecting them.
- **Species detection cost:** gene-level edit distance over 100k genomes is too expensive every tick, so it will need hashing with periodic sampling.
- **Batched worlds vs. one big world:** how GPU memory is split between them, and whether a world can run alone while others are batched.
- **AI director autonomy:** how much an AI may change unattended overnight, and how changes are summarised for the human in the morning.
- **Node-graph shape for genomes:** whether to show genes as cards and the brain as a DAG, or everything as one graph.

---

## Appendix A: what changed since v0.5

Numbers in brackets refer to the items in `archive/spec-v0.5-review.md`.

- **Syntax:** all examples now use the syntax of `language.md`, which absorbs its old list of differences. The reference world moved to `examples/dish.em`.
- **Grammar [1]:** designated built-ins take one positional subject; lists are expressions; most keywords are contextual; `explore` has its own productions. The reference world is the parser fixture (exit criterion 1).
- **Payment route [2]:** action fees go through the same `pay → becomes → vent` route as upkeep, and menus write fees as amounts only. `base` upkeep must be positive. New principle: every debit has a destination (§3.1).
- **Overflow [3]:** only `stage-1 grant − stage-2 grant` can overflow; the origin is debited for delivered plus overflowed amounts; sources and sinks have no balance or cap (§7.2).
- **World rules [4]:** each rule holds one kind of effect (transfer, reaction or signal) and commits before the next. The reference `decay` rule is split into `rotting` and `spread` (§7.4).
- **Hot-swaps [5]:** revisions, stale-patch rejection, a preflight against current balances that reserves room before commit, and `move_to` for lowered caps (§13.2).
- **Canonical reactions [6]:** aggregate, cancel, reduce, sort before hashing, excluding, checking or rating. No implicit catalysts in P0. Included reactions win over generated duplicates. The reaction space is counted before it is enumerated (§4.3, §4.4).
- **Persistent IDs [7]:** paths and IDs are separate; IDs live in `.emerge/ids.json`; labels (`@name`) give genes and statements stable names; patches state a base revision and the journal records resolved IDs (§12.1, §13.3).
- **Reference world [8]:** seeds always move and steer toward light; the profile has spare action slots; a `Builder` positive control and a mutation-off baseline are exit criteria 6 and 7; runs choose lineages (§15, §16).
- **Action semantics [9]:** a size mapping for every action; the first firing `move`, `turn` and `divide` gene counts and later ones pay nothing; `turn` is signed; `per distance` fees use the requested distance; `requires` is checked when the action takes effect (§5.1).
- **Senses [9]:** `gradient` and `probe` report in the entity's own frame; `field` sense renamed `probe`.
- **Signals [9]:** `blur` takes a field, not a value, and samples its neighbourhood (`language.md` §7).
- **Registers [9]:** reads are last tick's values; unwritten registers hold their value (§5.5).
- **Opcodes [9]:** source-level signatures are published in `language.md` §8.4, with `Select(cond, then, else)`.
- **Numeric contract [9]:** saturating integer and amount arithmetic, division by zero gives `0`, defined float sanitizing and conversions (§10.1).
- **CPU/GPU comparison [9]:** exact integer match only on contract fixtures; statistical agreement on full worlds (§10.2, exit criterion 3).
- **Lineage seeding:** runs choose which lineages to seed, and `--mutation off` is a run flag (§8.1, §12.2).
