# Emergent Core Language

**Core specification — draft v0.1**

Status: first draft. This document defines the **core** of the language: what a program is made of and how it runs, independent of any application. Evolving creatures (`spec-v0.7.2.md`) become one application written in this core, using a standard library. Syntax here is provisional and will be merged into `language.md`.

Conventions:
- **MUST** / **MUST NOT** mark normative requirements.
- `µ` is a micro-unit of a conserved quantity; in source code it is spelled `mu`.

---

## 0. Decisions

| Topic | Decision |
| :-- | :-- |
| Substrate | A **graph**. A grid is one kind of graph |
| Rule owners | **Laws** belong to the universe and are its physics. **Owned rules** belong to a node and run on that node |
| Time | **Lockstep** ticks. A node may run rules that touch only itself several times within a tick (**substeps**) |
| Edges | Edges have kinds and may hold values, free or conserved. Derived edges are computed each tick from a condition (§2.3) |
| Creation | Only laws may create nodes from nothing. Owned rules create only by copying or splitting their own node |
| Rewriting rules | Both laws and a node's own rules may rewrite that node's owned rules |
| Copy and mutate | Built into the language, not assembled from lower-level operations |
| Applications | Evolving creatures is the first application, chosen because it is easy to see and judge. The core is judged by how small and general it is |

**Guiding principle: the language allows, a world restricts, the compiler enforces.** The core permits every combination that makes sense (values on edges, conserved quantities anywhere, rules that rewrite rules). A world declares what it uses and what it forbids, and the compiler holds the world to its own declarations.

---

## 1. Layers

| Layer | Contents |
| :-- | :-- |
| **Core** (this document) | Graph, kinds, values, rules, templates, time, invariants, rules as data |
| **Standard library** | Graph shapes (grids, rings, random graphs), fields, spatial "nearby" edges, chemistry (elements, substances, reactions, potentials), common templates |
| **Applications** | Programs written in the language. The first is evolving creatures: senses, actions, upkeep, division, mutation settings, the reference world |
| **Tooling** | Views, charts, `explore`, hot-swaps, the AI API. They work on any program |

The core must be expressive enough that the standard library and the creatures application need no special support from the engine. The engine may still **recognise** common patterns (a regular grid, a fixed-degree graph) and run them faster.

---

## 2. The universe is a graph

### 2.1 Nodes and edges

- A **node** is a place. It has a **kind**, a persistent ID, values, and optionally a set of **owned rules**.
- An **edge** connects two nodes. It has a kind, a persistent ID, a direction (or none) and values.
- A **kind** declares the values its nodes or edges hold, and, for edge kinds, which node kinds they connect and how many.

```
kind Cell {
    free alive: f32;
    conserved food;
}

kind Critter {
    conserved energy;
    owned rules (genes: 32, signals: 16, modules: 8);
}

edge neighbour : Cell -- Cell;                     // undirected
edge at : Critter -> Cell (exactly 1 per Critter);
edge bond : Critter -- Critter (at most 4 per Critter) {
    free strength: f32;
    conserved glue;                                // edges may hold conserved values
}
```

- `owned rules (...)` means nodes of this kind carry their own rule set, with the given capacity (§5).
- Edge cardinality (`exactly 1`, `at most 4`) is a constraint the compiler and runtime enforce (§7).

### 2.2 Building the initial graph

The initial graph is built in `genesis`, by laws, usually with standard library generators:

```
universe Dish {
    genesis {
        let cells = grid(Cell, w: 256, h: 256, edge: neighbour, wrap: true, pattern: moore);
        scatter(Critter, count: 2_000, on: cells, edge: at, rules: Seed);
    }
}
```

A generator is ordinary core code. The engine recognises the regular structure a `grid` produces and stores its edges implicitly.

### 2.3 Derived edges

A **derived edge** kind is defined by a condition on the nodes it connects, not built by rules. The engine recomputes it from the snapshot at the start of every tick:

```
derived edge nearby : Critter -- Critter
    where distance(a.pos, b.pos, wrap: 512.0) <= 4.0
    (at most 8 per Critter, nearest first);
```

- To rules, a derived edge reads like any other edge: `sum(nearby.body)`, `move sugar * 5% along nearby`.
- The engine does not store it as a list of edges. It answers queries from an index, such as a spatial hash for a distance condition, so a world of moving nodes does not rebuild its graph every tick.
- **Limits.** A cardinality limit MUST say which edges are kept (`nearest first`, by a value, or by lowest ID). Ties are broken by persistent ID, so the result is deterministic.
- **What it cannot do.** A derived edge holds no values, because it does not persist from one tick to the next. Rules MUST NOT `link`, `unlink` or `rewire` it. Restructuring changes it only by changing the values its condition reads.
- The condition is a pure expression over the two nodes' values, and the compiler must be able to answer it from an index: a distance bound, equal values of a key, or the same node at the end of another edge (`a.at == b.at`). Other conditions are a compile error, because they would need every pair of nodes to be checked.

---

## 3. Values

| Kind of value | Meaning | Stored as |
| :-- | :-- | :-- |
| **Free** | Information: computed, copied, overwritten. `f32`, `int`, `bool` | as declared |
| **Conserved** | A quantity that can only be moved or transformed, never created or destroyed, except at the universe's boundary (§7.1) | `u32` µ, with a cap per slot |

- **Base quantities** are declared once per universe: `quantity C; quantity N; quantity E;`. Each has its own ledger.
- **Compound quantities** are declared as compositions of base quantities: `compound sugar = C + E;`. A conserved slot holds either kind.
- A slot's **cap** is declared with the slot (`conserved food cap 2_000u;`), at most `2^31 µ`.
- **Free values written by several rules** in one phase are combined by the slot's declared **combine** operator: `sum` (default), `max`, `min` or `any` (for `bool`). The order of combination is fixed (§6.3), so results are deterministic.
- **Dynamic free values.** A node kind with owned rules has a growable table of private free values (its **signals**), which its owned rules create and use (§5.3).

Units and the numeric contract (saturating integers, defined rounding, sanitised floats) carry over from spec v0.7.2 §3.5 and §10.1.

---

## 4. Rules

### 4.1 Laws and owned rules

| | **Law** | **Owned rule** |
| :-- | :-- | :-- |
| Belongs to | the universe | one node |
| Runs on | every node, edge or pattern it matches | its own node |
| Written as | free-form code | an instance of a **template** (§5), so mutation can change it |
| Changed by | the user or an AI, through a hot-swap | the node's own rules, laws, and mutation |
| May reach the boundary | yes: draw from sources, release to sinks, create nodes from nothing | no |

### 4.2 What a rule reads

Every rule reads the **snapshot** taken at the start of the tick (or at the start of the substep, §6.2):

- the values of the node or edge it runs on;
- along edges: the values of incident edges and of the nodes at their other end, following edge kinds, up to a depth the rule declares (default `1`);
- reductions over what it reaches: `sum`, `min`, `max`, `mean`, `count`, `argmin`, `argmax`;
- the tick number.

A rule never reads values written by another rule in the same phase.

### 4.3 What a rule does

There are exactly five kinds of effect:

| Effect | Example | Phase (§6) |
| :-- | :-- | :-- |
| **Set** a free value | `alive = 1.0;` | 1 |
| **Move** a conserved quantity along an edge, or between a node and an edge | `move food * 10% along neighbour;` | 2 |
| **Transform** conserved quantities inside one node or edge | `transform carbon + light -> sugar, rate: 1u;` | 3 |
| **Restructure** the graph: create or delete nodes and edges, rewire an edge's end | `rewire at to: argmax(at.neighbour.food);` | 4 |
| **Rewrite rules**: copy rule sets into new nodes, mutate a node's owned rules | `create Critter copying self with mutate(rules, Drift);` | 4 |

Limits on owned rules:
- They may **set** their own node's free values and their own signals.
- They may **move** conserved quantities only along edges incident to their own node, between their node and those edges or the nodes at the other end. Every move is a request, settled by arbitration.
- They may **transform** only inside their own node.
- They may **restructure** only their own edges (link, unlink, rewire), **delete** their own node, and **create** a node only by copying or splitting their own node (§5.4).
- They may **rewrite** only their own node's rules, and the rules of a node they create.

Laws have none of these limits, but all rules obey the invariants (§7).

### 4.4 Examples

```
law life on Cell {
    let n = sum(neighbour.alive);
    alive = select(when: n == 3.0 or (alive > 0.5 and n == 2.0), then: 1.0, else: 0.0);
}

law spread on Cell {
    move food * 3% along neighbour;            // split evenly between neighbours
}

law weather on Cell {
    move food 10mu from: rock;                 // only laws may draw from a source
}

law decay on edge bond {
    strength = strength * 0.99;
    when strength < 0.01 {
        unlink self;                           // weak bonds break
    }
}
```

---

## 5. Templates and rules as data

### 5.1 Templates

Owned rules are always **instances of templates**. A template is a rule with **holes**: numbers, choices and signal connections that an instance fills in. Mutation changes the filled-in values, never the template's code. Small changes to numbers cause small changes in behaviour, which keeps evolution workable.

```
template weigh(bias: -16.0..16.0, curve: [linear, step, sigmoid, tanh])
    reads inputs x[1..4] weight w[-16.0..16.0]
    writes signal out
{
    out = curve(bias + sum(w * x));
}

template forage(speed: 0.0..2.0)
    reads signal go
{
    rewire at to: argmax(at.neighbour.food) when go > 0.5;
}
```

- A template declares its **parameters** (with ranges or choice lists), its **reads** (signals, with optional weights) and its **writes**.
- The body is ordinary rule code, under the owned-rule limits of §4.3.
- A universe declares which templates its nodes may use, as a **menu** per node kind: `Critter uses [weigh, forage, eat, split];`. Mutation can only produce instances of templates on the menu.
- The standard library provides common templates. Applications define their own. The engine has no built-in templates.

### 5.2 Rule sets and modules

A node's owned rules are a **rule set**: an ordered list of **modules**, each an ordered list of **instances**. Each instance and each module has an `expressed` bit. Inactive instances are kept, inherited and mutated, but do nothing.

```
rules Seed for Critter {
    module forage {
        instance weigh(curve: tanh) <- [10.0 * food_left] -> turn;
        instance forage(speed: 1.0) <- go;
    }
    module reproduce {
        instance weigh(bias: -10.0, curve: step) <- [energy_level] -> ready;
        instance split(share: 50%) <- ready;
    }
}
```

### 5.3 Signals

Signals are the node's private, growable table of free values that its owned rules read and write. Each has an evolvable **keep** factor (how much survives each step), and is private to a module or part of its interface depending on which instances use it. These are the internal signals of spec v0.7.2 §5.1, now a core feature of any node kind that owns rules.

### 5.4 Built-in operations on rules

| Operation | Meaning | Who may use it |
| :-- | :-- | :-- |
| `create K copying self with R` | Create a node of kind `K` whose rule set is `R` | an owned rule, for its own node |
| `split self with R` | As `copying`, and divide the node's conserved values between the two, as the instance says | an owned rule, for its own node |
| `create K with rules S` | Create a node with the named rule set `S` | laws only |
| `mutate(rules, M)` | The node's rules, changed by mutation settings `M` | inside the above, or as a rule effect on the node's own rules, or by a law on any node |
| `copy(rules)` | The node's rules, unchanged | as `mutate` |

**Mutation settings** are a declared value listing operator rates. The operators are built in: point changes to parameters and choices, rewiring of inputs and outputs, expression flips, and growth (duplicate, insert, delete instances; duplicate, delete, split, merge modules), as in spec v0.7.2 §8.1. New material starts unexpressed.

```
mutation Drift {
    rate: 0.02;
    param_sigma: 0.1;
    expression_flip: 0.01;
    duplicate_instance: 0.005;
    duplicate_module: 0.002;
}
```

The compiler guarantees that mutation only ever produces valid rule sets: instances of menu templates, parameters in range, within capacity.

---

## 6. Time

### 6.1 Ticks and phases

The universe advances in lockstep **ticks**. Every rule, law or owned, reads the snapshot and contributes effects. Effects are applied in four phases:

1. **Set:** all free-value writes are combined (§3) and applied.
2. **Move:** all move requests, from laws and owned rules alike, are arbitrated together in two stages (by origin, then by destination capacity) and applied, as in spec v0.7.2 §7.2.
3. **Transform:** in each node and edge, transforms run in a fixed order (laws in declaration order, then owned instances in rule-set order), each limited by its inputs and by output capacity.
4. **Restructure and rewrite:** graph changes and rule rewrites are applied, with conflicts settled as in §6.3. New nodes join the graph at the end of the tick.

Because everything in a phase reads the same snapshot and is settled together, the order in which rules are declared does not change what moves where. The only exception is the declared order within one node's transforms.

### 6.2 Substeps

A node kind may run some of its owned rules several times per tick:

```
kind Critter {
    conserved energy;
    owned rules (genes: 32, signals: 16, modules: 8) substeps 3;
}
```

Only **internal** instances take part in substeps: those that read and write nothing but their own node's free values and signals. The compiler checks this for every template. Internal instances cannot conflict with anything outside the node, so substeps need no arbitration and stay deterministic. All other rules run once per tick, after the last substep, and read its result.

### 6.3 Conflicts in restructuring

Some graph changes compete: two rules relinking to a node that accepts only one more edge, a rule rewiring an edge another rule deletes. They are settled in a fixed order:

1. deletions before other changes;
2. laws before owned rules;
3. then by the persistent ID of the rule's node or edge, with ties broken by Philox keyed by the contested element.

A change that would violate a cardinality constraint after that order is applied is dropped.

---

## 7. Invariants

### 7.1 Conservation

For every base quantity, after every tick:

```
Σ over nodes and edges (amount, counting compounds by composition) − emitted + absorbed == 0
```

- **Boundaries.** Sources emit and sinks absorb. Only laws may use them.
- **Transforms balance.** A transform MUST conserve every base quantity, and is put into canonical form before use (spec v0.7.2 §4.4). A transform may name a **catalyst** that raises its rate without being consumed (spec v0.7.2 §4.5). The catalyst is outside the balance.
- **Deletion needs a route.** When a node or edge is deleted, its conserved contents MUST go somewhere: a law-declared route (`on delete Critter { move all to: at; }`) or a sink. The compiler checks that every conserved slot of every deletable kind has a route.
- **Splitting divides, never copies.** A `split` divides conserved contents between parent and child. `copying` gives the child nothing; anything it starts with must be moved to it.

### 7.2 Measures that never increase

A universe may declare a **measure**, a weighted sum of conserved quantities, that no transform may increase:

```
measure potential = 10 * light + 8 * energy + 9 * sugar + 5 * body;
never increases potential by less than 1;
```

This is the dissipation law of spec v0.7.2 §3.2, now a general mechanism. A world may declare several measures or none.

### 7.3 Declared restrictions

Everything a world declares, the compiler enforces:
- the value slots of each kind, and their caps;
- edge kinds, which node kinds they connect, and their cardinality;
- which templates each node kind may use, and the owned-rule capacity;
- which quantities are conserved, and which measures never increase.

A world that declares no edge values gets no edge values; a world that declares none of its node kinds with `owned rules` has no evolution. The language allows; the world restricts.

---

## 8. What moves out of the core

| Today (spec v0.7.2) | Becomes |
| :-- | :-- |
| Fields | Standard library: a grid of nodes with values |
| Continuous 2D space and positions | Standard library: node values for position, plus a derived `nearby` edge kind (§2.3) |
| Elements, substances, reactions, potentials | Standard library chemistry, built on base and compound quantities, transforms and measures |
| Senses, actions, regulation kinds | Templates in the standard library and in the creatures application |
| Upkeep | A law in the creatures application |
| Division, mutation settings | `split` with `mutate` in an owned rule; a `mutation` declaration |
| Entities, lineages, seed genomes | Node kinds with owned rules, and named rule sets |
| Arbitration, numeric contract, determinism, journal, hot-swaps | Stay as they are, now stated for any rule |

---

## 9. Open questions

- **Performance of general graphs.** Arbitrary rewiring and creation are hard to run fast on a GPU. The engine will need fast paths for regular graphs and for kinds whose structure rarely changes. The 100k-entity target should be re-examined once a first engine exists.
- **Read depth.** How far along edges a rule may read before matching becomes too costly, and whether laws may match general **subgraph patterns** (for example "two Critters joined by a bond, both on the same Cell") or only nodes and edges with a fixed read depth. Kappa, a rule-based language for rewriting site graphs, is the closest prior art for pattern-matching laws and for how they can be run efficiently.
- **Moves between distant nodes.** Moves currently follow one edge. Multi-edge moves (through a pipe of several edges) may be needed, or may be better built from repeated single moves.
- **Reading another node's signals.** Owned rules read node values along edges (§4.2), but signals are private to their node (§5.3). Bonded nodes that act as one body, such as a leaf cell and a root cell linked by a `bond`, may need to read each other's interface signals. Whether to allow that along declared edge kinds, and what it costs, is open. Substances already flow between bonded nodes through laws (`move sugar * 20% along bond`), so multicellular bodies need no special composite object.
- **Layout for views.** A general graph has no positions. Views need either position values on nodes or a layout algorithm. Grids and spatial worlds have natural positions.
- **Author-defined templates and the GPU.** Templates compile to code, so a universe with many templates produces many code paths. Binning by template sequence may be enough.
- **Syntax.** The examples here are provisional. Once the core settles, `language.md` will be rewritten around it.
