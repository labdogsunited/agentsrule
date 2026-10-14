# Emergent: a Rule-Based Language for Open-Ended Evolution

**Architecture & Design Specification — v0.4**

Status: draft. Supersedes v0.1–v0.3 (now in `archive/`). This version was rebuilt around the product decisions in §0.

Conventions:
- **MUST** / **MUST NOT** mark normative requirements.
- **[P0]** / **[P1]** / **[P2]** mark the phase in which a feature arrives (§16).
- `emerge` is a placeholder name for the CLI and engine; source files use the `.em` extension.

---

## 0. Product decisions

| Topic | Decision |
| :-- | :-- |
| Primary user | One researcher/tinkerer (the author), working with AI assistants |
| First experiment | Evolving creatures |
| Goals, in priority order | 1. Open-ended surprise · 2. Beautiful visuals · 3. Goal-directed evolution · 4. Understanding rule spaces |
| What an entity is | An abstract thing whose every capability is defined by rules, all of which can evolve |
| Fixed laws (unbreakable) | Conservation · Cost of living · The world rules the user writes |
| Not fixed | Locality: non-local interaction is allowed when a world enables it |
| Flexibility | The language provides mechanisms and each experiment chooses policies: space, interaction types, and scale vs. freedom |
| Metabolism | Abstract, user-defined "chemistry": substances plus transformation rules, evolvable |
| Ruliology | All three meanings: the world is made of local rules; rules rewrite programs; rule spaces are explored systematically |
| Authoring | Node graphs plus AI. **AI-friendly matters more than human-friendly** |
| Live changes | Rules can be hot-swapped while a world runs |
| AI roles | Author, observer, experimenter, live director |
| Platform | Browser front end; engine runs natively on the desktop CPU/GPU |
| Exact replay | Nice to have, not a gate |
| Team and timeline | Solo plus AI, over several months |

---

## 1. Design principles

1. **Everything is a rule.** World physics, chemistry, senses, actions, decisions and mutation are all expressed as rules over local state. There is no hard-coded "creature".
2. **Three laws, enforced by the runtime.** Evolved or hot-swapped rules may do anything except:
   - create or destroy substance;
   - avoid paying upkeep;
   - change the world rules.

   These checks happen in the runtime (§3), not by convention.
3. **Mechanisms, not policies.** The world declares *what is possible*: substances, actions, senses, channels and space. Entities evolve *which of those possibilities they use and how*.
4. **Open-endedness first.** Defaults favour diversity and novelty: variable genomes, neutral drift, rich action menus. Built-in metrics report whether anything new is happening.
5. **AI-native.** The text format, API, diagnostics and views are designed so that an AI can author, run, observe and steer worlds without a human in the loop (§12).
6. **Watchable.** Every world can be visualized natively; visualization is part of the language (§11).

---

## 2. Core concepts

| Concept | Meaning |
| :-- | :-- |
| **World** | One running simulation: spaces, substances, world rules, entities |
| **Space** | Where things are: `Continuous2D`, `Grid2D`, `Graph`, and later `Continuous3D` |
| **Field** | A grid laid over a space. Each cell holds substances (conserved) or signals (free information) |
| **Element** | An abstract, indivisible unit of conservation (for example `C`, `N`, `E`) |
| **Substance** | A named composition of elements (for example `sugar = C + E`). Amounts are `u32` micro-units |
| **Signal** | Non-conserved `f32` information: smells, messages, internal registers |
| **Store** | Anything holding substances: a field cell, an entity's inventory, a source or a sink |
| **World rule** | A user-authored rule the world runs every tick. Evolution cannot change it; the user or AI can hot-swap it |
| **Entity** | An abstract agent with a position, an inventory, registers and a **genome** |
| **Genome** | The entity's evolvable rules: sense genes, reaction genes, action genes, a brain program, and optional bond and message genes |
| **Lineage** | A template that seeds entities. "Species" are detected from genome similarity at runtime, not declared |

---

## 3. The three fixed laws

### 3.1 Conservation

- **The ledger counts elements, not substances.** A reaction may turn substances into other substances only if the element totals on both sides are equal. For every element `e`, after every tick:

  ```
  Σ_stores amount_e  +  absorbed_e  −  emitted_e  ==  0
  ```

  `emitted_e` and `absorbed_e` are cumulative 64-bit totals over sources and sinks.
- **Fixed-point arithmetic.** Amounts are `u32` micro-units (`1 unit = 1_000 µ`). Totals and products use 64-bit arithmetic built from 32-bit halves, so shaders never need native 64-bit integers or float atomics.
- **Every store has a cap of at most 2^31 µ.** A transfer into a store is clamped to its free capacity, or the excess is routed to a declared overflow store. Nothing ever wraps around or is silently lost.
- **Contested transfers are split pro-rata,** with any integer remainder assigned by a seeded random tie-break (carried over from v0.3).
- **Initial stock** comes only from sources, in `genesis`.
- **Audits.** The ledger is checked every tick in debug builds and every `K` ticks in release builds. An imbalance halts the world and reports which element drifted and in which tick.
- **Hot-swaps** (§13) that would add or remove stores must say where the contents go. Otherwise the swap is rejected.

### 3.2 Cost of living

Every entity pays **upkeep** every tick. The world declares what upkeep is paid in and where it goes:

```
upkeep { pay: energy, to: heat,
         base: 50, per_gene: 5, per_brain_node: 1, per_carried_unit: 0.01 }
```

- Upkeep is charged by the runtime before any action, so a genome cannot skip it.
- An entity that can't pay in full dies (§7.4).
- Actions also have costs, declared with each action (§5.2).
- Because cost grows with genome size, bloat is selected against. Dormant genes and dormant brain nodes are cheaper (`per_gene` applies only to expressed genes), which keeps neutral drift affordable.

### 3.3 World rules are immutable to evolution

Evolution only changes genomes. World rules, the menus of senses and actions, substances, elements and upkeep all live outside genomes. Mutation operators have no way to reach them. Only the user or an AI can change them, through a hot-swap (§13), and every such change is recorded in the run journal.

---

## 4. Chemistry: substances and reactions

### 4.1 Declaring a chemistry

```
element C, N, E;                  // E is energy

substance light   = E;
substance energy  = E;
substance heat    = E;
substance sugar   = C + E;
substance carbon  = C;
substance nitrate = N;
substance body    = C + N;
```

### 4.2 Reactions

```
reaction photosynthesis : carbon + light            -> sugar;
reaction respiration    : sugar                     -> carbon + energy;
reaction build          : carbon + nitrate + energy -> body + heat;
reaction rot            : body                      -> carbon + nitrate;
```

- The compiler **MUST** reject a reaction whose element totals don't balance. All four reactions above balance.
- A reaction runs in a **place**: a field cell (world rule) or an entity's inventory (gene).
- **World reactions** are world rules with a declared rate, for example `rot at 0.5%/tick in Dish.cells`.
- **Entity reactions** are genes. Each reaction gene names a reaction and has an evolvable rate (µ per tick, capped by `max_rate`). The reaction runs only if the inputs are present in the entity's inventory, and it is limited by the scarcest input.

### 4.3 Evolvable chemistry

Each world declares a **reaction space**, the set of reactions that entity genes may point to:

```
reaction_space {
    include: [photosynthesis, respiration, build];  // hand-written reactions
    generate: balanced(max_inputs: 3, max_outputs: 3, max_coeff: 2);  // all balanced reactions over the declared substances
}
```

- The compiler enumerates the reaction space into an indexed table, and orders it so that neighbouring indices are similar reactions.
- A mutation of a reaction gene moves its index to a nearby entry or a random one. This means evolution can discover reactions the user never wrote, while every reaction stays balanced by construction.
- `generate: none` restricts entities to the hand-written list.

---

## 5. Entities and genomes

### 5.1 Menus: the world declares, the genome chooses

The world declares menus of possible **senses** and **actions**. Genes select items from the menus and set their parameters.

```
senses {
    sense field(f: any field, angle: -180..180, dist: 0..8);     // value of f at a point relative to heading
    sense gradient(f: any field);                                 // returns 2 lanes
    sense inventory(s: any substance);
    sense crowd(radius: 0..6);                                    // number of neighbours
    sense neighbour(s: any substance, rank: 0..3);                // nth-nearest neighbour's amount of s
    sense channel(ch: any channel);                               // most recent message value
    sense clock();                                                // sin(tick / period) for rhythms
}

actions {
    action move(speed: 0..2.0)            cost energy 2 per unit;
    action turn(angle: -30..30)           cost energy 0.1;
    action take(s: any substance, amount) cost energy 0.5;        // from the field cell
    action drop(s: any substance, amount) cost 0;
    action give(s: any substance, amount, to: neighbour) cost energy 0.5;
    action steal(s: any substance, amount, from: neighbour) cost energy 3;
    action emit(sig: any signal, amount)  cost energy 0.2;
    action send(ch: any channel, value)   cost energy 1;
    action divide(split: 0..1)            requires body >= 10;
    action bond(to: neighbour)            cost energy 5;           // [P2]
}
```

An experiment that doesn't want an action (for example `steal`) simply leaves it out. This is where each experiment sets its policies.

### 5.2 Gene types

| Gene | Selects | Produces |
| :-- | :-- | :-- |
| **Sense gene** | A sense from the menu, plus its parameters | One or two brain input lanes |
| **Reaction gene** | An index into the reaction space, plus a rate | Chemistry inside the entity's inventory |
| **Action gene** | An action from the menu, plus fixed parameters | A threshold on a brain output; when it fires, an intent is emitted with an amount derived from that output |
| **Brain** | A CGP program (§5.4) | Maps input lanes to output lanes |
| **Mutation gene** [P2] | A rewrite rule and its weight | Lets the mutation operators themselves evolve ("evolution of evolvability") |

Each gene has an `expressed` bit. Unexpressed genes are inherited and mutated but cost no upkeep, and a single mutation can switch them back on.

### 5.3 Execution profiles: scale or freedom

Each lineage chooses a profile. Several profiles can coexist in one world.

| | `profile fast` [P0] | `profile free` [P1] |
| :-- | :-- | :-- |
| Genome shape | Fixed number of slots per gene type, fixed brain size | Variable length; genes can be duplicated, deleted and inserted; the brain can grow |
| Mutation operators | Point mutation, expression flips, rewrite rules | All of `fast`, plus duplication, deletion, insertion and transposition |
| Execution | GPU lockstep, agents binned by genome hash (v0.3 §6) | GPU with arena-allocated genomes, binned by length and shape; CPU fallback |
| Target scale | 100k+ entities | 1k–20k entities |
| Best for | Visual spectacle, statistics, large ecologies | Open-ended surprise, new capabilities appearing over time |

`fast` and `free` share the same gene types, the same menus and the same three laws. A genome can migrate from `fast` to `free` (it is padded) but not the other way.

### 5.4 Brain: positional CGP

The brain is the CGP design from v0.3 §5, unchanged:

- Inputs and outputs are scalar lanes.
- Each node is 8 bytes (`opcode`, `src_a/b/c`, `param`), using 16 protected, branch-free opcodes.
- Output genes, an active bitmask, and no compaction (so homology between genomes is preserved).

In the `free` profile, node insertion and deletion keep the graph feed-forward by renumbering operand references. Wiring between genes and the brain is positional: sense genes fill input lanes in genome order, and action genes read output lanes in genome order. When genes are added or removed in `free`, the lane assignments shift with them, which is itself a source of variation.

### 5.5 Registers and memory

Each entity has `R` persistent `f32` registers (default 4). Some brain outputs are wired to registers and read back next tick as inputs. This gives entities memory and internal state without loops.

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

Messages are signals: they are free to copy, but sending them costs upkeep. Substances only move through `take`/`drop`/`give`/`steal`, through exchanges, and through reactions. That keeps conservation intact even across non-local interactions.

---

## 7. Tick pipeline

```
┌ 1 Sense ─┬ 2 Think ─┬ 3 Arbitrate ─┬ 4 React ─┬ 5 World ─┬ 6 Life ─┐
│ snapshot │ brains   │ upkeep, then │ entity   │ world    │ death,  │
│ reads    │ → intents│ transfers,   │ reaction │ rules,   │ divide, │
│          │          │ movement     │ genes    │ sources  │ mutate  │
└──────────┴──────────┴──────────────┴──────────┴──────────┴─────────┘
           hot-swaps and director actions are applied only between ticks
```

### 7.1 Sense and think

Each entity evaluates its sense genes against a read-only snapshot, runs its brain, and turns firing action genes into intents. This step is read-only, so it is free of races.

### 7.2 Arbitrate

The steps run in a fixed order:

1. **Upkeep** is charged.
2. **Action costs** are charged. An action the entity can't afford is dropped.
3. **Transfers** are resolved: for each source store, contending requests are split pro-rata.
4. **Movement** is resolved: each entity moves by its intent plus a soft repulsion force, computed as a gather from the snapshot so no atomics are needed.

### 7.3 React and world

- Entity reaction genes run in the entity's inventory, limited by rate and by the scarcest input.
- World rules then run: field reactions, diffusion, decay, source inflows, and local CA-style rules (§9.1).

### 7.4 Life

- **Death:** an entity dies when it can't pay upkeep or exceeds `max_age`. Its whole inventory is dropped into its field cell. If no field holds a given substance, that substance goes to the world's declared `remains` sink. The compiler **MUST** check that every substance has such a route.
- **Division:** the child gets `split` of every substance (rounded down per substance; the remainder stays with the parent), a mutated copy of the genome (§8), and a position next to the parent.
- **Compaction and binning** of the entity buffer (not of genomes) prepare the next tick.

---

## 8. Evolution

### 8.1 Variation

Variation is applied to the child's genome at division, in this order:

1. **Point mutations** of gene parameters, gene choices, reaction indices, brain nodes and expression bits. The rate is per gene and set per lineage. Rates can optionally evolve too (`mutation_rate: evolvable`).
2. **Structural mutations** (`free` profile only): duplicate, delete, insert or transpose a gene, and insert or delete a brain node.
3. **Rewrite rulesets:** user-authored pattern rules on genomes (§9.2).
4. **Recombination** [P1]: a mate's genome is crossed over, uniformly or by aligned gene types, when the world enables `divide(mate: true)`.
5. **Horizontal transfer** [P2]: an optional `share_gene` action copies one gene to a neighbour, at a cost.

All randomness comes from Philox, keyed by `(seed, tick, entity_id, pass)`.

### 8.2 Selection

Selection is implicit: an entity that can't pay upkeep dies, and one that gathers enough resources divides. Optional `fitness` blocks [P1] add goal-directed pressure. They **only** gate or weight `divide`; they never kill entities or move substances.

### 8.3 Measuring open-endedness

These metrics are built into the runtime and streamed as `observe` columns:

- **Species:** clusters of similar genomes (hash buckets, refined by gene-level edit distance), plus counts of births and extinctions.
- **Evolutionary activity** (Bedau–Packard): how long new genes persist, and whether new adaptive genes keep appearing.
- **Complexity:** expressed genes, active brain nodes, distinct reactions in use, and the longest chain of reactions in use (for example light → sugar → energy → body).
- **Behavioural novelty:** distance between entities' behaviour descriptors (movement statistics, action mix, chemistry use) and an archive of past descriptors.
- **Ecology:** trophic structure (who takes from whom), and flows between species.

The observer API (§12) summarizes these metrics so an AI can report "something new is happening" or "this world is stuck".

---

## 9. Ruliology

### 9.1 The world is made of rules

World rules can be written as local, cell-wise rules over field neighbourhoods, in the style of cellular automata:

```
rule sand on Dish.cells {
    // falling-sand style: move 1 unit of `grain` down if the cell below has room
    when grain > 0 and below.grain < cap(grain) {
        transfer(grain, from: self, to: below, amount: 1_000);
    }
}

rule life on Board.cells (signal alive: f32) {
    let n = sum(moore.alive);
    alive = select(1.0, 0.0, n == 3 or (alive > 0.5 and n == 2));
}
```

Rules that move substances are arbitrated like any other transfer, so conservation still holds. Signal-only rules such as `life` are pure cellular automata.

### 9.2 Rules rewrite programs

`ruleset` blocks hold pattern → replacement rules applied to genomes during variation (§8.1), with weights and `where` guards (from v0.3 §5.5):

```
ruleset Tinker {
    rule wrap   (weight = 0.5) : Add($a, $b) -> Mul(Add($a, $b), Sin($a));
    rule prune  (weight = 0.2) : Mul($x, Const($c)) where abs($c) < 1e-3 -> Const(0.0);
    rule swapgene (weight = 0.3) : Gene(sense, $p) -> Gene(sense, jitter($p, 0.1));
}
```

In the `free` profile [P2], rulesets can themselves be genes (§5.2), so the way an entity mutates can evolve.

### 9.3 Exploring rule spaces

`explore` blocks run many variants of a world and rank the results:

```
explore ChemistryAtlas {
    base: Dish;
    vary reaction_space.max_coeff in [1, 2, 3];
    vary rules.rot.rate in logspace(0.001, 0.05, 8);
    vary seed in 1..4;
    run 50_000 ticks, batch: 32;           // 32 small worlds share one GPU at a time
    measure activity, species, max_chain, novelty;
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
- **Same seed, same device, same journal ⇒ same run** (`--deterministic`). The journal (§13.3) records hot-swaps and director actions with their tick numbers, so replay includes them.
- The conservation ledger is exact everywhere and is never relaxed.
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
  - a chemistry graph of which reactions are in use and the flows between species [P1].
- **Interaction:** selecting and following entities, play/pause/step, rewind (checkpoints plus re-simulation, in deterministic mode), and side-by-side comparison of runs [P1].
- **AI-readable views:** every view can also return a snapshot as a PNG plus a JSON summary of what is on screen (counts, ranges, notable regions). This is for multimodal AIs (§12).

---

## 12. AI-native design

### 12.1 Language rules for machine authoring

- **One canonical form.** `emerge fmt` produces the only accepted formatting, and every construct has exactly one spelling.
- **Explicit everything.** Arguments are named, and units are part of the type or the name. `emerge fmt --explicit` prints every default.
- **A small grammar** with a machine-readable definition. `emerge docs --llm` prints a single reference file under 30k tokens, written for language models.
- **Stable IDs.** Every rule, gene type, view and node has a stable ID, so edits are **patches against IDs** (`add`, `replace`, `remove`, `set`), not text diffs. The node editor, the AI and hot-swaps all use the same patch format.
- **Structured diagnostics:** JSON with an error code, location, explanation and machine-applicable suggested fixes.
- **Explain.** `explain genome <id>` returns a plain-language summary ("senses sugar ahead; moves toward it; respires when energy < 20; divides at body ≥ 12") plus a simplified expression of the active brain.

### 12.2 One API for everything

The engine exposes a single protocol: JSON-RPC over WebSocket, plus binary frames for bulk data. The browser UI, the CLI and an MCP server all use it, and **anything the UI can do, the API can do**.

| Role | API surface |
| :-- | :-- |
| **Author** | `world.create`, `world.patch`, `world.validate`, `docs.query` |
| **Observer** | `stats.query`, `entities.query` (filter, sample, aggregate), `genome.explain`, `view.snapshot`, `metrics.openendedness`, `events.subscribe` |
| **Experimenter** | `explore.run`, `explore.status`, `explore.results`, `run.fork`, `run.compare` |
| **Live director** | `world.hotswap`, `source.adjust`, `entity.inject` (spawn designed genomes, endowed from a source), `schedule` (timed events), `run.pause`/`run.step`/`run.resume` |

### 12.3 Guardrails for AI control

- Every patch and hot-swap is validated (types, conservation routes, caps) before it is applied, and applied only between ticks.
- Every AI action is written to the run journal with who did it, when and why (a free-text `reason` field), so runs can be reviewed and replayed.
- Optional budgets limit how often a director may intervene (for example at most one hot-swap per 1,000 ticks), so experiments stay interpretable.

---

## 13. Hot-swapping

### 13.1 What can be swapped live

| Swappable | Notes |
| :-- | :-- |
| World rules, reactions in world rules, rates | Applied at the next tick boundary |
| Sources and sinks (rate, location) | Ledger-safe by construction |
| Menus (adding senses or actions) | Existing genes keep their indices; new items become reachable by mutation |
| Reaction space | Re-enumerated; existing reaction genes are remapped by ID, and genes pointing at removed reactions become unexpressed |
| Rulesets, mutation rates, upkeep costs | Take effect at the next division |
| Views and charts | Instant; no effect on the simulation |
| Adding substances or fields | New stores start empty, or are filled from a source |
| Removing substances or fields | The swap **MUST** name a destination for the contents (another store or a sink); otherwise it is rejected |
| Space type or size | Not swappable. Fork the run instead |

### 13.2 How a swap works

`validate → pause at tick boundary → migrate stores and remap genes → rebuild GPU pipelines → resume`. Pipelines that the swap doesn't affect are reused. The target pause is under 200 ms for a rule change.

### 13.3 Journal

The run journal is an append-only log of the seed, the initial source, and every swap, director action and `entity.inject`, each with its tick. A run can be reproduced (in deterministic mode) from its journal and the source alone.

---

## 14. Architecture

```
 Browser (TypeScript)                      Desktop engine (Rust)
 ┌───────────────────────────┐   WS     ┌──────────────────────────────────┐
 │ Node-graph editor ⇄ text  │◄───────► │ API server (JSON-RPC + binary)   │
 │ View renderer (WebGPU)    │ localhost│ Compiler: .em → IR → WGSL        │
 │ Charts, inspectors, atlas │          │ Runtime: wgpu compute, ledger,   │
 │ AI chat / journal panel   │          │   arbitration, evolution          │
 └───────────────────────────┘          │ Explore scheduler (batched worlds)│
                                        │ Journal, checkpoints, telemetry  │
 CLI ─────────────────────────────────► │                                  │
 MCP server (for Claude etc.) ────────► └──────────────────────────────────┘
```

- **Engine:** a Rust workspace with `emerge-lang` (parser, type checker, IR, patch format), `emerge-gpu` (WGSL generation and kernels), `emerge-runtime` (scheduler, ledger, journal), `emerge-server` (API) and `emerge-cli`. **No UI dependencies.**
- **UI:** a browser app served by the engine at `localhost`. The node graph and the text are two views of the same IR, and edits are sent as ID patches. Node layout is stored in a `.meta.json` sidecar file.
- **MCP server:** a thin adapter mapping MCP tools onto the API in §12.2.
- **No Wasm engine build and no Tauri.** The browser is only the front end.

---

## 15. Reference world (Phase 0)

```
world Dish {
    space Continuous2D(w: 512, h: 512, wrap: true, max_interaction_radius: 4.0);

    element C, N, E;
    substance light = E;  substance energy = E;  substance heat = E;
    substance sugar = C + E;  substance carbon = C;  substance nitrate = N;
    substance body = C + N;

    field cells : Grid(256, 256) holds [light, sugar, carbon, nitrate, body] (cap: 2_000_000);
    field scent : Grid(256, 256) signal f32;

    source sun  : light;
    source spark : energy;                            // only used to endow new lineages
    source rock : nitrate, carbon;
    sink   heat_sink : heat;
    remains: cells;                                   // where substances of dead entities go

    reaction photosynthesis : carbon + light            -> sugar;
    reaction respiration    : sugar                     -> carbon + energy;
    reaction build          : carbon + nitrate + energy -> body + heat;
    reaction rot            : body                      -> carbon + nitrate;

    reaction_space { include: [photosynthesis, respiration, build, rot];
                     generate: balanced(max_inputs: 3, max_outputs: 3, max_coeff: 2); }

    upkeep { pay: energy, to: heat_sink,
             base: 50, per_gene: 5, per_brain_node: 1, per_carried_unit: 0.01 }
    max_age: 20_000;

    // world rules
    rule sunlight on cells { transfer(light, from: sun, to: self, amount: 2_000, overflow: heat_sink); }
    rule weather  on cells { transfer(carbon, from: rock, to: self, amount: 20, overflow: heat_sink);
                             transfer(nitrate, from: rock, to: self, amount: 10, overflow: heat_sink); }
    rule decay    on cells { react(rot, rate: 0.5%); diffuse_conserved([sugar, carbon, nitrate], rate_bp: 300); }
    rule smell    on scent { scent = decay(diffuse(scent, rate: 0.2), 0.9); }

    senses  { /* as in §5.1 */ }
    actions { /* as in §5.1, without bond */ }

    genesis {
        for cell in cells { transfer(carbon, from: rock, to: cell, amount: 50_000);
                            transfer(nitrate, from: rock, to: cell, amount: 20_000); }
    }
}

lineage Drifter in Dish {
    profile fast { senses: 8, reactions: 4, actions: 6, brain: cgp(nodes: 32), registers: 4 }
    mutation { rate: 0.02, param_sigma: 0.1, expression_flip: 0.01, rules: Tinker }

    seed genome {                                     // a minimal hand-written starting point
        sense gradient(f: cells.light);
        sense inventory(s: energy);
        reaction photosynthesis rate 500;
        reaction respiration rate 500;
        action move(speed: 1.0) on out[0];
        action divide(split: 0.5) on out[1] > 0.5;
        brain { out[0] = Scale(in[0], 1.0); out[1] = Gt(in[2], 200.0); }
    }

    genesis(count: 2_000) {
        pos = philox_uniform(0..512, 0..512);
        transfer(energy, from: Dish.spark, amount: 5_000);    // endowment via a source
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
    line(metrics.max_chain);
    ledger(elements);
}
```

Notes:
- Survival requires chaining at least two reactions (photosynthesis, then respiration). Division requires `body`, which needs a third reaction (`build`) that the seed genome **doesn't have**. Discovering it, by mutation into the reaction space, is the first "surprise" milestone.
- Seed genome inputs are lanes in genome order: `in[0..1]` are the light gradient and `in[2]` is the energy inventory.

---

## 16. Roadmap

### Phase 0: evolving creatures, end to end (about 8–10 weeks)

1. **Language:** elements, substances, reactions (with balance checking), the reaction space (`include` plus `generate: balanced`), fields, sources and sinks, upkeep, world rules (transfer, react, diffuse, decay), menus, lineages with `profile fast`, seed genomes, `view` (field and agent layers), and `chart` (line and ledger).
2. **Runtime:** `Continuous2D` with a spatial hash, the 6-step tick pipeline, element ledger audits, pro-rata arbitration, death and division, point mutation, rewrite rulesets.
3. **Engine API and MCP:** author, observer and live-director basics (`world.create`/`patch`/`hotswap`, `stats.query`, `genome.explain`, `view.snapshot`).
4. **Browser UI:** live view, charts, entity inspector, a hot-swap editor (text; the node graph comes later), and a journal panel.
5. **Open-endedness metrics:** species counts, complexity, and longest reaction chain.

### Phase 0 exit criteria

| # | Test | Pass condition |
| :-- | :-- | :-- |
| 1 | Ledger | Zero element drift over 100k ticks, including 20 random hot-swaps |
| 2 | Cost of living | No entity survives 1,000 ticks without income; a bloated variant loses to a lean one in head-to-head runs |
| 3 | Evolution | In §15, at least 50% of 10 seeds evolve `build` usage within 200k ticks; a control run with mutation off never does |
| 4 | Hot-swap | Changing a world rule mid-run: no restart, under 200 ms pause, ledger intact, view updates within 1 s |
| 5 | AI loop | Through MCP only, Claude creates a world from a plain description, runs it, reads stats and a snapshot, hot-swaps a rule, and explains a genome. Scripted end-to-end test |
| 6 | Scale | 100k `fast` entities plus 2 fields at ≥ 30 ticks/s on the author's GPU; browser view at ≥ 30 fps |
| 7 | Replay | In `--deterministic` mode, same seed + journal ⇒ same state hash on the same machine. Required once passing; must not regress |

### Phase 1: freedom and exploration

`profile free` with structural mutation, channels, `Grid2D`, recombination, `fitness` blocks, `explore` with batched worlds and atlases, the node-graph editor, full open-endedness metrics (activity, novelty), the full set of visual layers, rewind, and the experimenter API.

### Phase 2: richer societies

Bonds and composite entities, exchanges and markets, the `Graph` space, horizontal gene transfer, evolvable mutation rulesets, and an AI director with budgets.

---

## 17. Open questions

- **Gene ↔ lane wiring in `free`:** whether positional wiring (shifts on insertion) gives better evolvability than labelled wiring (genes carry a lane tag).
- **Reaction space size:** `balanced(max 3, max 3, coeff 2)` over 8 substances may produce thousands of reactions. The space may need a locality-preserving order, or a cap on its size.
- **Species detection cost:** gene-level edit distance over 100k genomes is too expensive every tick, so it will need hashing with periodic sampling.
- **Batched worlds vs. one big world:** how GPU memory is split between them, and whether a world can run alone while others are batched.
- **AI director autonomy:** how much an AI may change unattended overnight, and how changes are summarised for the human in the morning.
- **Node-graph shape for genomes:** whether to show genes as cards and the brain as a DAG, or everything as one graph.

---

## Appendix A: what changed since v0.3

- **Entities:** a fixed scaffolding plus an evolvable brain became fully rule-based genomes (senses, reactions, actions, brain and memory) chosen from world-declared menus.
- **Conservation:** single quantities became element-based chemistry with balanced, evolvable reactions; the ledger now counts elements.
- **Space:** Phase 0 moved from the `Exclusive` grid to `Continuous2D`, with grids laid over it.
- **Execution:** added two profiles, `fast` and `free`, so each experiment chooses between scale and freedom.
- **Ruliology:** now covers all three meanings: local world rules, genome rewrite rules, and `explore` with atlases.
- **New capabilities:** hot-swapping, the journal, the AI-native API and MCP server, and open-endedness metrics.
- **Deployment:** the Wasm/Tauri dual deployment became a native engine with a browser UI on localhost.
- **Determinism:** relaxed from a gate to best effort, with a deterministic mode. The ledger stays exact.

**Kept from v0.3:** u32 fixed-point amounts with 64-bit ledgers, pro-rata arbitration, Philox, positional CGP, protected opcodes, rewrite rules with `where` guards, language-native views, and the warp-coherence techniques.
