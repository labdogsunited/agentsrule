# Emergent: a Rule-Based Language for Open-Ended Evolution

**Architecture & Design Specification — v0.7.9**

Status: draft. v0.7.9 records what the GPU runtime, the engine and the first renderer settled: the opt-in contract mode that makes the CPU reference and the GPU equal word for word on one driver (§10.1), the catalyst arithmetic and the `10_000bp` cap on a catalysed world rate (§4.5), view checking and still rendering (§11), and the implemented engine API, hot-swap subset and journal (§12.2, §13), with the exit tests they meet (§16, Appendix J). v0.7.8 defines mutation exactly, as the runner implements it, defines contact and the senses the spec only listed, adds rounds [P1] so that some entities can act several times per tick, and records the first evolution runs: exit test 9 now counts producers and passes, in 13 of 20 seeds (§15.1, Appendix I). v0.7.7 makes timing and writes exact: one authoritative table of what each step reads (`core.md` §6.2), one writer per free value unless a `combine` is declared, and atomic restructuring with defined rejection and fee behaviour (Appendix H). v0.7.6 records the reference world's 50k-tick baseline and control runs, which pass exit tests 7 and 8 in 10 of 10 seeds, and settles two findings of the `.em` compiler: `weigh` defaults and integer division shares (Appendix G). v0.7.5 recalibrates the reference world after the first runs of the CPU runner showed that nothing in it starved, and settles the runner's remaining findings: float arithmetic on amounts, exact geometry, no repulsion in P0, the genesis random pass, and canonical spellings found by the parser (Appendix F). v0.7.4 settles what the first CPU runner (`crates/emerge-ref`) found: a canonical request order, exact Philox keys, the `max_age` boundary, venting of dying entities, and two corrected claims about the reference world (Appendix E). v0.7.3 adopted the core's four-phase schedule as the only tick pipeline, rechecks the reference world under it, and tightens attribution, derived edges, the memory cost and catalysts (Appendix D). From v0.7.3 on, discrepancies the runner finds drive each revision. v0.7.2 added explicit catalysts, an optional cost for memory, per-gene attribution in the birth log and automatic labels, and widened the Phase 0 GPU spike (Appendix C). v0.7.1 added the birth log and made scale a tracked target (Appendix B). v0.7 superseded v0.6.1 and replaced the fixed circuit brain with **minds made of genes**: creatures regulate internal signals with evolvable genes, grouped into modules that can be duplicated and specialized, and the world chooses which kinds of regulation gene exist (§5). Appendix A lists those changes. Earlier versions are in `archive/`.

> **Being restructured.** `core.md` now defines the core language (a graph of nodes and edges, laws and owned rules, templates, invariants). This spec will be reorganized into the standard library and the creatures application built on that core. Where they disagree, `core.md` leads on the core model.

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
| Minds | No built-in brain. Creatures evolve networks of regulation genes over internal signals; the world chooses which kinds of regulation gene exist. Growth and modularity come first; learning within a lifetime comes later |
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
| **Signal** | Element-free `f32` information. **Field signals** lie over space (smells, messages); **internal signals** live inside one entity and make up its mind (§5.1) |
| **Store** | Anything inside the world that holds substances: a field cell or an entity's inventory |
| **Source / sink** | A **ledger boundary** outside the world. Sources emit substances into stores; sinks absorb them. They are not stores and hold no balance |
| **World rule** | A user-authored rule the world runs every tick. Evolution cannot change it; the user or AI can hot-swap it |
| **Entity** | An abstract agent with a stable ID, a position, a heading (stored as a unit vector), an inventory, internal signals and a **genome** |
| **Genome** | The entity's evolvable rules: sense, regulation, reaction and action genes, grouped into modules, plus its signal table (§5) |
| **Module** | A group of genes that can be duplicated, specialized and switched on or off as a unit (§5.6) |
| **Lineage** | A template that seeds entities. "Species" are detected from genome similarity at runtime, not declared |
| **Revision** | A number that identifies one version of a running world's definition. Every committed patch increments it (§13) |

---

## 3. The four fixed laws

### 3.1 Conservation

- **The ledger counts elements, not substances.** A reaction may turn substances into other substances only if the element totals on both sides are equal. For every element `e`, after every tick:

  ```
  Σ_stores amount_e − emitted_e + absorbed_e == 0
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
    per_signal: 1mu;
    per_carried_u: 1mu;
}
```

- **The payment route** is the reaction `pay → becomes`, followed by moving the product to the `vent` sink. The two substances **MUST** have the same element composition, the reaction **MUST** be downhill (§3.2), and `vent` must accept `becomes`. A payment of `x` µ debits `x` µ of `pay` from the entity, and the ledger records `x` µ of `becomes` absorbed by `vent`.
- **Action fees use the same route** [P0]. Menus write a fee as an amount only (`cost 10mu`), and it is paid in the `pay` substance. Separately declared payment routes for other substances are a later extension.
- **What upkeep counts.** `per_gene` applies to expressed genes only. `per_signal` applies to signals in use only (§5.1). The optional `per_signal_keep` (default `0mu`) adds `ceil(per_signal_keep × keep)` for each signal in use. It is a cost of **passive retention** only: a signal can also be kept by recurrent genes (a `linear` `weigh` gene writing a signal back to itself), which pay `per_gene` instead. Which kind of memory evolves under which costs is something to measure, not something the cost forbids. It depends only on the genome, so it is computed once per entity. `per_carried_u` applies to `floor(Σ_s amount_s / 1_000)`: amounts of all substances are summed first, then rounded down to whole units, so splitting an inventory across substances saves nothing.
- **`base` MUST be greater than `0mu`**, so no entity lives for free.
- **Rounding.** Totals are computed in `u64`. Costs that depend on an `f32` are rounded up to whole µ (§3.5).
- **Order.** Upkeep is charged first in Move (§7.2), before any fee, so a genome cannot skip it.
- **Failure.** An entity that cannot pay its upkeep in full is marked **dying** at once (§7.2) and does nothing else that tick.
- **Changes** to upkeep by hot-swap apply to every entity from the next tick (§13).

Because cost grows with active genes and signals in use, bloat is selected against, while dormant genes, dormant modules and unread signals stay free and keep neutral drift affordable.

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
| Signal values, gene weights, distances, speeds, angles | `f32` literal with a `.` (`1.0`), with the unit in the parameter name (`angle_deg`, `speed`) | `f32` |

- **Amounts from fractions round down.** `amount × bp / 10_000` uses `u64` and floors. The remainder stays where it was.
- **Costs never round in the entity's favour.** Costs that depend on an `f32` (for example `move` per distance) are rounded up to whole µ.
- **Senses report amounts in units** as `f32`, computed as `f32(amount) × 0.001f32` (`inventory(body)` reads `12_500 µ` as `12.5`). There is no division, because division is not bit-portable across GPUs. `0.001` is not exact in `f32`, so a reading can be off by one ULP (`10_500 µ` reads as `10.500001`), and a gate set at an exact amount may open one µ early or late.
- **Products of a float and an amount** (action sizes `floor(clamp(o, 0, 1) × max)`, gene reaction caps `floor(rate × clamp(s, 0, 1))`, the retention cost `ceil(per_signal_keep × keep)`, fees per distance) are computed in `f32`, with the amount converted first, and then rounded as stated. Every amount that enters such a product **MUST** be at most `2^24 µ` (about `16_777u`), so it converts to `f32` exactly; the compiler checks menu ranges, gene rates and costs against this.
- **Integer and float edge cases** (overflow, division by zero, NaN) follow the numeric contract in §10.1.

---

## 4. Chemistry: substances and reactions

### 4.1 Declaring a chemistry

```
element C;
element N;
element E;

second_law { min_drop: 1; }

substance light = E potential 10;
substance energy = E potential 8;
substance heat = E potential 0;
substance sugar = C + E potential 9;
substance carbon = C potential 0;
substance nitrate = N potential 0;
substance body = C + N potential 5;
```

A substance's composition counts elements per µ: `1 µ` of `sugar` holds `1 µ` of `C` and `1 µ` of `E`. The order of substance declarations is their **stable substance order**, used for canonical reactions (§4.4).

### 4.2 Reactions

```
reaction photosynthesis : light + carbon -> sugar; // ΔG = −1
reaction respiration : sugar -> energy + carbon; // ΔG = −1
reaction build : energy + carbon + nitrate -> heat + body; // ΔG = −3
reaction rot : body -> carbon + nitrate; // ΔG = −5
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
- **Size.** The compiler counts the space combinatorially before enumerating it, reports the count, and rejects a space larger than `limits.max_reaction_space` (§5.8).
- **Order.** The space is sorted by canonical form: the input terms, then the output terms, each as a list of `(substance, coefficient)` in substance declaration order, compared lexicographically. So neighbouring indices are similar reactions. A mutation of a reaction gene (§8.1) moves its index, half the time, by `1..8` entries up or down (clamped to the ends), and otherwise to a uniformly random entry, and lowers the gene's rate to the new reaction's maximum if needed. Evolution can discover reactions the user never wrote, while every reaction stays balanced and downhill by construction.

### 4.4 Canonical reactions

Every reaction, hand-written or generated, is put into **canonical form** before it is hashed, excluded, checked for `ΔG`, or given a rate:

1. **Aggregate:** repeated terms on one side are merged (`sugar + sugar` → `2 sugar`).
2. **Cancel:** a substance on both sides is removed from both by its smaller coefficient (`carbon + light → carbon + energy` → `light → energy`).
3. **Reduce:** all coefficients are divided by their greatest common divisor (`2 light → 2 energy` → `light → energy`).
4. **Sort:** terms on each side follow stable substance order (§4.1).

A reaction that cancels to nothing on either side is rejected. Because cancellation removes substances that appear on both sides, there are **no implicit catalysts**: a substance that is needed but not consumed cannot be written into a formula. Explicit catalysts (§4.5) cover that case.

### 4.5 Catalysts [P1]

A reaction may name a **catalyst**: a substance that speeds it up when present in the place where the reaction runs, without being consumed. The reaction is possible without it, at its normal rate.

```
reaction build : energy + carbon + nitrate -> heat + body
    catalyze iron(half: 100mu, boost: 10.0);
```

- **The ledger does not see it.** The catalyst is not part of the formula, so element balance, canonical form, `ΔG` and the reaction's persistent ID are unchanged. Two reactions with the same canonical form are the same reaction whatever their catalysts. The included reaction's catalyst wins, as for names and rates (§4.3).
- **Rate.** With `c` µ of the catalyst present, the reaction's cap becomes `max_rate × (1 + (boost − 1) × c / (c + half))`, rounded down to whole µ; a world reaction's `bp` rate is scaled by the same factor and then capped at `10_000bp`, so a world reaction never takes more than the cell holds (above that cap an amount would underflow). Without the catalyst the reaction runs as before. `boost` MUST be at least `1.0` and `half` greater than `0mu`, so a catalyst only ever speeds a reaction up. An entity reaction gene still scales this by its speed signal (§5.2).
- **When it is read.** The amount `c` is read when the reaction runs: for an entity's reaction gene, in Transform, after the genes before it have run; for a world reaction, in each cell when that rule runs.
- **Arithmetic.** The default mode computes the factor in `f64` and floors the product. Contract mode (§10.1) uses integers only, because a GPU need not round an `f64` division the same way: `frac = floor(c × 2^16 / (c + half))`, `k = floor((boost − 1) × 2^16 + 0.5)` (computed once, on the host), `g = k × frac` in `u64`, and the result is `cap + cap × floor(g / 2^32) + floor(cap × (g mod 2^32) / 2^32)`, saturating at `2^32 − 1`. It is below the `f64` result by at most about `(boost + 1) × 2^−16` of the cap, plus one µ. `emerge-ref` and `emerge-gpu` implement both ahead of P1.
- **Generated reactions** have no catalysts in P1. To give one a catalyst, a world declares that reaction by hand with its catalyst and includes it; the included reaction then replaces the generated one (§4.3).

Catalysts give creatures a reason to collect and keep substances they never use up, such as trace metals or cofactors, to make a pathway faster.

---

## 5. Entities and genomes

A creature has no special brain. Its genome is a list of **genes** grouped into **modules**. Every gene reads something and writes something. Its "mind" is the network that its genes form over **internal signals**.

### 5.1 Internal signals

- **Signals are element-free quantities.** A substance is made of elements and is conserved; a signal is made of nothing and is free information. Signals can rise, fall, be copied and fade, and neither conservation nor dissipation applies to them, because there is nothing in them to conserve. Field signals (§2) are the same idea laid over space; internal signals live inside one entity.
- **Signals belong to the genome**, not to the world. A genome has a **signal table**, and evolution can add signals to it (§8.1). Each signal is an `f32` value with an evolvable **keep** factor in `0.0..1.0` (default `0.0`) that sets how much of its value survives each mind step: `0.0` is a reflex, close to `1.0` is long memory.
- **Scope is derived, not declared.** A signal is **private** to a module if every gene that reads or writes it is in that module. Otherwise it is part of the module's **interface**.
- A signal is **in use** if at least one active gene writes it and at least one active gene reads it. Upkeep counts signals in use (§3.3).
- At birth every signal starts at `0.0`.

### 5.2 Genes

Every gene has one of four kinds, chosen from a world menu:

| Kind | Reads | Writes | Menu |
| :-- | :-- | :-- | :-- |
| **Sense** | the world, through a sense | one or two signals | `senses` |
| **Regulation** | up to four signals | one signal | `regulation` |
| **Reaction** | substances in the inventory, at a speed set by one signal | substances | `reaction_space` |
| **Action** | one signal | the world, through an action | `actions` |

- **Sources.** A gene that reads a signal may instead read a constant. Reaction and action genes read the constant `1.0` unless the genome says otherwise, so they run at full rate or fire at full size. Anything new in a genome is therefore immediately able to act once expressed.
- **Reaction speed.** A reaction gene runs at `floor(rate × clamp(s, 0, 1))` µ of extent per tick, where `s` is the signal it reads. This is how a creature's mind switches its metabolism on and off, like a cell expressing an enzyme.
- **Action size.** An action gene fires when its signal is above its threshold (default `0.0`). The signal value then sets the action's size (§5.5).
- **Expression.** Every gene and every module has an `expressed` bit. A gene is **active** only if it and its module are both expressed. Inactive genes are inherited and mutated but cost no upkeep and do nothing: an inactive sense writes nothing, an inactive regulation gene contributes nothing, an inactive reaction does not run, an inactive action emits no intent.
- **Order.** Genes have an order in the genome: modules in order, genes in order within a module. Order decides which reaction gene runs first (§7.3), which duplicate action counts (§5.5), and the order in which contributions to a signal are summed (§5.4).

### 5.3 Regulation gene kinds

The world declares which kinds of regulation gene exist, the same way it declares senses and actions. This is how each world chooses what kind of mind its creatures can evolve, using one engine.

```
regulation {
    gene weigh(
        bias: -16.0..16.0,
        curve: [linear, step, sigmoid, tanh]
    ) inputs 1..4 weight -16.0..16.0;
    gene product() inputs 2..2;
}
```

| Kind | Inputs | Output | Phase |
| :-- | :-- | :-- | :-- |
| `weigh` | 1–4, each with a weight | `curve(bias + Σ weight·input)` | P0 |
| `product` | 2, unweighted | `input₁ · input₂` | P0 |
| `delay` | 1 | the input as it was `ticks` mind steps ago | P1 |
| `add`, `sub`, `mul`, `div`, `min`, `max`, `compare`, `select`, `sin` | 1–3 | the arithmetic operation, protected as in v0.3 §5.3 | P1 |

Curves: `linear(x) = x`; `step(x) = x > 0 ? 1 : 0`; `sigmoid(x) = 1 / (1 + e^−x)`; `tanh(x)`. A `weigh` gene that omits a parameter gets `bias: 0.0` and `curve: linear`; an input written without a weight has weight `1.0`.

A world that offers only `weigh` gives its creatures something like a small recurrent neural network that can grow. A world that offers the arithmetic kinds with `keep 0.0` signals and enough mind steps can rebuild the circuit brains of v0.6 as a special case.

### 5.4 How a mind runs

Each tick, after the senses read the snapshot, the mind runs for `mind_steps` steps (a world setting, default `1`):

```
for step in 1 .. mind_steps:
    every active regulation gene computes its output from the signal values at the start of the step
    every signal s becomes:
        keep_s · s
        + the readings of the active sense genes that write s
        + the outputs of the active regulation genes that write s
    every signal is sanitized and clamped (§10.1)
```

- All genes read the same values and all signals update together, so feedback loops are allowed and order never changes the result. Contributions to one signal are summed in gene order, so float results are deterministic.
- A signal takes one mind step per regulation gene it passes through. Worlds that want faster creatures set more `mind_steps`, at more compute cost.
- After the last step, action genes read their signals to form intents, and reaction genes read theirs to set their speed for Transform (§7.3).

### 5.5 Menus: senses and actions

The world declares menus of possible **senses** and **actions**. Genes select items from the menus and set their parameters.

```
senses {
    sense probe(q: any quantity, angle_deg: -180.0..180.0, dist: 0.0..8.0);
    sense gradient(q: any quantity);
    sense inventory(s: any substance);
    sense crowd(radius: 0.0..6.0);
    sense neighbour(s: any substance, rank: 0..3);
    sense channel(ch: any channel); // [P1]
    sense clock(period: 1..1_000);
}

actions {
    action move(speed: 0.0..2.0) cost 20mu per distance;
    action turn(angle_deg: 0.0..30.0) cost 10mu;
    action take(s: any substance, max: 0u..10u) cost 5mu;
    action drop(s: any substance, max: 0u..10u) cost 0u;
    action give(s: any substance, max: 0u..10u, to: neighbour) cost 5mu;
    action steal(s: any substance, max: 0u..10u, from: neighbour) cost 30mu;
    action emit(sig: any signal, max: 0.0..10.0) cost 2mu;
    action send(ch: any channel, max: 0.0..1.0) cost 10mu; // [P1]
    action divide(split: 0.0..1.0) cost 0u requires body >= 10u;
    action bond(to: neighbour) cost 50mu; // [P2]
}
```

**Senses.** Two-output senses report in the entity's own frame: `gradient` writes the component along the heading and the component to the left, in units of the quantity per unit distance (§6.1 defines it exactly). Amounts are reported in units as `f32` (§3.5). The one-output senses:

| Sense | Reading |
| :-- | :-- |
| `inventory(s)` | the entity's own amount of `s` |
| `probe(q, angle_deg, dist)` | `q` in the cell under the point `dist` from the entity along its heading turned `angle_deg` to the left (the rotation of §6.1), wrapped or clamped like movement |
| `crowd(radius)` | the number of other living entities within `radius`, counted no further than `max_interaction_radius` |
| `neighbour(s, rank)` | the amount of `s` carried by the `rank`-th nearest other living entity within `max_interaction_radius` (rank `0` is the nearest), or `0.0` if there are fewer |
| `clock(period)` | `sin(2π × (tick mod period) / period)` |

Contact senses read the snapshot. "Nearest" is the same everywhere: squared distance in `f32`, measured with wrapping, ties to the lower entity ID.

**How a signal becomes an action.** With `o` the signal an action gene reads:

| Action | Size when it fires |
| :-- | :-- |
| `move` | distance `clamp(o, 0, 1) × speed` along the heading |
| `turn` | angle `clamp(o, −1, 1) × angle_deg`; positive turns left |
| `take`, `drop`, `give`, `steal` | amount `floor(clamp(o, 0, 1) × max)` µ |
| `emit` | field signal `clamp(o, 0, 1) × max` |
| `send` [P1] | value `clamp(o, −1, 1) × max` |
| `divide`, `bond` | no size; the gene's parameters apply |

**Several genes for the same action.**
- For `move`, `turn` and `divide`, only the **first firing gene in genome order** counts. Later ones are dropped before fees and pay nothing. A parent divides at most once per tick.
- Substance actions (`take`, `drop`, `give`, `steal`) and `emit` all count. Each is a separate request with its own fee.

**Targets.**
- `take` and `drop` act on `cell_at(pos)`, using the entity's position at the start of the tick.
- `give` and `steal` act on the **nearest living entity** in the snapshot within `max_interaction_radius`, measured with wrapping. Ties go to the lower entity ID. An entity never targets itself. Several contact genes of one entity all act on that same entity.
- If the target is dying this tick (§7.2), the transfer is dropped in Move, and the attempt fee is kept.
- `guard(s: any substance)` (experimental, 2026-10-19, `archive/plan-test9.md`) has no size and no target. When it fires, its fee is paid whether or not anyone steals, and every `steal` of `s` against the entity this tick is dropped in Move; the thief's fee is kept. It needs `max_interaction_radius`. Point mutation changes its substance or its threshold.
- If an action has no target (no neighbour in range, or a cell that cannot hold the substance), its intent is dropped in Set and **pays no fee**, like a duplicate action.

**Fees.** Fees are attempt fees (§7.2). `per distance` fees are charged on the **requested** distance, before movement is resolved.

**Requirements.** `requires` is a condition on the entity's own inventory (`language.md` §5). It is checked when the action takes effect: for `divide`, in Restructure, on the inventory after Transform (§7.5).

An experiment that doesn't want an action (for example `steal`) simply leaves it out. A world **MUST NOT** list a menu item from a later phase than the one the engine implements. A runner that lacks a menu item may still run the world as a **partial run** when asked to: it reports the item as warning `W1001`, no gene may use it, and mutation never produces it.

### 5.6 Modules

Genes are grouped into **modules**: runs of genes that sit together in the genome. A module has a name in source (evolved modules get generated names like `m3`) and an `expressed` bit.

- **Modules are units of reuse.** Duplicating a module (§8.1) copies all its genes. Its **private** signals are replaced with fresh signals in the copy, and its **interface** signals are shared. The copy is an independent second version of a working sub-network, which mutation can then specialize, as one limb design becomes legs and arms.
- **Modules are units of expression.** A whole module can be switched off or on by one mutation.
- Reuse is by copying, as in biology, not by calling shared code. Nothing is called at runtime; every gene simply runs.

### 5.7 Profiles: capacity and execution

Every lineage uses the same genome model and the same mutation operators, including growth (§8.1). A **profile** sets only how large a genome may grow and how it is executed. Several profiles can coexist in one world.

```
profile fast { genes: 32; signals: 16; modules: 8; }
```

| | `fast` [P0] | `free` [P0 on CPU, P1 on GPU] |
| :-- | :-- | :-- |
| Capacity | Small and fixed per lineage (for example 32 genes, 16 signals, 8 modules) | Large (for example 256 genes, 64 signals, 32 modules) |
| Storage | Fixed-size arrays per entity; unused capacity is empty | Arena-allocated, variable length |
| Execution | GPU lockstep; gene slots read their kind and parameters as data, or entities are binned by genome shape, as the GPU spike decides (§16) | CPU, then GPU, by the same method |
| Target scale | 100k+ entities | 1k–20k entities |
| Best for | Large ecologies, visual spectacle, statistics | Open-ended growth over long runs |

A structural mutation that would exceed a profile's capacity is skipped. A genome can migrate from `fast` to `free`, but not the other way.

### 5.8 Limits

Every world declares its limits. The compiler checks static limits and the runtime enforces dynamic ones.

```
limits {
    max_entities: 200_000;
    max_reaction_space: 65_536; // reaction indices fit in u16
    inventory_cap: 100u; // per substance, per entity
}
```

- Intents per entity are bounded by the number of action genes, and so by the profile's capacity.
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

Fields are grids laid over any space, and signals can be sampled bilinearly. On `Continuous2D`, geometry is defined exactly, in `f32`, so the CPU and the GPU make the same choices wherever the arithmetic allows:

- **Cells.** `cell_at(pos) = (floor(x × (grid_w / w)), floor(y × (grid_h / h)))`, with the ratio computed first, each clamped to the grid, and the cell index is `y × grid_w + x`. Cell `(x, y − 1)` is **north** of `(x, y)`.
- **Wrapping.** A coordinate wraps as `rem_euclid(c, size)`, and a result equal to `size` (possible in `f32`) becomes `0`. Without wrapping, it is clamped to `0` and the largest `f32` below `size`.
- **Gradient.** `gradient(q)` uses central differences over the four neighbouring cells, `gx = (east − west) × (grid_w / (2 × w))` and `gy = (south − north) × (grid_h / (2 × h))`, with neighbours wrapped or, without wrapping, clamped to the edge. Values are in units (§3.5). The vector is then projected onto the entity's heading `(hx, hy)` and onto its left `(−hy, hx)`.
- **Turning.** A turn by angle `a` rotates the heading, `(hx cos a − hy sin a, hx sin a + hy cos a)`, and then divides it by its length, so rounding cannot make the heading drift away from unit length.

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

The creatures application runs on the core schedule (`core.md` §6): every tick has four phases, **Set**, **Move**, **Transform** and **Restructure**, and every rule, world rule or gene, contributes to them from one snapshot. **`core.md` §6.2 is authoritative for what each step reads and when its writes become visible**; it maps each creature step (senses, mind, intents, upkeep, fees, transfers, movement, reactions, venting, disposal, division) to its row, and names the runner test that shows it. This section describes the steps; where it seems to disagree with that table, the table wins.

```
┌ 1 Set ────────────┬ 2 Move ──────────────┬ 3 Transform ──────┬ 4 Restructure ──┐
│ senses read the   │ upkeep, fees,        │ entity reactions, │ disposal,       │
│ snapshot; minds   │ entity and world     │ world reactions,  │ division,       │
│ run; field signal │ transfers together,  │ waste venting     │ mutation        │
│ rules → intents   │ movement             │                   │                 │
└───────────────────┴──────────────────────┴───────────────────┴─────────────────┘
           hot-swaps and director actions are applied only between ticks
```

**Rounds [P1].** A world may set `rounds: R` so that some lineages act several times per tick: Set and Move repeat once per round, each with its own snapshot and arbitration, and Transform and Restructure run once (`core.md` §6.5). A lineage's profile sets how many rounds it acts in (`profile fast { …; rounds: 3; }`), and an upkeep term `per_round` charges for each round it acts in. With `rounds: 1`, the default, this section describes the whole tick.

All ordering below is part of the contract. The CPU reference executor (§14) implements it literally, and GPU kernels must match it as described in §10.2.

**Change from v0.7.2.** Earlier versions ran world rules as a fifth step after entity reactions, one rule after another, each seeing the state the previous rule left. Now world rules are split by kind into the phases: signal rules run in Set, transfer rules (including sources and diffusion) are arbitrated in Move **together with entity transfers**, and reaction rules run in Transform. Declaration order matters only among reaction rules in the same cell (§7.4).

**Snapshot.** The snapshot is the state at the start of the tick. Everything that reads outside an entity's own signals reads the snapshot: senses, world-rule conditions, signal rules and derived edges (`core.md` §2.3), which stay fixed for the whole tick even after entities move.

### 7.1 Set

1. **Senses.** Each living entity evaluates its active sense genes against the snapshot.
2. **Minds.** Each entity runs its mind for `mind_steps` steps (§5.4). These are the core's substeps: they read and write only the entity's own signals, so they need no arbitration.
3. **Intents.** Action genes read the resulting signals and become intents, in genome order, with the rules for duplicate actions and missing targets (§5.5). Reaction genes read theirs to set their speed for Transform.
4. **Field signals.** Signal rules compute each cell's new value from the snapshot. `emit` intents add to it (the combine operator is `sum`). The results are committed together.

### 7.2 Move

The steps run in this order:

1. **Upkeep.** Each entity's upkeep is computed (§3.3). If its `pay` balance covers it, it is paid through the payment route. If not, the entity is marked **dying**: it pays nothing, its intents are cancelled, it runs no reactions, it cannot divide, and it is excluded from every transfer this tick, as origin or destination. Its inventory is disposed of in Restructure.
2. **Action fees.** Intents are charged in genome order, through the payment route. An intent whose fee the entity can't afford at that point is dropped and pays nothing. A fee is an **attempt fee**: it is paid in full even if the action later moves less than requested.
3. **Transfers.** All substance-moving requests are resolved together in two stages, using the balances after steps 1 and 2: entity intents (`take`, `drop`, `give`, `steal`) and every statement of every transfer rule (§7.4), including source inflows and diffusion. Incoming amounts therefore cannot fund outgoing amounts in the same step.
   - **Stage 1, per origin and substance:** if requests exceed the origin's balance, each request is granted pro-rata. A source origin has no balance, so stage 1 grants every request from a source in full.
   - **Stage 2, per destination and substance:** if the stage-1 grants exceed the destination's headroom (cap minus balance at the start of the step), each grant is reduced pro-rata. Outflows during this step do not create headroom. A sink destination has no cap.
   - **Commit:** each transfer **delivers** its stage-2 amount. If it names an overflow sink, it also **overflows** `stage-1 grant − stage-2 grant` to that sink. The origin is debited `delivered + overflowed`; a source origin records the same sum as emitted. Anything not delivered or overflowed stays at its origin, and an amount the origin could not supply in stage 1 is simply never moved.
   - **Request order.** Requests have one canonical order: first entity intents, by entity ID and then genome order; then transfer rules, by declaration order, then cell index, then statement order, with each diffusion's destinations in neighbour order (N, E, S, W on a grid). Within each stage, a store's requests for one substance keep this order, and a request's **position** is its index among them.
   - **Remainders:** each stage floors the pro-rata shares and then hands out the leftover µ one at a time, largest fractional part first. Among equal fractional parts, the request with the smaller Philox draw goes first (§10.2: the stream is the store being arbitrated, the pass is `arbitrate_origin` in stage 1 and `arbitrate_dest` in stage 2, the draw index is the request's position), and then the lower position. Draws prevent a systematic advantage for low entity IDs.

   Some capacity may be left unused when stage 2 reduces a grant that stage 1 limited. That is accepted for P0.
4. **Movement.** Each entity whose movement fees were paid turns and then moves by its intents. In P0 there is no repulsion: entities may overlap, and crowding acts only through shared cells and contention. A soft repulsion force [P1] will be computed as a gather from the snapshot, so it needs no atomics; its formula, range and strength are open (§17). Dying entities do not move. Positions change here, but derived edges and each entity's field cell for the rest of the tick are those of the snapshot.

### 7.3 Transform

Transforms use the balances left by Move.

**Entity reactions.** Each living entity runs its active reaction genes **in genome order**, one after another. For each gene:

```
extent = min( floor(rate × clamp(s, 0, 1)),   // s: the signal the gene reads (§5.2)
              min over inputs i: floor(balance_i / coeff_i),
              min over outputs o: floor(headroom_o / coeff_o) )
```

The reaction then consumes and produces `extent × coeff` µ of each substance atomically. Because reactions are canonical (§4.4), no substance is both an input and an output. Two genes that want the same input are served in genome order, and a gene can use products made by an earlier gene in the same tick (**same-tick chaining is allowed**). Running genes in a fixed order within one entity needs no atomics.

**World reactions.** In each cell, reaction rules run in declaration order, each statement in order, with the extent rule of §4.2. Cells and entities are independent of each other in this phase.

**Venting.** After the reactions, every substance the world marks as `waste` (`waste heat -> void;`) is moved from each inventory to its sink, including the inventories of dying entities: their waste goes to the waste sink, not to `remains`. A sink has no cap, so venting cannot conflict with anything.

### 7.4 World rules

World rules are the application's laws. Each rule holds **one kind of effect**, and the compiler rejects a rule that mixes kinds (`language.md` §7). The kind decides the phase:

| Kind | Statements | Phase | Execution |
| :-- | :-- | :-- | :-- |
| **Signal rule** | signal assignments | Set | Every cell reads the snapshot and writes a new value. If several rules write one field, their writes combine as the field declares (`signal combine max`; one writer needs no declaration, `language.md` §6.3), and a cell no rule writes keeps its value. Then the amounts of `emit` intents are added (§7.1), and all values are committed together |
| **Transfer rule** | `transfer`, `diffuse_conserved` | Move | Every statement of every transfer rule, in every cell, is a request in the same arbitration as entity transfers (§7.2 step 3). Diffusion offers `floor(amount × rate / 10_000)` of each listed substance per cell, split equally between its four von Neumann neighbours, with the remainder staying put. Overflow follows §7.2 |
| **Reaction rule** | `react` | Transform | In each cell, in declaration order of rules and then of statements, limited by inputs and output headroom (§7.3) |

Conditions (`when`) read the snapshot. **Source inflows** are transfer rules with a source origin.

Because transfer rules are arbitrated together, their declaration order does not change what moves where. A world that needs "diffuse, then rot" gets it from the phases (Move before Transform), not from the order of its rules.

### 7.5 Restructure

1. **Disposal.** An entity dies if it is dying (§7.2) or has reached `max_age`: it dies in the tick in which the number of ticks it has lived, counting this one, reaches `max_age`, so it acts in exactly `max_age` ticks. If the world names a `decompose` reaction (`decompose: rot;`), that reaction first runs on the inventory to its full extent, `min over inputs i: floor(balance_i / coeff_i)`, with no rate limit and no cap on its outputs; it is a declared reaction, so it conserves elements and lowers potential. Then each substance in its inventory goes to its field cell, as far as the cell holds that substance and has headroom. Everything else goes to the world's `remains` sink. The compiler **MUST** check that `remains` accepts every substance that cannot always reach a field cell. Disposals are applied in entity-ID order, so competing for a cell's headroom is deterministic.
2. **Division.** An entity divides if its first firing `divide` intent survived Move (its fee was paid), the action's `requires` condition holds on its inventory **after Transform**, and `max_entities` is not reached. The child gets `floor(amount × split_bp / 10_000)` of every substance, where `split_bp = round(split × 10_000)` is fixed when the genome is built, so division uses integers only and is exact on CPU and GPU (the remainder stays with the parent), a new stable entity ID, a mutated copy of the genome (§8), and its parent's heading vector, copied exactly. The child is placed at `pos − heading × child_offset`, wrapped to the space, where `pos` is the parent's position after movement, `heading` is its stored unit vector and `child_offset` is a world setting (`child_offset: 0.5;`, default `0.5`). This uses no trigonometry and no random draw, so CPU and GPU place children identically. Children are created in parent-ID order, so IDs are deterministic.
3. **Newborns** join at the end of the tick. They are in the next tick's snapshot and first act then.
4. **Compaction and binning** of the entity buffer (not of genomes) prepare the next tick.

---

## 8. Evolution

### 8.1 Variation

Variation is applied to the child's genome at division, in this order:

1. **Point mutations** of gene parameters, menu choices, reaction IDs, weights, signal `keep` factors, and expression bits of genes and modules. The rate is per gene and set per lineage. Rates can optionally evolve too (`rate: evolvable;`).
2. **Wiring mutations:** `rewire` moves one input of a gene to another signal in the table or to the constant `1.0`; `diverge` points a gene's output at a fresh signal, so a copy can stop interfering with its original.
3. **Growth mutations,** in every profile, within its capacity (§5.7):
   - `duplicate_gene` copies a gene next to itself;
   - `insert_gene` adds a gene drawn uniformly from the menus, reading random existing signals and writing a fresh one (actions and reactions read `1.0`);
   - `delete_gene` removes a gene and any signal nothing uses any more;
   - `duplicate_module` copies a module, with fresh private signals and shared interface signals (§5.6);
   - `delete_module`, `split_module` (at a random gene) and `merge_modules` (two neighbours).

   **New material starts dormant.** Duplicated and inserted genes, and duplicated modules, start unexpressed. They cost nothing and drift freely until an expression flip switches them on. This keeps growth neutral rather than costly.
4. **Rewrite rulesets:** user-authored pattern rules on genes (§9.2).
5. **Recombination** [P1]: a mate's genome is crossed over, aligned by module, when the world enables `divide(mate: true)`.
6. **Horizontal transfer** [P2]: an optional `share_gene` action copies one gene or module to a neighbour, at a cost.

Gene-level rates apply per gene and module-level rates per module, at each division.

A run may switch variation off for control experiments (`--mutation off`). A lineage with `mutation` needs a `profile`; if it declares several, the first is used.

**Exact operators.** The steps above leave choices open, which the runner fixes as follows, so that CPU and GPU produce the same child:

- **Draws.** All draws use Philox (§10.2) with pass `mutate = 3` and the child's ID as the stream, numbered sequentially from `0` as the operators consume them, in the order of this section. Every decision ("does this gene mutate?") is one draw, compared with its rate, so later draw indices depend on earlier outcomes: a GPU kernel must run the operators in the same order. A uniform draw `u` is as in §10.2; a normal draw uses Box–Muller from two uniform draws, `sqrt(−2 ln(1 − u₁)) × cos(2π u₂)`; "uniform in `0..n`" is `min(floor(u × n), n − 1)`.
- **Point mutations (step 1).** For each gene in genome order, with probability `rate`, **one** of its fields changes, chosen uniformly: for `weigh`, the bias, the curve or one weight; for a sense, its substance or one of its parameters; for an action, a parameter or the threshold (for `take`, `drop`, `give` and `steal`: the substance, drawn from the action's `s` list on the menu, `max` or the threshold); for a reaction gene, the reaction (§4.3) or the rate. A float moves by `N(0, param_sigma × (hi − lo))` and is clamped to the menu's range; amounts are then rounded to whole µ, and integer parameters to whole numbers. Thresholds have no menu range: they move by `N(0, param_sigma)` and are clamped to `±16`. Then each signal's `keep` changes with probability `rate`, by `N(0, param_sigma)`, within `0..1`. Then each gene's and each module's expression bit flips with probability `expression_flip`.
- **Wiring (step 2).** For each gene, with probability `rewire`, one of its inputs moves to a uniformly chosen signal; for an action or reaction gene's source, the constant `1.0` is one more choice. Then, with probability `diverge`, one of its outputs moves to a fresh signal with `keep 0.0`, if the profile has room.
- **Growth (step 3).** Gene operators run per gene in genome order (duplicate, insert, delete), then module operators per module (duplicate, delete, split, merge). An inserted gene's kind is uniform over the kinds the menus offer, with parameters uniform in their ranges; regulation genes read uniformly chosen existing signals. A module's **private** signals are those no gene outside it uses. `delete_module` never removes the last module; a genome may lose all its genes, and its entity then does nothing and starves. Afterwards, signals no gene uses are removed and the rest renumbered in order.

### 8.2 Selection

Selection is implicit: an entity that can't pay upkeep dies, and one that gathers enough resources divides. Optional `fitness` blocks [P1] add goal-directed pressure. They **only** gate or weight `divide`; they never kill entities or move substances.

### 8.3 Measuring open-endedness

These metrics are built into the runtime and streamed as `observe` columns:

- **Species:** clusters of similar genomes (hash buckets, refined by gene-level edit distance on a sample), plus counts of births and extinctions.
- **Evolutionary activity** (Bedau–Packard): how long new genes persist, and whether new adaptive genes keep appearing. Activity is reported next to a **neutral control** (the same world with selection replaced by random survival), because persistence alone does not show adaptation.
- **Complexity:** active genes, signals in use, modules, distinct reactions in use, and **chain depth**.
- **Growth and modularity:** genome size over time; how many modules descend from a duplication, and how many of those have **diverged** (the copies differ in at least one active gene); the depth of the signal network (longest path from a sense to an action through regulation genes, cycles collapsed as for chain depth).
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
        transfer(grain, from: self, to: below, amount: 1u) @transfer_grain;
    }
}

rule life on board {
    let n = sum(moore.value);
    self.value = select(when: n == 3.0 or (self.value > 0.5 and n == 2.0), then: 1.0, else: 0.0);
}
```

Rules that move substances are arbitrated together with every other transfer (§7.2, §7.4), so conservation still holds. Signal rules such as `life` are pure cellular automata.

### 9.2 Rules rewrite programs

`ruleset` blocks hold pattern → replacement rules applied to genes during variation (§8.1), with weights and `where` guards (from v0.3 §5.5). A pattern matches a gene's kind, menu item and parameters; parameters and wiring it does not mention are kept:

```
ruleset Tinker {
    rule sharpen weight 0.3 : regulate weigh(curve: sigmoid) -> regulate weigh(curve: step);
    rule nudge weight 0.3 : sense $kind(dist: $d) -> sense $kind(dist: jitter($d, sigma: 0.1));
}
```

A rewrite that would produce a genome outside the menus, the reaction space or the profile's shape is discarded, and the genome is left as it was.

In [P2], rulesets can themselves be genes, so the way an entity mutates can evolve.

### 9.3 Exploring rule spaces

`explore` blocks run many variants of a world and rank the results:

```
explore ChemistryAtlas {
    base: Dish;
    vary Dish.reaction_space.generate.max_coeff in [1, 2, 3];
    vary Dish.rules.rotting@react_rot.rate in logspace(from: 10bp, to: 500bp, steps: 8);
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
| Float magnitude | internal signals clamped to `±1e4` after every mind step; field signals and sense readings clamped to `±1e6` |
| Curves | `step` and `linear` are exact. `sigmoid` and `tanh` use the engine's float functions in the default mode, and the defined division-free curves (§17) in contract mode |
| `amount(s, units: x)` | `floor(x × 1_000)` µ; NaN or negative gives `0`; values above `2^31` µ give `2^31` µ |
| Ledger totals | `u64`; the runtime halts before any total exceeds `2^63` |

Denormal floats may be flushed to zero. Fused multiply-add is allowed only where the source says `MulAdd`, and everywhere contract mode defines it.

**Contract mode** (`--contract gpu`, `emerge-ref`'s `World::contract`; off by default). An opt-in set of definitions for the float operations of a tick, chosen so a GPU can reproduce them bit for bit. The default mode is unchanged by it, so archived runs stay reproducible; results differ slightly between the two modes, so runs are compared only within one mode.

- Minds: contributions summed from zero in gene order, every multiply-add fused (`mul_add`), and a multiply-add result below `2^−100` in magnitude is `0`; the curves are `tanh_d` and `sigmoid_d` (§17).
- Movement and senses: a defined polynomial `sincos` in place of `sin` and `cos`; a turned heading renormalised by one Newton step, `h × (1.5 − 0.5 × h·h)`, with no `sqrt` or division; the gradient's `1 / 2w` computed once on the host; the clock phase as an integer fraction; `gradient` and `delta` take the difference of two amounts in integers, with its sign, before converting it to `f32` once.
- Mutation: a normal draw is the sum of twelve Philox uniforms minus 6, instead of Box–Muller; rounding is `floor(x + 0.5)`.
- Catalysts: the integer factor of §4.5.

Exactness is per driver: the NVIDIA driver fuses every `a×b + c` as `mul_add` does, llvmpipe never does. So the GPU runtime equals the CPU reference in contract mode word for word on one driver (§10.2), and across drivers only statistical agreement is claimed. The `emerge-gpu` runtime runs in contract mode only.

**Known deviation of the default mode.** `delta(s)` subtracts two `u32` amounts, so a decrease wraps to a large positive reading (clamped to the reading limit) instead of a negative one. Contract mode uses the signed difference. The default mode is left as it is so archived runs reproduce; whether to fix it is open (`crates/emerge-ref/DISCREPANCIES.md`).

### 10.2 Determinism

- All randomness uses Philox-4x32-10. Each draw is named by `(seed, tick, stream, pass, index)`, and maps to one Philox call exactly:
  - **key** = `(seed & 0xFFFF_FFFF, seed >> 32)`;
  - **counter** = `(index / 4, tick, stream, (pass << 24) | (domain << 16) | aux)`, and the draw is word `index mod 4` of the output;
  - **stream** is an entity ID (domain `0`) or a cell index `y × w + x` (domain `1`). For anything a world rule does to a cell, the domain is `2` and `aux` is the rule's declaration index; in arbitration, `aux` is the substance's declaration index; otherwise `aux` is `0`;
  - **passes** are numbered: `arbitrate_origin = 1`, `arbitrate_dest = 2`, `mutate = 3`, `rewrite = 4`, `genesis = 5`. A new pass gets the next number.
  - **genesis** draws, for each entity a lineage's `genesis` block creates, use the new entity's ID as the stream: draw `0` for `x`, `1` for `y`, and `2` for the heading angle (`angle = u × 2π`, heading `(cos, sin)`). A draw becomes a uniform `u` in `[0, 1)` as `(draw >> 8) × 2^−24`, and a range `a..b` as `a + u × (b − a)`.

  This limits a run to `2^32` ticks, `2^32` entity IDs, and `2^16` substances and world rules, and each pass to `2^34` draws per stream per tick. Accumulation uses integers. There are no float atomics.
- **Same seed, same device, same journal ⇒ same run** (`--deterministic`). The journal (§13.3) records everything needed to reproduce the run.
- The conservation ledger is exact everywhere and is never relaxed, in every mode.
- Bit-identical results across different GPUs are a non-goal.
- **CPU and GPU agree exactly only on integer contracts, unless both use contract mode.** In the default mode, transcendental functions (`sigmoid`, `tanh`, `sin`, `sqrt`) and movement differ between CPU and GPU, and a tiny float difference can change which cell an entity is in. Contract mode (§10.1) removes those differences on one driver: there, full states of whole worlds, with mutation, are compared word for word (`proposals/gpu-runtime.md`). Without it, the reference executor is compared with GPU kernels in two ways (§16):
  - **Contract fixtures** use fixed positions, axis-aligned headings (`(±1, 0)` or `(0, ±1)`), no `turn` actions, and minds made only of exact operations (`weigh` with `step` or `linear` curves, small integer weights and biases, `product` of small integers). On these, every integer in the state must match exactly. Child placement (§7.5) needs no trigonometry, so it is exact in fixtures.
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
  - genome inspector with modules, active genes, and the signal network with live signal values;
  - histogram of each signal's value across a species;
  - parent/child diff;
  - phylogeny and Muller plots, built from the birth log (§13.4);
  - a chemistry graph of which reactions are in use, their `ΔG`, and the flows between species [P1].
- **Interaction:** selecting and following entities, play/pause/step, rewind (checkpoints plus re-simulation, in deterministic mode), and side-by-side comparison of runs [P1].
- **AI-readable views:** every view can also return a snapshot as a PNG plus a JSON summary of what is on screen (counts, ranges, notable regions). This is for multimodal AIs (§12).

**Implemented (v0.7.9).**
- **Checking.** `emerge check` resolves views and charts (`language.md` §9.2): an unknown name is `E0114`, a value of the wrong kind `E0205`, an empty or non-positive log range `E0204`. A binding that is valid but not computed yet (`species_id`, `by: species`, `metrics.…`) is warning `W1201`; the file stays valid.
- **Still rendering.** `emerge-view` draws a view from one frame (grid values, agents, tick, revision) to PNG on the CPU with `tiny-skia`, not through WGSL: fields as exact cells (every cell's colour is the palette's mapping of that cell's value, with no smoothing), agents with glyphs, colour and size, and a legend. `emerge-server render` fills the frame from a journal replayed to a tick, or from a dish, lineages, seed and flags run to a tick, on either backend; the GPU's frame equals the CPU's in contract mode. An unavailable binding is drawn grey and named in the legend. `made` (µ of a substance the agent's own reactions made) is counted only over an explicit window (`--made S --window W`), which the legend states; counting changes nothing in the run.
- **Not built:** the browser renderer and streaming, WGSL from views, charts, evolution views, interaction, and the PNG-plus-summary snapshot (the API's `view.snapshot` returns the field and a sample of agents as JSON, §12.2).
- **Direction (decided 2026-10-21, not implemented):** live viewing first, with recording and export (PNG, EXR, image sequences, MOV) driven by the same view definition; a VEX-like view language with layers, compositing, glow and editable palettes (`proposals/visual-language.md` §11, `proposals/vex-language.md`).

---

## 12. AI-native design

### 12.1 Language rules for machine authoring

- **One canonical form.** `emerge fmt` produces the only accepted formatting, and every construct has exactly one spelling (`language.md` §1).
- **Explicit everything.** Arguments are named except for the one subject argument of designated built-ins, and every quantity has a unit (§3.5). `emerge fmt --explicit` prints every default.
- **A small grammar** with a machine-readable definition (`language.md` §11). `emerge docs --llm` prints a single reference file under 30k tokens, written for language models.
- **Paths and persistent IDs.** Every declaration, gene, statement, view and node has a readable **path** (`Dish.rules.rotting`) and a **persistent ID** (a `u64`). Paths can change when items are added or renamed; IDs never change and are never reused.
  - IDs are stored in `.emerge/ids.json` next to the sources, and are kept in version control with them.
  - Genes and effect statements are addressed by **label**, never by position. `emerge fmt` adds a derived label to any that lack one (`language.md` §10.1), so an AI patching a world never has to count items.
  - When sources are reloaded, items are matched to existing IDs by label (`@name`, `language.md` §10), then by named path, then by kind and position. Unmatched items get new IDs, and IDs of vanished items are retired.
  - Edits are **patches** (`add`, `set`, `replace`, `remove`, `rename`) written against paths **at a stated revision**. The engine resolves paths to IDs at that revision and records the IDs in the journal (§13).
- **Structured diagnostics:** JSON with an error code, location, explanation and machine-applicable suggested fixes (`language.md` §14). Chemistry errors include each reaction's element balance, canonical form and `ΔG`.
- **Explain.** `explain genome <id>` returns a plain-language summary ("module forage: senses body to the side and turns toward it; module metabolism: takes carbon and light and respires; module reproduce: divides at body ≥ 10u") plus the active signal network as formulas.

### 12.2 One API for everything

The engine exposes a single protocol: JSON-RPC over WebSocket, plus binary frames for bulk data. The browser UI, the CLI and an MCP server all use it, and **anything the UI can do, the API can do**.

| Role | API surface |
| :-- | :-- |
| **Author** | `world.create` (with the lineages to seed), `world.patch`, `world.validate`, `docs.query` |
| **Observer** | `stats.query`, `entities.query` (filter, sample, aggregate), `genome.explain`, `view.snapshot`, `metrics.openendedness`, `events.subscribe`, `lineage.query` (ancestors, descendants and the mutations between them, from the birth log, §13.4) |
| **Experimenter** | `explore.run`, `explore.status`, `explore.results`, `run.fork`, `run.compare` |
| **Live director** | `world.hotswap`, `source.adjust`, `entity.inject` (spawn designed genomes, endowed from a source), `schedule` (timed events), `run.pause`/`run.step`/`run.resume` |

Every call that changes a world takes a `base_revision` and is rejected if it is stale.

**Implemented (v0.7.9, `crates/emerge-server`).** `emerge-server serve` speaks JSON-RPC 2.0 over WebSocket on `127.0.0.1`; one engine thread owns the runs and running worlds tick between calls. Methods: `world.create`, `world.list`, `world.validate`, `world.source`, `world.hotswap`, `run.step`, `run.pause`, `run.resume`, `stats.query`, `view.snapshot`, `entities.query`, `genome.explain`, `journal.read`, `docs.query`, `docs.example`; and on this server only, `view.subscribe` and `view.unsubscribe`, which push frames of a view as binary messages (a JSON header, then the same PNG `render` writes), and `view.record` and `view.recordings`, which write a PNG sequence and a manifest under `--journal-dir` (`emerge-server mov` turns one into a MOV with `ffmpeg`). A rejected swap is error `-32001` with its kind, and the current revision when stale. `genome.explain` and `entities.query` need the CPU backend (GPU genomes are not decoded back yet). Binary frames, `world.patch` as a separate call, and the experimenter and most director calls are not built.

- **Local safety.** A WebSocket handshake that carries an `Origin` header (a web page) is refused with 403, so a page cannot drive the engine. With `serve --viewer`, the server also answers `GET /` with its own page and admits a page's socket only from its own origin, with its own `Host`, and with a random per-process token passed in the URL fragment and sent as `Sec-WebSocket-Protocol: emerge.k.<token>` (`proposals/live-viewer.md`; added 2026-10-21). Callers cannot name file paths: journals are written only into the server's `--journal-dir`, as `<world>.jsonl`, and never overwrite a file. `run.step` stops after 30 s.
- **MCP.** `emerge-server mcp` is the MCP adapter over stdio: thirteen tools, one for each API method except `run.pause` and `run.resume` (an MCP client advances a world with `run.step`). It serves both the handshake era of MCP (`initialize`, revisions up to 2025-11-25) and the current one (2026-07-28: `server/discover`, the version in each request's `_meta`). Tool names are the method names with `_` for the dot (`world_create` calls `world.create`), because some clients accept only letters, digits, `_` and `-` (changed 2026-10-21; a dotted name is rejected with the new name in the message).

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
| Menus (adding senses, actions or regulation kinds) | Next tick. Existing genes keep their choices; new items become reachable by mutation. Removing a menu item tombstones the genes that use it (they become unexpressed) |
| `mind_steps` | Next tick |
| Reaction space, including potentials | Next tick. Reactions keep their IDs. A removed or now-uphill reaction becomes a **tombstone**: genes that point at it become unexpressed but keep its ID, so it can never silently point at a different reaction |
| Rulesets, mutation rates | At births from the next tick |
| Views and charts | Instant; no effect on the simulation |
| Adding substances or fields | New stores start empty, or are filled from a source |
| Removing substances or fields | The swap **MUST** name a destination for the contents (another store or a sink that accepts them). Reactions, genes, senses and views that use the substance are tombstoned or removed in the same swap |
| Space type or size | Not swappable. Fork the run instead |

**Implemented subset (v0.7.9, `proposals/engine-api.md`).** Both backends swap: world rules (their amounts and rates, adding and removing rules or statements: inflows, `react`, `diffuse_conserved`), sources and sinks added after the existing ones, `upkeep` costs, `max_age`, `max_entities`, `mind_steps`, a lineage's mutation rates, and raised caps. A lowered cap is accepted only if no balance is above it (`move_to` is not built). Everything else in the table above is rejected as "not swappable yet" and changes nothing: substances, elements, reactions and the reaction space, fields, menus, action fees, lineage seeds and genesis, rulesets. The check is fail-closed: the current source and the candidate are compiled freshly, the swappable fields are blanked in both, and any other difference rejects the swap, so a part nobody listed is never half-applied.

### 13.2 How a swap works

Each world has a **revision** number, starting at `0` and incremented by every committed swap.

1. **Stage:** the patch states its base revision. If that is not the current revision, the patch is rejected as stale, and the client must rebase it.
2. **Compile and validate:** compile the whole candidate world, including GPU pipelines, and check every dependency, unit, law, route and limit. This runs while the world keeps running, and does not depend on current balances.
3. **Preflight,** at the next tick boundary with the world paused: check the revision again; then check the patch against **current** balances: every migration destination has room or is a sink, and every lowered cap has a destination for its excess. Reserve that room. If anything fails, reject the patch; nothing has changed.
4. **Commit:** migrate stores, remap genes, switch configuration and pipelines, and increment the revision. Commit only moves data into room reserved in step 3, so it cannot fail.
5. **Resume.**

At most one patch commits per tick boundary, in the order received; others wait or become stale. The **pause** target (steps 3 and 4) is under 200 ms for a rule change. Compile time is measured and reported separately.

Measured (2026-10-20): a rule change pauses a CPU world 0.28 ms (compile 38 ms, apart), and the full dish on the GPU 5.6 ms (65_536 cells, 5_843 entities; compile 151 ms). On the GPU a commit writes only the configuration parameters and re-makes the buffers sized by the laws; the kernels do not change, so no pipeline is rebuilt.

### 13.3 Journal

The run journal is an append-only log of:

- the engine version, compiler version, backend and device identity, and mode flags (`--deterministic`, `--mutation off`, any opt-outs);
- the seed, the lineages seeded, the full initial source (or its content hash plus the files) and `.emerge/ids.json`;
- every committed swap, director action and `entity.inject`, with its tick, base revision, and operations **resolved to persistent IDs**;
- every rejected swap, with the reason.

A run can be reproduced (in deterministic mode, on the same device) from its journal alone.

**Implemented (v0.7.9, `crates/emerge-server/src/journal.rs`).** JSON lines. A header with the engine version, the backend, contract mode and mutation flags, the seed, the lineages, and the full source in canonical form with its hash; one line per swap with its tick, base revision, the patch text and reason, and either `committed` with the new source's hash or `rejected` with the reason; an end line with a digest of the state. `replay` rebuilds a run from the text alone and checks each swap's outcome and the end digest; a changed patch, digest or mode flag does not replay. On the GPU, where a genome sits in the pools depends on the order in which children got room, so the GPU digest hashes genomes by content. **Gaps:** no compiler version or device identity beyond the backend, no `.emerge/ids.json`, and swaps are recorded as patch text, not as operations resolved to persistent IDs (persistent IDs are not built yet). Director actions and `entity.inject` are not built.

### 13.4 Birth log

The journal records what the user and AI did. The **birth log** records what evolution did. It is a separate append-only log with one record per birth:

- tick, parent ID and child ID (`entity.inject` and genesis births have no parent);
- the **mutation diff**: each mutation operator applied (§8.1), with the genes or modules it touched by persistent ID, and their values before and after;
- the conserved amounts the child started with.

Deaths are recorded with tick, entity ID and cause (starvation, a rule, a hot-swap).

Together these give the full family tree with every change along each branch. Phylogeny and Muller views (§11) and `lineage.query` (§12.2) read from it. Questions such as "which mutations did the lineages that survived the last 5,000 ticks have in common" can then be answered from records, not inferred from snapshots.

**Attribution counters.** Each entity keeps two `u32` counters per gene slot, both saturating: the µ of an **attributed substance** that gene **acquired** (moved into the entity by its `take` or `steal`), and the µ of it that the gene **produced** (reaction outputs). The attributed substances are listed in `birth_log { attribute: [body]; }`; the default is the substance in the `divide` action's `requires` condition (`body` in Dish). Counters reset at birth and after each division, and are written to the log at division and death. An entity writes only its own counters, so they need no atomics. In the `fast` profile this is 256 bytes per entity for each attributed substance.

The counters are **gross**: they report each gene's share of what the entity acquired and produced since birth or its previous division, for example "in descendants where `@build` is active, it produced 84% of the `body` gained since the previous division". They do not say where the inventory present at division came from. A gene that produces `body` which the entity then drops counts as much as one whose `body` is kept, and inventory inherited from the parent is not counted at all. Tracing the origin of retained inventory would need explicit allocation rules and is not specified.

- The birth log is on by default. It is not needed for replay (the journal is enough in deterministic mode), so it is written outside the simulation step and may lag behind it.
- At 100k entities a log can grow by millions of records per hour. It is stored in compact binary chunks, and a world may set `birth_log { keep: all | ancestors_of_living | off; }` (alongside `attribute`). `ancestors_of_living` periodically prunes branches with no living descendants.

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

- **Engine:** a Rust workspace with `emerge-lang` (parser, formatter, type checker, unit and chemistry checks, IR, IDs, patch format), `emerge-ref` (CPU reference executor), `emerge-gpu` (WGSL generation and kernels), `emerge-runtime` (scheduler, ledger, journal, revisions), `emerge-server` (API) and `emerge-cli`. **No UI dependencies.** As built (v0.7.9): `emerge-lang`, `emerge-ref` (with the `emerge` CLI), `emerge-gpu` (hand-written WGSL kernels, its own `emerge-gpu` binary, contract mode only), `emerge-server` (runs on either backend, hot-swap, journal, API, MCP, `render`), `emerge-view` (still renderer) and `emerge-gpu-spike` (the spike's measurements). There is no separate `emerge-runtime` or `emerge-cli` yet.
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
| `Drifter` | Three modules: `forage` (moves forward, steers toward `body` in the cells), `metabolism` (photosynthesis and respiration, takes light and carbon) and `reproduce` (takes body, divides at `body ≥ 10u`) | The evolution test, and the baseline with mutation off |
| `Builder` | As `Drifter`, plus `build` and `take(nitrate)` in `metabolism` | The positive control: shows the world supports body synthesis at all |

A run chooses which lineages to seed (`--lineages`, or `world.create` in the API).

The world offers one regulation gene kind, `weigh`, and runs one mind step per tick. `Drifter`'s seed genome:

```
seed genome {
    module forage {
        sense gradient(q: cells.body) -> food_x, food_y @food_dir;
        regulate weigh(curve: tanh) <- [10.0 * food_y] -> steer @weigh_steer;
        action move(speed: 1.0) @go;
        action turn(angle_deg: 30.0) <- steer threshold -2.0 @turn_to_food;
    }

    module metabolism {
        reaction photosynthesis rate 1u @photosynthesis;
        reaction respiration rate 1u @respiration;
        action take(s: light, max: 1u) @eat_light;
        action take(s: carbon, max: 1u) @eat_carbon;
    }

    module reproduce {
        sense inventory(s: body) -> fullness @inventory_fullness;
        regulate weigh(bias: -10.0, curve: step) <- [fullness] -> ready @weigh_ready;
        action take(s: body, max: 1u) @eat_body;
        action divide(split: 0.5) <- ready threshold 0.5 @split;
    }
}
```

Genes without `<-` read the constant `1.0`: `move` always fires at full speed, the reactions run at full rate, and the `take` genes always take. `turn` reads `steer`, a `tanh` in `−1..1`, with `threshold -2.0`, so it fires every tick with a signed angle. With the default threshold `0.0` it would turn only left, and `-1.0` would miss a saturated `tanh`, which is exactly `−1.0` in `f32`. `Builder` runs `build` at `50mu` per tick.

### 15.1 Why this world should evolve

These are design arguments, checked against the CPU reference runner (`crates/emerge-ref`) with mutation off. *v0.7.5 recalibrated the world, because the runner showed that the v0.7.4 numbers did not limit the population: see the last bullet.*

- **Energy limits the population.** `sunlight` adds `40mu` of light per cell per tick, and a cell holds at most `1u` of light, so the whole dish receives about `2_621u` of light per tick and cannot stockpile it. Each tick a seed takes light and carbon, and in the same tick (Transform follows Move) runs photosynthesis and then respiration, which gives up to `1u` of energy, but only as much as the light it found. Its costs are about `20mu` base, `5mu × 12` active genes, `1mu × 4` signals in use (`food_x` is written but never read, so it is free), `20mu` for moving, `10mu` for turning (every tick), `15mu` in take fees, and `1mu` per unit carried: about `130mu` per tick. A lone seed finds plenty, since a full cell refills in 25 ticks; in a crowd the light is shared. The population therefore grows until income per seed falls to its costs, and seeds that forage badly starve. The `5u` endowment covers more than 30 ticks without income.
- **Seeds forage.** `move` always fires at full speed, and `turn` steers up the gradient of `body` in the cells. Seeds do not sense light, so grazed and ungrazed light is left for evolution to find.
- **Seeds can reproduce before discovering body synthesis.** `body` arrives from the detritus rain (`1mu` per cell per tick, plus `1u` per cell at genesis) and from the dead: disposal (§7.5) drops an entity's storable contents into its cell, so body is recycled. Rot removes it at `50bp` per tick, so an ungrazed cell ends every tick at `199mu`. A seed must travel and collect `10u` to divide, and divides as soon as it has, so births are limited by body, and deaths by energy. Divisions are where mutation happens.
- **Measured (CPU runner, `emerge run examples/dish.em --mutation off`, 10 seeds each, 50_000 ticks).** Both lineages settle within 1_000 ticks and stay there. `Drifter`: about `9_340` entities (lowest `9_114`, highest `9_582` over all windows after tick 5_000), about 115 births and as many deaths per tick, so a life lasts about 80 ticks; mean energy `8.6u`. `Builder`: about `4_040` entities (lowest `3_553`), about 200 births per tick and lives of about 20 ticks; mean energy `4.6u`, and entity reactions make about `190u` of body per tick. `build` lets `Builder` divide more often, and halving its energy at each division kills most newborns. Energy is far from the `100u` cap in both, so energy binds. Exit tests 7 and 8 pass in 10 of 10 seeds (§16).
- **Sensitivity.** With `threshold -1.0` on `turn`, populations were about `10_300` and `7_000`: the gradient of `body` is often steep (the dead drop their body into their cell), so `steer` often saturates at exactly `−1.0`, and those seeds did not turn. A `Builder` with `build` at `250mu` dies out, and at `10mu` of light per cell some seeds lose all `Builder`s in the first ticks; `40mu` leaves a margin of about two.
- **Why v0.7.4 was recalibrated.** With `2u` of light per cell per tick and a `2_000u` cap, a seed earned up to `1u` of energy for about `150mu` of costs. Nothing starved: mean energy reached the `100u` cap, and the only deaths were from `max_age`. Body recycling made the rain a limit on the birth rate, not on numbers, so the population grew without bound: `Drifter` passed `89_000` entities by tick 9_000 and was still growing, and `Builder` hit `max_entities` (`200_000`) before tick 1_000. Exit tests 7 and 8 would have passed with no selection at all, which is useless for test 9.
- **Discovering body synthesis is the milestone, and it is a test of growth.** Body synthesis needs an active reaction that produces `body` (`build`, or a generated one such as `carbon + nitrate + light → body + heat`, ΔG = −5) and an active `take(nitrate)`. The natural route is duplication and divergence: copy `eat_carbon` and change its substance to `nitrate`; copy a reaction gene and move its reaction ID to one that makes `body`; switch both copies on. Copies start dormant (§8.1), so each step is free until the last. The `fast` profile leaves room for 20 more genes. The test measures **body synthesis flux**, not a particular reaction.
- **No free lunch.** The dissipation law blocks `heat → energy` (ΔG = +8), `carbon + nitrate → body` without energy (ΔG = +5), and reversed respiration `carbon + energy → sugar` (ΔG = +1).
- **Shortcuts that remain are physics.** `light → energy` (ΔG = −2) is downhill, so it is generated, capped at `250mu` per tick, while the curated chain runs at up to `1u`. To forbid it, add `exclude: [light -> energy];`.
- **First evolution runs (exit test 9, v0.7.8).** Ten seeds of `Drifter` with mutation on, as a partial run (no `scent`, `smell`, `emit` or `Tinker`):
  - **Without contact** (`give`, `steal` and contact senses not yet in the runner; stopped at 31_000–35_000 ticks): genomes shrank from 12 active genes to 6–8, because every gene and its fees cost upkeep, and entities hoarded 11–20u of energy. Body synthesis stayed near zero, with under 1% of entities carrying a gene for it.
  - **With contact** (stopped at 41_000–48_000 ticks): `steal` spread to 98–100% of entities in every seed within 10_000 ticks. Mean energy fell to about 1u, populations to 8_000–13_000, and lifetimes to 25–40 ticks. Genomes grew to 10–18 active genes. Body was made in almost every window of every seed, but genes for it stayed with 0.3–7% of entities, and one seed of ten reached the old test's 10% (at tick 19_000). In a world of thieves, stealing `body` competes with making it: the most common genome of one seed steals both energy and body.
  - **Carrying is not producing.** A census of evolved genomes (`emerge run --census`) found body-making genes with no `take(nitrate)`, which cannot run. Test 9 therefore now counts **producers**, entities whose own reactions made `body` in the window, and asks for 2% of them rather than 10% of carriers (§16). A rerun with this measure is pending. If it fails too, the world is recalibrated so that making `body` pays better than stealing it.
- **Test 9 status (2026-10-19): passed, 13 of 20 seeds.** The dish below with only `Drifter`'s `duplicate_gene` raised from 0.005 to 0.02 (`archive/test9-dish.em`, `dish=fnv64:348b96e0c416e860`): 13 of 20 seeds keep at least 2% producers for 10_000 ticks within 40_000 ticks, sustained at ticks 14_000–39_000, with 4–99% producers at the end. A 14th seed (7) completes its 10_000 ticks exactly at tick 40_000 and is not counted. The rule was set before the batch: 10 or more of 20 passes, fewer leaves the earlier result. Earlier result, before that change: partly passed, 7 of 20 seeds. Full record in `archive/plan-test9.md`. Final dish: `examples/dish.em` recalibrated (sunlight 320mu, weather body 1mu, `max_age` 1_000, `decompose: rot;`, `steal` 1u for 200mu, `max_interaction_radius` 1.0) plus the experimental `guard` action (§5.5). Unseeded `Drifter`, mutation on, 40_000 ticks: 7 of 20 seeds keep at least 2% producers for 10_000 ticks (sustained at ticks 19_000–38_000, with 5–88% producers at the end); the target is 10 of 20. Without `guard`: 6 of 20, and without generated reactions: 1 of 20. What we learned:
  - Making `body` must pay better than collecting it: with the original dish, producers lost even with `steal` removed.
  - A body route breaks under mutation more often than a `steal` gene does (13.5% against 3.2% per division), and theft spreads to 95–100% of entities.
  - With `guard`, producers that reach 2% hold on in every seed (6 of 6, against 6 of 10 without it).
  - **Discovery or early establishment appears to be the bottleneck:** producers never reach 2% in 13 of 20 seeds. Every body route needs `take(nitrate)` as well as a body reaction, and the dish gives only about 10 births per tick. Under mutation alone a complete route appears in about 0.03–0.10% of lines; raising `duplicate_gene` from 0.005 to 0.02 makes that 6–10× more common, without more `steal(body)`. That has not been tried under selection. The output cannot yet tell producers that never appear from producers that appear and die out quickly.
- **Stepping-stone diagnostic (2026-10-19): joining the two pieces is the bottleneck.** Lineage `Halfway` is `Drifter` plus `reaction build rate 50mu` in `metabolism`, with no `take(nitrate)` (`archive/halfway-dish.em`, the test 9 dish with `Halfway` appended). Seeds 1–20, same flags, unseeded for `take(nitrate)`. The rule was set before the batch: 14 or more of 20 sustained means joining is the bottleneck, 8 or fewer means establishment is. Result: 20 of 20 sustained, most at ticks 10_000–11_000 (the earliest possible), the latest at 37_000, against 7 of 20 for `Drifter` in the same dish. With the body reaction already present, `take(nitrate)` is found and producers establish in every seed. A replay of `Drifter` seed 2 agrees: of 191_952 births, 4_353 carried a body reaction and 2_712 a `take(nitrate)`, but only 117 both. A catalysed-respiration stepping stone (nitrate as catalyst for `respiration`) did not work, because energy in this dish is limited by light, not by reaction speed. Record in `archive/plan-next-2026-10.md`; the catalysed dish is `archive/catalyst-dish.em`.

### 15.2 Ledger check

Every reaction balances elements and is canonical. `void` accepts every substance, so every overflow, payment, waste and disposal route type-checks. `energy` and `heat` cannot be stored in `cells`, so they go to `remains: void` when an entity dies. Action fees and upkeep both leave as `heat` through `vent: void`.

---

## 16. Roadmap

### Phase 0: evolving creatures, end to end (about 12–14 weeks)

The first milestone is narrow on purpose: **one canonical world that parses, formats, runs on the CPU, conserves elements through every fee and overflow, reproduces without mutation, and rejects a bad hot-swap without changing state.** Everything else builds on it.

1. **Parser and formatter:** the grammar in `language.md` §11, with `examples/dish.em` as the fixture.
2. **Contracts:** the CPU reference executor for §5.4, §7 and §10.1, with unit tests for contention (two origins into one destination, two genes on one input, an origin without enough stock into a full destination with overflow), rounding and remainders, dying entities, fees, canonical reactions, the dissipation law, synchronous signal updates, and every growth mutation.
3. **Language:** elements, substances with potentials, canonical reactions, the reaction space (`include`, `generate: balanced`, `exclude`, kinetics), units, limits, fields, sources and sinks, waste venting, the payment route, world rules of the three kinds, menus (senses, actions and regulation kinds `weigh` and `product`), lineages with profiles, seed genomes with modules and signals, labels and persistent IDs, rulesets, `view` (field and agent layers), and `chart` (line and ledger).
4. **Growth:** all mutation operators of §8.1, including module duplication, on the CPU for both profiles.
5. **Runtime (GPU):** `Continuous2D` with a spatial hash, the four-phase tick pipeline and the `fast` profile checked against the reference executor, element ledger audits, two-stage arbitration, death and division, all mutation operators within `fast` capacity, rewrite rulesets. Start with a **GPU spike** in the first weeks, before the runtime is built around its answers:
   - **Varied genomes.** Compare two ways of running them: a loop over gene slots that reads each slot's kind and parameters as data, and binning entities by gene-kind sequence. Measure both on genomes that have drifted for many generations, where nearly every lineage has a different sequence and bins become small. Measure the slot loop's cost as the number of template kinds grows.
   - **Portable curves.** Check whether a division-free `sigmoid` and `tanh` (§17) give bit-identical results on the target GPUs and on the CPU, and what they cost.
   - Dense contact and view streaming. Scale (100k entities) is tracked, not a gate.
6. **Engine API and MCP:** author, observer and live-director basics (`world.create`/`patch`/`hotswap` with revisions, `stats.query`, `genome.explain`, `view.snapshot`).
7. **Browser UI:** live view, charts, entity inspector (modules, genes, signal network), a hot-swap editor (text; the node graph comes later), and a journal panel.
8. **Open-endedness metrics:** species counts, complexity, growth and modularity, chain depth, reaction flux.
9. **Birth log** (§13.4), with `lineage.query`.

### Phase 0 exit criteria

| # | Test | Pass condition |
| :-- | :-- | :-- |
| 1 | Parser | `examples/dish.em` parses; parse → format → parse gives the same AST; formatting it changes nothing |
| 2 | Ledger | Zero element drift over 100k ticks, including 20 random valid hot-swaps and 20 rejected ones, on GPU and on the reference executor |
| 3 | Contracts | On every contract fixture (§10.2), the GPU runtime matches every integer in the reference executor's state exactly. On `examples/dish.em`, population, births and body flux on GPU and CPU agree within 5% over 10 seeds |
| 4 | Dissipation | The compiler rejects every uphill or non-canonical reaction in a test suite, and the generated reaction space of §15 contains no reaction with `ΔG > −1` and no duplicate canonical forms |
| 5 | Growth operators | One million random mutations, in both profiles, produce only valid genomes within capacity. Module duplication gives the copy fresh private signals and shares interface signals, in unit tests |
| 6 | Cost of living | An entity with no income, `S` µ of the `pay` substance and minimum upkeep `u` µ per tick dies no later than tick `ceil(S / u) + 1`. A bloated variant loses to a lean one in head-to-head runs |
| 7 | Baseline | `Drifter` with mutation off survives 50k ticks with births every 1,000 ticks, in at least 9 of 10 seeds |
| 8 | Positive control | `Builder` with mutation off survives 50k ticks with body synthesis flux above zero, in at least 9 of 10 seeds |
| 9 | Evolution | `Drifter` with mutation on reaches body synthesis flux above zero, made in every window of 1,000 ticks by ≥ 2% of the population (**producers**: entities whose own reactions made some `body` in the window), sustained over 10k ticks, within 200k ticks, in at least 5 of 10 seeds. The baseline (test 7) never does |
| 10 | Hot-swap | Changing a world rule mid-run: no restart, pause under 200 ms, ledger intact, view updates within 1 s. A swap that fails validation, compilation or preflight, or that is stale, leaves the world and its revision unchanged |
| 11 | AI loop | Through MCP only, Claude creates a world from a plain description, runs it, reads stats and a snapshot, hot-swaps a rule, and explains a genome. Scripted end-to-end test |

**Status (v0.7.9, 2026-10-20).** Tests 1 to 11 are met, with these qualifiers:
- **2:** on both backends, `emerge-server exit2` (`archive/exit2-cpu-journal.jsonl`, `archive/exit2-gpu-journal.jsonl`; each replays to the same digest). The CPU half ran in the default mode, the GPU half in contract mode.
- **3:** the contract fixtures, rebuilt on a laid grid, match every integer of the CPU's state after every tick (worlds with a space; `.em` worlds always have one). The statistical half was measured against the CPU in contract mode, where the GPU agrees exactly: full states on `Drifter` and `Builder` in `examples/dish.em`, 10 seeds each, compared over 150 ticks, and populations on a 30× light variant equal every 1_000 ticks up to 9_000. Against the default-mode CPU, only final populations of exit tests 7 and 8 were compared (`proposals/gpu-runtime.md`, M3); the 5% rule against the default mode is not established.
- **5, 7, 8:** also met on the GPU (one profile; the runner has one).
- **10:** met with `view.snapshot` standing in for the browser view, which is not built.
- **11:** met by a scripted MCP client and by one real session with Claude Haiku.

If test 7 or 8 fails, the world is recalibrated before test 9 is attempted: a failing baseline means the world can't support the seeds, and a failing control means it can't support the target metabolism.

**Tracked, not blocking:**
- In `--deterministic` mode, same journal ⇒ same state hash on the same machine. Once this passes, a regression is a bug.
- In the evolution runs: genome size over time, and the number of duplicated modules that have diverged and stay active (§8.3). These are the first evidence of growth and modularity.
- **Scale:** 100k `fast` entities plus 2 fields at ≥ 30 ticks/s on a named GPU and workload (recorded with the result); browser view at ≥ 30 fps. This is the target, but Phase 0 does not wait for it: `core.md` generalizes the substrate to graphs, and the target will be re-examined once the GPU spike (step 5) has numbers.
- **Turnaround:** wall-clock time for (a) the test-11 script, from plain description to explained genome, and (b) 10 seeds of `examples/dish.em` for 50k ticks, from command to results table. Recorded with each release, so slow compiles, startup and tooling show up, not only tick rate. The measure that matters is how many useful experiments fit in an hour.
- **AI repair effort:** in test 11 and in a fixed set of authoring tasks (write a world from a description, add a reaction, fix a broken world), how many rejected validations and patches came before success, and how many tokens were used. A rise means the diagnostics or the docs got worse.

### Phase 1: freedom and exploration

`profile free` on the GPU, the `delay` and arithmetic regulation kinds, channels, `Grid2D`, recombination, `fitness` blocks, `explore` with batched worlds and atlases, the node-graph editor, full open-endedness metrics (activity with neutral controls, novelty), the full set of visual layers, rewind, and the experimenter API.

### Phase 2: richer societies and learning

Bonds and composite entities, exchanges and markets, the `Graph` space, horizontal gene transfer, evolvable mutation rulesets, **plastic regulation genes** (learning within a lifetime: weights that change according to a reward signal), and an AI director with budgets.

---

## 17. Open questions

- **Potential values:** whether hand-tuned integer potentials are enough, or whether worlds need tools to derive them (for example from a list of reactions that must be downhill).
- **Other payment routes:** whether actions should be able to cost substances other than the upkeep payment, each with its own declared conversion and vent.
- **Mind speed:** whether one mind step per tick is enough for interesting behaviour, or whether worlds will routinely need `mind_steps` of 3 or more, and what that costs on the GPU.
- **Module operators:** whether evolution also needs to move single genes between modules, and whether module boundaries should influence mutation rates.
- **GPU execution of varied genomes:** in `fast`, entities with different gene kinds in different orders diverge on the GPU. Binning by gene-kind sequence may break down once most lineages differ; a slot loop that treats gene kinds as data avoids that, but still branches on kind and pays for every template on the menu. The GPU spike (§16) compares them. **Spike result (2026-10-20, `proposals/gpu-spike.md`):** a slot loop over gene slots, with each slot's kind read as data, runs the minds of 100_000 evolved entities in 1.8 ms per tick, and 100_000 entirely different random 32-gene genomes in about 6.5 ms; binning by shape is not cheaper (235 mind shapes in one evolved population: 1.7 ms of dispatch overhead alone, 64 ms to build each shape's kernel). The slot loop is the method for the runtime.
- **Learning:** the shape of plastic genes (Hebbian, reward-modulated) and what reward signal a world offers.
- **Repulsion:** whether worlds need a soft repulsion force (§7.2), and its formula, range and strength. P0 runs without one; first measure whether entities pile up in ways that matter.
- **Arbitration fairness:** whether the capacity left unused by two-stage arbitration matters in dense worlds, and whether a second pass should redistribute it.
- **Portable transcendental functions:** defining `sigmoid` and `tanh` as specific functions, not as their mathematical ideals, would allow exact CPU/GPU comparison of minds. Any such definition must avoid division, because WGSL division is only accurate to 2.5 ULP and may differ between GPUs: a clamped polynomial, or a table indexed by an integer, built only from `+`, `−` and `×`, which are correctly rounded. It also needs the backend not to fuse them into FMA. Rational (Padé) forms such as `x(27 + x²) / (27 + 9x²)` divide, and are only valid for `|x| ≤ 3`. **Decision (2026-10-16, to test in the spike):** `tanh(x) = u * (2 − |u|)` with `u = clamp(x * 0.5, −1, 1)`. It is close to the real `tanh` (slope 1 at 0, `f(1) = 0.75` against 0.76) and exactly ±1 for `|x| ≥ 2`, so `threshold -2.0` stays valid. `sigmoid(x) = 0.5 + 0.5 * tanh(x / 2)` with the same form, so its input is `clamp(x * 0.25, −1, 1)`. Only `+ − ×`, `abs` and `clamp`: no division, and the backend must not fuse into FMA. The cubic `0.5c − c³/54` is the fallback. The GPU spike (§16) tests this. `sin` and `sqrt` in movement remain. **Spike result (2026-10-20):** on an RTX 3090 the GPU flushes denormals, so `tanh` as written differs from the CPU for `|x| < 2^-125`; with the rule "an input below `2^-100` in magnitude is `0`" (`tanh_d`, `sigmoid_d`) both curves match the CPU on all 2^32 bit patterns. **Decided (2026-10-20, left to Claude by the user): option 1, exact per driver.** The arithmetic around the curves is not portable. The NVIDIA driver fuses every `a*b + c` (bit-identical to `f32::mul_add`), llvmpipe never does, not even for WGSL `fma()`. Minds are therefore bit-exact between CPU and GPU only per driver (the CPU reference using `mul_add`), unless the minds become integer or fixed-point, or the backend can forbid contraction. The decision: the GPU is checked against a CPU reference that uses `mul_add` for every multiply-add of a mind and the defined curves, with the rule that a multiply-add result below `2^-100` in magnitude is `0`; across different drivers only the statistical agreement of exit test 3 is claimed. Integer minds and a no-contraction backend stay open for later. Done as an opt-in mode instead of a switch (2026-10-20): `emerge-ref --contract gpu` (§10.1), so the default runner and archived runs are unchanged.
- **Continuous expression:** whether genes and modules need an evolvable expression level in `0.0..1.0` instead of an on/off bit, so new material can be faded in rather than switched on at full strength. First measure whether switching on is actually harmful: the share of expression flips that are followed by early death or no offspring, from the birth log (§13.4).
- **Patch rebasing:** whether the engine should rebase simple stale patches automatically (for example a `set` on an item no other patch touched) instead of rejecting them.
- **Species detection cost:** gene-level edit distance over 100k genomes is too expensive every tick, so it will need hashing with periodic sampling.
- **Batched worlds vs. one big world:** how GPU memory is split between them, and whether a world can run alone while others are batched.
- **AI director autonomy:** how much an AI may change unattended overnight, and how changes are summarised for the human in the morning.
- **Node-graph shape for genomes:** whether to show modules as cards and the signal network as a graph, or everything as one graph.
- **Causal analysis:** the birth log (§13.4) records which mutations happened, not why a lineage survived. Explaining an outcome needs events linked by cause (a move fed a reaction, which paid for a division). Kappa's causal **stories**, compressed traces of the events that led to an outcome, are the closest prior art. The question is what to record, and at what cost, for such traces to be built after the fact.

---

## Appendix A: what changed since v0.6.1

- **Minds (§5):** the CGP brain, input and output lanes, registers and the 16 opcodes are gone. A creature's mind is a network of **regulation genes** over **internal signals**, which are element-free quantities that conservation and dissipation do not touch. Signals have an evolvable `keep` factor that sets memory length.
- **One gene shape (§5.2):** sense, regulation, reaction and action genes all read something and write something. Reaction genes read a signal that sets their speed, so minds control metabolism.
- **Regulation menu (§5.3):** the world declares which regulation gene kinds exist (`weigh` and `product` in P0; `delay` and arithmetic kinds in P1). One engine supports many kinds of mind.
- **Mind steps (§5.4):** signals update synchronously, `mind_steps` times per tick, a world setting.
- **Modules (§5.6):** genes are grouped into modules, which can be duplicated (fresh private signals, shared interface), split, merged, deleted and switched on or off.
- **Profiles (§5.7):** `fast` and `free` share one genome model and the same operators; they differ only in capacity and execution. Growth is in Phase 0 on the CPU for both, and on the GPU for `fast`.
- **Growth mutations (§8.1):** gene and module duplication, insertion, deletion, rewiring and divergence; new material starts dormant.
- **Upkeep (§3.3):** `per_signal` replaces `per_brain_node`.
- **Defaults:** genes that read nothing read the constant `1.0`. This replaces v0.6.1's rule for unassigned action lanes.
- **Metrics (§8.3):** genome size, module duplication and divergence, and signal-network depth.
- **Reference world (§15):** seed genomes rewritten as three modules; the milestone is now reached by duplication and divergence.
- **Roadmap (§16):** Phase 0 gains growth and a growth-operator test (criterion 5) and is estimated at 12–14 weeks; learning moves to Phase 2 as plastic genes.
- **Archive:** earlier appendices are in the archived specs.

## Appendix B: what changed in v0.7.1

- **Birth log (§13.4):** a new append-only log of every birth (parent, child, mutation diff, starting amounts) and death, with retention settings. Phylogeny and Muller views (§11) read from it, and the observer API gains `lineage.query` (§12.2). Added to Phase 0 as roadmap step 9.
- **Scale is tracked, not a gate (§16):** former exit criterion 12 (100k entities at 30 ticks/s) moves to "tracked, not blocking". Roadmap step 5 starts with a GPU spike that measures divergence cost early.
- **Turnaround (§16):** wall-clock time for the AI loop and for a 10-seed run of the reference world is recorded with each release.
- **AI repair effort (§16):** rejected attempts and tokens per authoring task are recorded alongside test 11.
- **Open questions (§17):** causal analysis, with Kappa's causal stories as prior art.

## Appendix C: what changed in v0.7.2

- **Catalysts (§4.5) [P1]:** a reaction may name a catalyst that raises its rate cap without being consumed. Balance, canonical form, `ΔG` and IDs are unchanged. Removed from the open questions.
- **Cost of memory (§3.3):** optional `per_signal_keep` upkeep term, default `0mu`.
- **Attribution counters (§13.4):** per-gene counters of resources acquired or produced, written to the birth log at division and death, so `lineage.query` can say which genes paid for reproduction.
- **Labels (§12.1):** every gene and effect statement has a label in canonical form, and `emerge fmt` derives missing ones. Positional paths are removed from the language (`language.md` §10.1). The reference world and all examples are labelled.
- **GPU spike (§16):** now compares a slot loop over gene kinds with binning by gene-kind sequence, and tests division-free portable curves. §5.7 no longer fixes the execution method.
- **Open questions (§17):** portable curves restated (division-free, with the WGSL accuracy limit); continuous expression added, to be decided from birth-log evidence.

## Appendix D: what changed in v0.7.3

- **One schedule (§7):** the six-step pipeline is replaced by the core's four phases (`core.md` §6.1). Senses read the snapshot and minds run as substeps in Set; upkeep, fees, all transfers and movement happen in Move; reactions and venting in Transform; disposal, then division, in Restructure. **World transfer rules are now arbitrated together with entity transfers**, instead of running afterwards one rule at a time. World rules no longer see each other's effects within a tick; only reaction rules in one cell keep a declared order. Derived edges stay fixed from the snapshot for the whole tick. Division checks the inventory after Transform, and newborns first act in the next tick.
- **Reference world (§15.1):** rechecked under the new schedule. The light field stays flat, ungrazed cells settle at `199`–`200mu` of `body`, and a seed shares a cell's `body` pro-rata with diffusion, taking about 99% of it.
- **Attribution (§13.4):** counters are gross acquisition and production per gene since birth or the previous division, for chosen substances (default: the `divide` requirement). The claim that they show where the inventory used for division came from is removed.
- **Memory cost (§3.3):** `per_signal_keep` is described as a cost of passive retention; memory kept by recurrent genes remains possible and is paid by `per_gene`.
- **Catalysts (§4.5):** they speed up a reaction that is already possible; "must be present" is removed.
- **`core.md`:** the schedule (§6.1) now states the order within each phase, including `charge` laws, fees and paid sets. Undirected derived edges with a limit use mutual selection (§2.3).

## Appendix E: what changed in v0.7.4

Found by the CPU runner (`crates/emerge-ref`):

- **Request order (§7.2):** transfer requests have one canonical order, and remainder ties are broken by a Philox draw indexed by the request's position, then by position.
- **Philox keys (§10.2):** the exact mapping from `(seed, tick, stream, pass, index)` to a Philox key and counter. Entity and cell streams are now in separate domains; before, entity `5` and cell `5` could share a stream.
- **`max_age` (§7.5):** an entity acts in exactly `max_age` ticks.
- **Venting (§7.3):** dying entities are vented too.
- **Reference world (§15.1):** an ungrazed cell ends every tick at `199mu`, not "between `199` and `200mu`"; a seed sharing a cell with diffusion loses at most about 0.4%, and in a `200mu` cell takes all of it.
- **Editorial:** §5.5 drops intents "in Set", not "in Think".

## Appendix F: what changed in v0.7.5

Found by the CPU runner (`crates/emerge-ref`) and the parser (`crates/emerge-lang`):

- **Reference world recalibrated (§15, §15.1).** The v0.7.4 dish gave seeds four to eight times the energy they spent, so nothing starved, and the population grew until `max_age` or `max_entities` stopped it. Now `sunlight` adds `40mu` per cell per tick, cells hold at most `1u` of light, and `Builder`'s `build` runs at `50mu`. Energy limits the population (about `10_300` `Drifter`s or `7_000` `Builder`s), and life is short enough for many generations. §15.1 records the measurements.
- **Seeds turn both ways (§15).** `turn` reads a `tanh` signal, but with the default threshold `0.0` it fired only for positive values, so seeds never turned right. The seeds now use `threshold -2.0`, below any `tanh` value.
- **Reading units (§3.5):** `f32(amount) × 0.001f32`, with no division; a reading can be one ULP off.
- **Float × amount (§3.5):** computed in `f32`, with every amount involved at most `2^24 µ`.
- **Geometry (§6.1):** `cell_at`, wrapping, the `gradient` stencil (cell `y − 1` is north), and renormalizing the heading after a turn.
- **Repulsion (§7.2, §17):** none in P0; it moves to P1 as an open question.
- **Genesis randomness (§10.2):** pass `genesis = 5`, with fixed draws for position and heading.
- **Canonical spellings (§4.2, §4.5, §5.5):** reactions are written with terms in declaration order, as the compiler requires (`light + carbon -> sugar`), and zero fees as `0u`.

## Appendix G: what changed in v0.7.6

- **Baseline and control (§15.1, §16).** `emerge run` now compiles `examples/dish.em` itself. Over 10 seeds of 50_000 ticks with mutation off, `Drifter` survives with births in all 50 windows of 1_000 ticks, and `Builder` survives with body synthesis in all 50: exit tests 7 and 8 pass in 10 of 10 seeds. §15.1 records the populations, and replaces the 5_000-tick figures of v0.7.5, which were measured with `threshold -1.0`. Turnaround (tracked, §16): about 3.7 hours on a 32-core CPU for the 20 runs, with the runs limited by memory bandwidth, not cores.
- **`weigh` defaults (§5.3):** `bias: 0.0`, `curve: linear`, and weight `1.0` for an unweighted input.
- **Division shares (§7.5):** `split` becomes whole basis points when the genome is built, and the child's share is computed in integers.

## Appendix H: what changed in v0.7.7

Prompted by a review of the core and the runner:

- **What each step reads (§7, `core.md` §6.2).** One table states, for every step, what it reads of its own node and of everything else, what it writes, and where the write becomes visible, with the runner test that shows each row. The only values that change during a phase and are read again within it are signals between mind steps, balances during upkeep, fees and reactions (each in a fixed order), and an entity's own values in Restructure's conditions. `core.md` §4.2's "a rule never reads values written by another rule in the same phase" now names these exceptions.
- **Writes (§7.4, `core.md` §3).** A free value has one writer unless it declares how writes combine (`sum`, `max`, `min`, `any`, `all`); a second writer without one is `E0305`. An unwritten value keeps its value. The default `sum` is gone; signals keep their own rule (`keep × old + Σ contributions`). For creatures, a signal field written by several rules declares `signal combine ...`, and `emit` amounts are added after the rules' writes.
- **Restructuring (`core.md` §6.4).** Every graph change is atomic and applied in a fixed order against the graph the previous changes left. Deleting a node deletes its incident edges and routes their contents, with a fallback sink for what a full or deleted destination cannot take (in creatures: the cell, then `remains`, as before). Creating or splitting a node creates its required edges with it, or is rejected as a whole, moving nothing. Restructuring fees are attempt fees, not refunded on rejection, and rejections are reported with a reason.
- **Diagnostics (`language.md` §14).** `emerge check` reports problems with codes and positions; the codes in use are listed.

## Appendix I: what changed in v0.7.8

Found while implementing mutation and contact in the CPU runner, and in the first evolution runs:

- **Mutation, exactly (§8.1, §4.3).** Draw numbering (pass `mutate`, child's ID as stream, sequential draws), point mutations (one field per mutated gene, Gaussian steps scaled to the menu range, thresholds within `±16`, `keep` and expression bits), wiring, the order of growth operators, what an inserted gene looks like, which signals of a module are private, and the order of the reaction space with how a reaction ID moves through it. A lineage with mutation needs a profile.
- **Senses and contact (§5.5).** `probe`, `crowd`, `neighbour` and `clock` are defined; `crowd` counts only as far as `max_interaction_radius`. All contact genes of an entity act on its nearest neighbour. A dying target is not a transfer partner, and the attempt fee is kept.
- **Partial runs (§5.5).** A runner may leave out menu items it lacks, with warning `W1001`; genes cannot use them and mutation does not produce them.
- **Rounds [P1] (§7, `core.md` §6.5).** A world may let some lineages act several times per tick, each round with its own snapshot and arbitration, and may run world rules `every N ticks`.
- **Exit test 9 (§15.1, §16).** Contact changes the outcome: with predation, genomes grow and body synthesis appears in every seed, but without contact genomes shrink. Test 9 now counts entities that produce `body` (at least 2%), not entities that carry a gene for it (10%). Partly passed on 2026-10-19: 7 of 20 seeds in the recalibrated dish with `guard` (§15.1). Passed later that day: 13 of 20 seeds with `duplicate_gene` raised from 0.005 to 0.02 (§15.1).
- **Tooling.** `emerge run` reports stolen amounts, thieves, producers, and with `--census N` the most common genomes in `.em` syntax.

## Appendix J: what changed in v0.7.9

Recorded from the GPU runtime, the engine and the first renderer (2026-10-20); direction decided 2026-10-21:

- **Contract mode (§10.1, §10.2).** An opt-in set of float definitions (fused multiply-add in minds, the defined curves, polynomial `sincos`, a Newton-step heading, a twelve-uniform normal draw, integer differences, integer catalysts) under which the GPU runtime equals the CPU reference word for word on one driver. The default mode is unchanged. The default mode's `delta` wrap is recorded as a known deviation.
- **Catalysts (§4.5).** The contract-mode integer factor is defined exactly. A catalysed world-reaction rate is capped at `10_000bp` in both modes; before, a higher rate underflowed an amount. No archived dish catalyses a world reaction. Both approved by the user on 2026-10-21.
- **Views (§11).** Views and charts are checked (`E0114`, `E0204`, `E0205`, `W1201`); `emerge-view` renders still PNGs with exact cells. The live, VEX-like direction is recorded as decided, not built.
- **Engine (§12.2, §13).** The implemented API methods, local-safety rules and dual-era MCP adapter; the Phase 0 hot-swap subset with its fail-closed check and measured pauses; the journal as built, with its gaps.
- **Architecture (§14).** The crates as built.
- **Exit tests (§16).** Status with qualifiers: 1 to 11 met; test 3's statistical half is established against contract mode only.
- **Open questions (§17).** The portable-curves entry records that the switch became an opt-in mode.
