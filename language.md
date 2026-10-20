# The `.em` Language

**Language reference — draft, companion to spec v0.7.8**

This document defines the syntax and static rules of `.em` files. `spec-v0.7.8.md` defines what the constructs mean at runtime (tick pipeline, arbitration, evolution). This document is normative for syntax; the spec is normative for behaviour. `examples/dish.em` is the reference program and the parser fixture (§13).

> **Being restructured.** `core.md` now defines the core language. This document's syntax will be rewritten around it; until then, `core.md`'s examples are provisional and this document remains the syntax reference for the creatures application.

Conventions:
- **MUST** / **MUST NOT** mark normative requirements.
- Grammar fragments use the notation in §11.
- `µ` is a micro-unit; in source code it is spelled `mu`.

---

## 1. Design goals

1. **One spelling per meaning.** `emerge fmt` output is the only accepted form. There are no synonyms, optional separators or alternative orders.
2. **Laws in the type system.** Conservation, dissipation and the separation between evolution and world rules are checked at compile time wherever possible (§4, §5).
3. **Small contexts.** A file is a set of declarations plus a few small inner languages, each with a fixed list of what it may read and do (§5).
4. **Patchable.** Every item has a readable path and a persistent ID, and every edit can be expressed as a patch at a stated revision (§10).
5. **Compiles to straight-line code.** No user-written loops or recursion in anything that runs every tick, so world rules and minds lower directly to WGSL.

---

## 2. Lexical structure

| Element | Form | Examples |
| :-- | :-- | :-- |
| Comment | `//` to end of line. No block comments | `// rot returns nutrients` |
| Identifier | `[A-Za-z_][A-Za-z0-9_]*`, not a hard keyword | `photosynthesis`, `Dish`, `max_age` |
| Integer | digits with `_` separators | `20_000`, `3` |
| Float | digits, `.`, digits, optional exponent; or digits with an exponent | `0.5`, `1e-3`, `30.0` |
| Amount | integer or float followed by `u` or `mu` | `5u`, `250mu`, `0.5u` |
| Rate | integer followed by `bp` | `50bp`, `10_000bp` |
| Range | `bound .. bound`, inclusive; a bound is an optionally negative integer, float or amount | `0..512`, `-30.0..30.0`, `0u..10u` |
| Label | `@` identifier | `@eat_light` |
| Capture | `$` identifier (patterns only) | `$a`, `$c` |
| Arrows | `->` (flows out) and `<-` (flows in) | `-> steer`, `<- steer` |

Arrows are single tokens, read by longest match: `a<-1.0` is `a <- 1.0`, so a comparison with a negative number needs a space (`a < -1.0`). The formatter always writes one.

Negative numbers outside ranges are written with unary minus (`-1.0`), which the formatter keeps attached to the literal. Unary minus applies only to `int` and `f32`: `-5u` and `-50bp` are type errors (`E0203`), because amounts and rates are never negative.

**Rules for literals:**
- An amount MUST be a whole number of µ: `0.5u` is `500mu` and allowed; `0.0005u` is an error (`E0101`).
- `emerge fmt` prints amounts in `u` when they are whole units (including `0u`) and in `mu` otherwise, and groups digits in threes in every number of four digits or more (`2_000`, `20_000`), including the integer part of a float (`5_000.0`).
- Floats MUST contain a `.` or an exponent. Where an `f32` is expected, an integer is an error (`E0104`), so there is one spelling.

**Separators:**
- Every statement, config entry and declaration that does not end in a block ends with `;`.
- Commas only separate items inside `( )` and `[ ]`.
- Config blocks and encoding blocks always use `key: value;`.

### 2.2 Formatting

`emerge fmt` is fully determined by these rules, so the canonical text of a program is unique:

- **Indentation:** 4 spaces per level. No tabs.
- **Spacing:** exactly one space around binary operators, `=`, `->` and `|>`, after `,`, and after the `:` of a config key, a named argument or a patch position (`add after rules.weather: ...`); none before them. The `:` between a declaration name and its type or formula (`field cells : Grid(...)`, `reaction rot : body -> ...`, `rule wrap weight 0.5 : ...`) has one space on each side. No space inside brackets.
- **No column alignment.** Consecutive declarations are never padded to line up. This keeps diffs small when a name changes length.
- **Line width:** 100 columns. If a line is longer, the formatter breaks one bracketed group (`(...)` or `[...]`) on it: among the **outermost** groups on that line (those not inside another group on the line), the one with the **most characters** inside, the first one on a tie. Each item goes on its own line, indented one level deeper, and the closing bracket goes on its own line at the original indentation. It repeats until every line fits or no group is left to break. There is no trailing comma.
- **Blank lines:** preserved between declarations and statements, collapsed to at most one. None at the start or end of a block.
- **Blocks:** every block opens on its header line and closes on its own line, with one item per line in between. The exception is a config block (`second_law`, `reaction_space`, `upkeep`, `limits`) with exactly one entry and no comments, which is written on one line if it fits: `second_law { min_drop: 1; }`.
- **Comments:** kept in place. A trailing comment has one space before `//`.
- **Order:** declarations in a world are sorted into the order of §6 (§6, first paragraph).

### 2.1 Keywords

Keywords come in two kinds.

**Hard keywords** are reserved everywhere and can never be identifiers:

```
self true false and or not
```

**Contextual keywords** are words that only have a special meaning in a particular position, such as the start of a declaration or statement. Everywhere else they are ordinary identifiers, so they can be argument names (`to:`, `where:`), config keys (`genes:`, `senses:`) or path segments (`Dish.rules`):

```
world lineage extends ruleset view chart explore patch const
element substance potential reaction field holds signal combine source sink waste
senses sense actions action cost per requires
rule on let when else for in genesis
profile mutation seed genome module regulation regulate gene keep inputs weight
dormant threshold rate where
layer vary base set add remove replace rename after before to move_to
any act reg
```

A **declared name** (of a world, substance, reaction, rule, field, lineage, `let` binding, label and so on) MUST NOT be any keyword of either kind (`E0110`), and MUST NOT be a built-in function name (§12) or a regulation kind (§8.5). This keeps every name unambiguous wherever it is used.

---

## 3. File structure

A file is a list of top-level items. Order between items does not matter; names are resolved globally.

```
world Dish { ... } // physics, chemistry, rules, menus
ruleset Tinker { ... } // genome rewrite rules
lineage Drifter in Dish { ... } // a seed genome and its mutation settings
view Main for Dish { ... } // rendering
chart Health for Dish { ... } // plots
explore Atlas { base: Dish; ... } // parameter sweeps
patch Dish base 41 { ... } // edits, only in patch files and the API
```

**Names.** Top-level items share one namespace. Inside a world, each kind of declaration (substances, reactions, rules, fields, sources and sinks) has its own namespace, matching its path segment (`Dish.reactions.rot`, `Dish.rules.rotting`). Even so, a name MUST be unique across all kinds within one world (`E0111`), so a bare name in an expression always means one thing.

**Constants.** `const NAME = expr;` in a world names a value computed from literals and other constants, in any order of declaration:

```
world Dish {
    const side = 2 * 128;
    const lifetime = 25_000 - 5_000;
    field cells : Grid(w: side, h: side) holds [carbon: 2_000u];
    max_age: lifetime;
}
```

Before checking, every use of a constant (in the world, or in a lineage of it) is replaced by its value, and arithmetic on literals is folded, using the rules of §4.1 and §4.3 (`10_000 + 10_000` is `20_000` everywhere an expression is allowed, with or without a constant). A combination §4.1 does not list, such as `1u + 2`, is left as written, and is an error in a `const` (`E0205`). Constants that wait on each other in a circle are `E0119`; one that depends on something that is not a constant is reported at that constant (`E0205`). A constant shares the world's namespace, so it cannot repeat the name of a substance, reaction or other declaration, of an element, or of a top-level item, and no `let` or `for` variable may use its name (`E0111`). It cannot be a keyword, a built-in function or curve, or a word such as `self`, `tick` or `below` (`E0110`). A constant cannot read the world, a tick or a random draw. Positions that take a literal token cannot use a constant: `potential`, `holds` amounts, menu ranges and costs, a reaction gene's `rate`, and `uniform(x: a..b)` ranges. `emerge fmt` keeps constants and expressions as written.

`emerge expand FILE` prints the file as the checker and runner see it: lineages that `extends` another written out (§8.1) and constants replaced by their values.

`emerge order FILE [WORLD]` prints the steps of one tick (spec §7.1 to §7.5) with each world rule placed in the phase its effect gives it: signal rules in Set, transfer rules (with source inflows and diffusion) in Move, reaction rules in Transform. A rule that mixes kinds, or has none, is listed as not placed. A test keeps the step names equal to the spec's.

A **project** is a directory. `emerge` loads every `.em` file in it as one program. Persistent IDs live in `.emerge/ids.json` in the same directory and are kept in version control with the sources (§10.2).

---

## 4. Types

### 4.1 Value types

| Type | Meaning | Literals | Operations |
| :-- | :-- | :-- | :-- |
| `amount[s]` | `u32` µ of substance `s` | `5u`, `250mu` | `+` `-` with the same `s`; `* rate`; `/ int`; comparison |
| `rate` | `u32` basis points | `50bp` | `amount * rate`; as an argument |
| `int` | `i32` | `3`, `20_000` | `+ - * /`, comparison |
| `f32` | float | `0.5`, `1e-3` | `+ - * /`, comparison, math built-ins |
| `bool` | truth value | `true`, `false` | `and`, `or`, `not` |
| `list[T]` | a list of values | `[sugar, carbon]` | as an argument |
| `vec2` | a 2D position | — (from `uniform`) | assignment to `position` |

Potentials are `i32` but appear only in `substance` declarations.

**Amounts are typed by substance.** This catches mixing substances at compile time:

```
self.carbon + self.carbon // ok: amount[carbon]
self.carbon + self.light // E0201: different substances
self.carbon > 0u // ok: bool (0u fits any substance)
self.carbon * 50bp // ok: amount[carbon], floored
// E0202: amount[light] where amount[carbon] is expected
transfer(carbon, from: self, to: below, amount: self.light);
```

**No implicit conversions.** Conversions are built-in functions (§12):

| Function | From → to |
| :-- | :-- |
| `units(a)` | `amount[s]` → `f32` in units |
| `amount(s, units: x)` | `f32` → `amount[s]` |
| `float(i)` | `int` → `f32` |

### 4.2 Reference types

These name things declared in a world. They appear as parameters of menus, genes, rules and views.

| Type | Values | Example |
| :-- | :-- | :-- |
| `substance` | a declared substance | `carbon` |
| `reaction` | a declared or generated reaction | `build` |
| `field` | a declared field | `cells`, `scent` |
| `quantity` | a substance in a field, or a signal field | `cells.light`, `scent` |
| `signal` | a signal field | `scent` |
| `source`, `sink` | a declared boundary | `rock`, `void` |
| `store` | a cell in a rule (`self`, `below`, a `for` variable) or, in genesis, the new entity (`self`) | `self` |
| `neighbour` | an entity selected by the runtime | used only in action menus |

In a menu, `any T` means "a gene may choose any value of type `T`", and `[a, b]` restricts the choice: `take(s: [carbon, light, body], max: 0u..10u)`.

### 4.3 Arithmetic

All arithmetic follows the numeric contract in spec §10.1: amount and integer arithmetic saturates, division by zero gives `0`, and NaN or infinite floats become `0.0`. In particular `a - b` on amounts never goes below `0mu`. There is no separate saturating operator.

---

## 5. Contexts

Each block of code belongs to exactly one context. A context fixes what code may read and what it may do. The type checker rejects anything outside the context with `E0301`, naming the context.

| Context | Appears in | May read | May do |
| :-- | :-- | :-- | :-- |
| **Declaration** | `world` top level, config blocks, `lineage` settings | constants | declare |
| **Transfer rule** | `rule ... on field { }` containing transfers | the current cell, its neighbourhoods, other fields at the same place, `tick` | `transfer`, `diffuse_conserved` |
| **Reaction rule** | `rule ... on field { }` containing reactions | as transfer rule | `react` |
| **Signal rule** | `rule ... on field { }` containing signal assignments | as transfer rule | assign signals of the rule's field |
| **Genesis** | `genesis { }` in a world or lineage | as transfer rule, plus `uniform` | `transfer` from sources; set a new entity's `position` and `heading`; `count:` in a lineage |
| **Requirement** | `requires` in an action menu | the entity's own inventory, by substance name | — (a `bool` expression) |
| **Genome** | `seed genome { }` | its own signals, by name | declare genes, modules and signals; no expressions beyond constants and weights |
| **Pattern** | `ruleset { }` | the genome being rewritten | produce a replacement |
| **View** | `view`, `chart` | any buffer or metric | produce pixels or plot series |
| **Explore** | `explore { }` | paths into the base world | vary them |

The most important consequences:

- **Substances move only through `transfer`, `react` and `diffuse_conserved`** in world rules and genesis, or through action intents chosen by genomes. There is no assignment to an amount anywhere: `self.carbon = 5u` is `E0302`. Signals can be assigned, because they are not conserved.
- **A world rule holds one kind of effect.** The kind is inferred from its effect statements. A rule that mixes kinds is `E0303`; split it into consecutive rules. A rule with no effects is `E0304`.
- **Genomes have no effects of their own.** Genes only read and write signals; the runtime turns action genes into intents and runs reaction genes under the laws.
- **Only genesis may draw from a source without a rule**, and only at time zero or in `entity.inject`.
- **View code cannot write simulation state** or use the simulation's random streams.

---

## 6. World declarations

A world body holds declarations and rules. `emerge fmt` sorts declarations into the order of this section, with world rules (§7) after the menus and before genesis. It keeps the relative order of substances, reactions and rules, because substance order defines canonical reactions and the order of reaction rules is execution order.

### 6.1 Space

```
space: Continuous2D(w: 512.0, h: 512.0, wrap: true, max_interaction_radius: 4.0);
```

### 6.2 Elements, dissipation and substances

```
element C;
element N;
element E;

second_law { min_drop: 1; }

substance light = E potential 10;
substance sugar = C + E potential 9;
substance ozone = 3 O potential 12; // coefficient before the element
```

One element per `element` declaration. Every substance MUST declare a potential (`E0403`).

### 6.3 Fields

```
field cells : Grid(w: 256, h: 256) holds [light: 2_000u, sugar: 2_000u, carbon: 2_000u];
field scent : Grid(w: 256, h: 256) signal;
```

A substance field lists each substance with its cap. A signal field holds one `f32` per cell, read and written as `self.value`. A field cannot hold both.

A signal field written by more than one rule MUST declare how their writes combine, `field scent : Grid(w: 256, h: 256) signal combine max;`, with `sum`, `max` or `min` (`E0305`). With one writer, no declaration is needed. A cell no rule writes in a tick keeps its value (`core.md` §3).

### 6.4 Boundaries, waste and remains

```
source sun : [light];
source rock : [carbon, nitrate, body];
sink void : any;
sink vent : [heat];

waste heat -> void; // vented from every inventory after Transform
remains: void; // where a dead entity's unplaceable substances go
decompose: rot; // optional: a declared reaction run to completion on a dead entity's inventory before disposal
```

### 6.5 Reactions and the reaction space

```
reaction respiration : sugar -> energy + carbon;
reaction burn : 2 sugar + oxygen -> 2 carbon + 2 energy + heat;

reaction_space {
    include: [photosynthesis, respiration, build];
    generate: balanced(max_inputs: 3, max_outputs: 3, max_coeff: 2);
    max_rate: 1u;
    generated_max_rate: 250mu;
    exclude: [light -> energy];
}
```

- `generate: none;` disables generation. Formulas inside `exclude` use the reaction syntax without a name.
- **Catalysts** [P1] follow the formula: `reaction build : energy + carbon + nitrate -> heat + body catalyze iron(half: 100mu, boost: 10.0);`. The catalyst is not part of the formula, its canonical form or its `ΔG` (spec §4.5).
- A hand-written reaction MUST be in canonical form (spec §4.4): no substance on both sides, no repeated terms, coefficients without a common factor, terms in substance declaration order. Otherwise it is `E0404`, with the canonical form as a suggested fix. `emerge fmt` rewrites a reaction into canonical form (it leaves a formula that names an undeclared substance as written), so the fix is to run it.
- The compiler reports, for each reaction, its element balance, canonical form and `ΔG`, and rejects any reaction that is unbalanced (`E0401`) or not downhill by at least `min_drop` (`E0402`).

### 6.6 Upkeep, limits and settings

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

limits {
    max_entities: 200_000;
    max_reaction_space: 65_536;
    inventory_cap: 100u;
}

max_age: 20_000;
child_offset: 0.5;
mind_steps: 1;
```

`child_offset` is the distance, in space units, behind its parent at which a child is placed (spec §7.5). It defaults to `0.5`. `mind_steps` is how many times per tick every mind updates its signals (spec §5.4). It defaults to `1`.

[P1] `rounds: 3;` sets how many rounds a tick has (default `1`, at most `16`); a profile's `rounds: k;` sets how many of them its lineage acts in, and `per_round` in `upkeep` charges for each round an entity acts in (spec §7, `core.md` §6.5). A world rule may add `rounds k` or `every N ticks` after its field: `rule seasons on cells every 100 ticks { … }`. The parser does not accept these yet.

`per_signal_keep` (optional, default `0mu`) adds `ceil(per_signal_keep × keep)` for each signal in use: a cost of passive retention. Memory kept by recurrent genes is not charged by it (spec §3.3).

`pay` and `becomes` MUST have the same composition, `pay → becomes` MUST be downhill, `vent` MUST accept `becomes` (`E0701`), and `base` MUST be greater than `0mu` (`E0702`).

### 6.7 Menus

```
senses {
    sense probe(q: any quantity, angle_deg: -180.0..180.0, dist: 0.0..8.0);
    sense gradient(q: any quantity); // .x forward, .y left
    sense inventory(s: any substance);
    sense crowd(radius: 0.0..6.0);
    sense neighbour(s: any substance, rank: 0..3);
    sense clock(period: 1..1_000);
}

actions {
    action move(speed: 0.0..2.0) cost 20mu per distance;
    action turn(angle_deg: 0.0..30.0) cost 10mu;
    action take(s: any substance, max: 0u..10u) cost 5mu;
    action drop(s: any substance, max: 0u..10u) cost 0u;
    action give(s: any substance, max: 0u..10u, to: neighbour) cost 5mu;
    action emit(sig: scent, max: 0.0..10.0) cost 2mu;
    action divide(split: 0.0..1.0) cost 0u requires body >= 10u;
}
```

The regulation menu lists the kinds of regulation gene genomes may use (spec §5.3, §8.5 here), with their parameter ranges, how many inputs they take and, for weighted kinds, the range of input weights:

```
regulation {
    gene weigh(
        bias: -16.0..16.0,
        curve: [linear, step, sigmoid, tanh]
    ) inputs 1..4 weight -16.0..16.0;
    gene product() inputs 2..2;
}
```

- Sense, action and regulation kind names are fixed by the engine; a menu chooses which ones exist in the world and narrows their parameter ranges. An unknown name is `E0501`.
- Every action states its fee, including `0u`. Fees are paid in the upkeep `pay` substance (spec §3.3), so the fee has no substance name.
- `per distance` is the only fee modifier in P0.
- `action guard(s: any substance) cost 20mu;` (experimental) takes no range: while it fires, `steal`s of `s` against the entity fail (spec §5.5).
- `requires` is an expression in the Requirement context (§5).

### 6.8 Genesis

```
genesis {
    for cell in cells {
        transfer(carbon, from: rock, to: cell, amount: 50u) @transfer_carbon;
    }
}
```

`for ... in field` is the only loop in the language and is allowed only in genesis.

---

## 7. World rules

```
rule NAME on FIELD { statements }
```

A rule runs once per cell of `FIELD`, every tick. Its kind decides the phase it runs in (spec §7.4): signal rules in Set, transfer rules in Move, reaction rules in Transform. Every rule reads the start-of-tick snapshot for its conditions and signal values, so rules do not see each other's effects within a tick.

### 7.1 Reading

| Expression | Meaning |
| :-- | :-- |
| `self.carbon`, `self.value` | the current cell's amount, or its signal value |
| `above.carbon`, `below`, `left`, `right` | one neighbouring cell (wrapping if the space wraps) |
| `moore.value`, `vonneumann.carbon` | the values in the 8 or 4 neighbouring cells, as a collection |
| `sum(moore.value)`, `min(...)`, `max(...)`, `mean(...)` | reductions over a collection |
| `count(moore.value > 0.5)` | number of neighbours where a condition holds, as `int` |
| `argmin(moore.height)`, `argmax(...)` | the neighbouring cell with the smallest or largest value; ties go to the first in the fixed order N, E, S, W, NE, SE, SW, NW |
| `scent.here` | another field at the same place (bilinear for signals, nearest cell for substances) |
| `cap(carbon)` | the rule's field cap for `carbon` |
| `tick` | the current tick, as `int` |

All reads in a rule see the snapshot: the state at the start of the tick.

### 7.2 The three kinds of rule

**Transfer rules** move substances. All transfers of all cells and all rules, and all entity transfers, are arbitrated together (spec §7.2), so the order of statements and of transfer rules does not matter.

```
rule weather on cells {
    transfer(carbon, from: rock, to: self, amount: 20mu, overflow: void) @transfer_carbon;
    when self.grain > 0u and below.grain < cap(grain) {
        transfer(grain, from: self, to: below, amount: 1u) @transfer_grain;
    }
}

rule spread on cells {
    diffuse_conserved([sugar, carbon, nitrate], rate: 300bp) @diffuse_conserved;
}
```

**Reaction rules** run reactions, after all transfers. In each cell they run in declaration order of rules and then of statements, each limited by inputs and output headroom.

```
rule rotting on cells {
    react(rot, rate: 50bp) @react_rot;
}
```

**Signal rules** assign the field's signal. Every cell reads values from the snapshot.

```
rule smell on scent {
    self.value = fade(blur(scent, amount: 0.2), keep: 0.9);
}

rule life on board {
    let n = sum(moore.value);
    self.value = select(when: n == 3.0 or (self.value > 0.5 and n == 2.0), then: 1.0, else: 0.0);
}
```

### 7.3 Statements

| Statement | Allowed in |
| :-- | :-- |
| `let name = expr;` | every rule and genesis |
| `when cond { ... } else { ... }` | every rule and genesis; the branches hold statements of the rule's kind |
| `transfer(...)`, `diffuse_conserved(...)` | transfer rules, genesis (`transfer` only) |
| `react(...)` | reaction rules |
| `self.value = expr;` | signal rules |
| `for cell in field { ... }` | world genesis |

Every effect statement (a `transfer`, `diffuse_conserved` or `react` call) has a label in canonical form (`react(rot, rate: 50bp) @slow_rot;`), so patches can address it by name. `emerge fmt` adds one if it is missing (§10.1).

### 7.4 What happens in a tick

A summary of spec §7, for reading a world's behaviour. Every tick runs four phases in this order:

| Phase | What happens, in order |
| :-- | :-- |
| 1. **Set** | Senses read the snapshot. Minds update signals. Action genes become intents; reaction genes set their speed. Signal rules run. |
| 2. **Move** | (a) **Upkeep** is paid from the `pay` substance. An entity that cannot pay it is **dying**: it pays nothing, does nothing else this tick, takes part in no transfer (nothing can be given to it or stolen from it), and is removed at the end. (b) **Action fees** are paid, in genome order; an intent whose fee is unaffordable is dropped. (c) All **transfers** (entity `take`, `drop`, `give`, `steal`, and every transfer rule) are settled together. (d) Entities **move**. |
| 3. **Transform** | Entity reactions run, each entity's genes in genome order. Then reaction rules run. Then waste is vented. |
| 4. **Restructure** | Dying and too-old entities are removed. Then entities divide. Newborns act from the next tick. |

Consequences that are easy to miss:

- **Upkeep comes before reactions.** An entity with fuel but no `pay` substance dies in its first tick: its reactions would make energy only after upkeep is due.
- **Everything outside the entity reads the start-of-tick snapshot,** including the cell it is in. An entity that moves and takes in the same tick takes from the cell it started in; from the next tick it takes from the new cell.
- **Transfers cannot fund each other in one tick.** They use the balances after upkeep and fees, so what an entity receives this tick cannot pay for what it gives this tick.
- **Shortfalls are shared pro rata.** If requests on one store exceed its balance, each request gets the same fraction, rounded down; the leftover µ go one at a time to the largest fractional parts. Example: requests of 1u and 0.5u on a cell holding 1u get 667µ and 333µ.
- **Fees are paid for attempts.** A fee is paid in full even if the action then moves less, or nothing, because of sharing. An action with no target (no neighbour in range, or a cell that cannot hold the substance) is dropped and pays no fee.
- **Reactions chain within a tick, in genome order.** A gene can use what an earlier gene made this tick; a gene placed before its supplier waits a tick.
- **Dormant genes cost nothing.** Upkeep counts expressed genes only (§6.6, spec §3.3); a `dormant` gene or module pays no per-gene upkeep until it is expressed.
- **Diffusion offers a share of each cell, not of each neighbour.** `diffuse_conserved(..., rate: 1_000bp)` offers `floor(amount × 1_000 / 10_000)` of the cell's amount, split equally between its four neighbours (N, E, S, W); the remainder stays. From 10u: 1u leaves, 250µ to each neighbour.
- **A full destination is shared too.** If what arrives at a store exceeds its headroom (cap minus balance), each incoming transfer is cut by the same fraction. A transfer with `overflow:` sends its cut part to that sink; one without keeps it at its origin (an entity's `drop` keeps it in the entity).
- **`max_age: N` means an entity acts in exactly N ticks** and is removed at the end of tick N.
- **A removed entity's inventory goes back to its cell** as far as the cell holds that substance and has room; only the rest goes to `remains` (§6.4).
- **Division checks the inventory after reactions,** and a child gets its share of every substance (`split`), rounded down; the remainder stays with the parent.

---

## 8. Lineages and genomes

### 8.1 Lineage

```
lineage Drifter in Dish {
    profile fast {
        genes: 32;
        signals: 16;
        modules: 8;
    }

    mutation {
        rate: 0.02;
        param_sigma: 0.1;
        expression_flip: 0.01;
        rewire: 0.01;
        diverge: 0.005;
        duplicate_gene: 0.005;
        insert_gene: 0.002;
        delete_gene: 0.005;
        duplicate_module: 0.002;
        delete_module: 0.001;
        split_module: 0.001;
        merge_modules: 0.001;
        rules: Tinker;
    }

    seed genome { ... }

    genesis {
        count: 2_000;
        position = uniform(x: 0.0..512.0, y: 0.0..512.0);
        transfer(energy, from: spark, to: self, amount: 5u) @transfer_energy;
    }
}
```

- The `profile` sets the genome's capacity (spec §5.7). The seed MUST fit in it (`E0801`).
- The `mutation` block sets the rate of each operator in spec §8.1. Omitted operators have rate `0.0`.
- Names inside a lineage resolve in the scope of its world, so `spark` means `Dish.spark`. `heading` defaults to a uniform random angle.
- **`extends`.** `lineage Builder extends Drifter in Dish { ... }` starts from a copy of `Drifter` and merges its own parts in, before checking, so the checker and the runner see the full lineage. One level only: the base MUST be in the same world and MUST NOT extend another (`E0117`); an unknown base is `E0114`. `emerge fmt` keeps the `extends` form. The merge:
  - `profile NAME` and `mutation`: entries with the same key replace the base's; other entries are added. A profile the base lacks is added.
  - `seed genome`: signals are added (a repeated name is `E0118`). A module with the base's name gets the new genes; any other module is added at the end (a different `dormant` flag is `E0118`). A gene goes at the end of its module, or after the gene named by `after @label` (an unknown label is `E0114`). A label already used in the base is `E0118`. `after` outside an extending lineage is `E0118`.
  - `genesis`: replaces the base's.

  ```
  lineage Builder extends Drifter in Dish {
      seed genome {
          module metabolism {
              reaction build rate 50mu @build after @respiration;
              action take(s: nitrate, max: 250mu) @eat_nitrate after @eat_carbon;
          }
      }
  }
  ```

### 8.2 Seed genomes and genes

A seed genome is an optional list of signal declarations followed by one or more modules. Every gene is inside a module.

```
seed genome {
    signal hunger keep 0.9;

    module forage {
        sense gradient(q: cells.body) -> food_x, food_y @food_dir;
        regulate weigh(curve: tanh) <- [10.0 * food_y] -> steer @weigh_steer;
        action move(speed: 1.0) @go;
        action turn(angle_deg: 30.0) <- steer @turn_to_food;
    }

    module metabolism {
        reaction photosynthesis rate 1u <- hunger @photosynthesis;
        reaction respiration rate 1u @respiration;
        dormant reaction build rate 250mu @build;
    }
}
```

The four gene forms, with arrows showing what flows in and out:

| Form | Meaning |
| :-- | :-- |
| `sense ITEM(params) -> s1;` or `-> s1, s2;` | write the sense's reading into one or two signals |
| `regulate KIND(params) <- [inputs] -> s;` | compute from the input signals and write signal `s` |
| `reaction NAME rate AMOUNT <- source;` | run a reaction at up to `rate` µ of extent per tick, scaled by `clamp(source, 0, 1)` |
| `action ITEM(params) <- source threshold x;` | fire when `source > x`, sized by `source` (spec §5.5) |

- A **source** is a signal name or a float constant. The default source is `1.0`, and `emerge fmt` omits `<- 1.0`, just as it omits `threshold 0.0`.
- **Inputs** of a regulation gene are a list of `weight * signal` terms. A weight of `1.0` is omitted: `[fullness]` means `[1.0 * fullness]`. Kinds without weights (such as `product`) take plain signal names. The number of inputs and the weight range come from the regulation menu (§6.7); out of range is `E0601`.
- `dormant` starts a gene unexpressed; `dormant module` starts a whole module unexpressed.
- Each parameter MUST lie in the menu's range (`E0601`).
- Every gene has a label (`@name`) in canonical form, which gives it a stable path and a stable ID match across edits. `emerge fmt` adds one if it is missing (§10.1).

### 8.3 Signals

- **Signals are named by the genome.** A name used in a gene is a signal in that genome's table. Names are local to the genome and MUST NOT match a name declared in the world (`E0112`), so a bare name is never ambiguous.
- **Every signal that is read MUST be written** by some gene in the seed (`E0605`). A signal that is written but never read is allowed; it is free (spec §5.1).
- **`signal NAME keep x;`** sets a signal's keep factor (`0.0..1.0`). It is needed only when `x` is not `0.0`.
- **Generated names.** Signals and modules that evolution creates get names of the form `s17` and `m3`. These forms are reserved: a seed MUST NOT declare them (`E0113`).
- Signals hold `f32` values and update as described in spec §5.4.

### 8.4 Modules

```
module reproduce {
    sense inventory(s: body) -> fullness @inventory_fullness;
    regulate weigh(bias: -10.0, curve: step) <- [fullness] -> ready @weigh_ready;
    action divide(split: 0.5) <- ready threshold 0.5 @split;
}
```

- A module's name is unique within its genome.
- Whether a signal is private to a module or part of its interface is **derived** from which genes use it (spec §5.1). In the example, `fullness` and `ready` are private to `reproduce`; if another module read `ready`, it would become part of the interface.
- The order of modules, and of genes within a module, is the genome order (spec §5.2).

### 8.5 Regulation kinds

The world's `regulation` menu (§6.7) lists which kinds a genome may use. The engine defines each kind:

| Kind | Inputs | Parameters | Output |
| :-- | :-- | :-- | :-- |
| `weigh` | 1–4, weighted | `bias`, `curve` | `curve(bias + Σ weight · input)` |
| `product` | 2, unweighted | — | `input₁ · input₂` |
| `delay` [P1] | 1, unweighted | `ticks` | the input `ticks` mind steps ago |
| `add`, `sub`, `mul`, `div`, `min`, `max`, `compare`, `select`, `sin` [P1] | 1–3, unweighted | — | as in spec §5.3 |

Curves: `linear`, `step`, `sigmoid`, `tanh` (spec §5.3).

---

## 9. Rulesets, views, charts, explore

### 9.1 Rulesets

```
ruleset Tinker {
    rule sharpen weight 0.3 : regulate weigh(curve: sigmoid) -> regulate weigh(curve: step);
    rule nudge weight 0.3 : sense $kind(dist: $d) -> sense $kind(dist: jitter($d, sigma: 0.1));
    rule calm weight 0.1 : regulate weigh(bias: $b) where $b > 8.0 -> regulate weigh(bias: $b / 2.0);
}
```

- A **pattern** is a gene form without its wiring: `sense`, `regulate`, `reaction` or `action`, then a name or a capture for the menu item, then the parameters it matches. Parameters and wiring it doesn't mention are left unchanged.
- `$x` captures a parameter's value.
- `where` guards are expressions over captured values, using math built-ins (`abs`, `min`, `max`).
- Replacements may call `jitter(value, sigma: f32)`, which draws from the mutation stream.

### 9.2 Views and charts

```
view Main for Dish {
    layer ground = field(cells.sugar) {
        color: value |> scale(log, range: 1.0..5_000.0) |> palette(greens);
    }
    layer life = agents(where: units(body) > 5.0) {
        glyph: arrow(dir: heading);
        color: species_id |> palette(categorical);
        size: 1.0 + float(expressed_genes) / 4.0;
    }
}

chart Health for Dish {
    line(population, by: species);
    line(metrics.chain_depth);
    line(metrics.flux.body_synthesis);
    ledger(elements);
}
```

- In a `field` layer, `value` is the field's value at the pixel. In an `agents` layer, entity properties (`heading`, `species_id`, `expressed_genes`, inventory amounts by substance name) are in scope.
- `x |> f(args)` means `f(x, args)`: the left value becomes the subject of the call on the right. It exists only in views.
- `emerge check` resolves views and charts (`crates/emerge-lang/src/view.rs`). A field layer shows one field: `field(cells.SUBSTANCE)` for a conserved field, `field(NAME)` for a signal field; `value` is the cell's value, in units for a substance. An agents layer reads `heading`, `position` (directions), `id` and `lineage_id` (identities), `age` and `expressed_genes` (whole numbers), and inventory amounts by substance name (`units(body)` makes one a number). Entries: `color` in both kinds, `glyph` (`arrow(dir: …)` or `dot()`) and `size` in agents layers. `scale(linear | log, range: low..high)` maps a number into `[0, 1]`; a log range must be above 0. Palettes: sequential `greens`, `blues`, `reds`, `grays`, `viridis`, `magma`, `inferno`, `cividis`; diverging `rdbu`; cyclic `phase` (constant lightness, the hue going round; for a heading or a phase); `categorical` (for identities). A chart line plots `population`, `births` or `deaths`, optionally `by: lineage`. `species_id`, `by: species` and `metrics.…` are valid but not computed yet: `W1201`, and a renderer shows them as unavailable.

### 9.3 Explore

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

`vary` takes a path (§10.1), or `seed`, and a list, range or generator of values. Every other entry is a setting.

---

## 10. Paths, IDs and patches

### 10.1 Paths

Every item has a readable path:

| Item | Path |
| :-- | :-- |
| A substance | `Dish.substances.body` |
| A reaction | `Dish.reactions.build` |
| A generated reaction | `Dish.reactions.gen_<hash>` (hash of its canonical formula) |
| A rule | `Dish.rules.rotting` |
| A statement in a rule | `Dish.rules.rotting@react_rot` |
| A setting | `Dish.upkeep.base` |
| A module | `Drifter.seed.forage` |
| A seed gene | `Drifter.seed.forage@turn_to_food` |
| A signal | `Drifter.seed.signals.steer` |

**There are no positional paths.** Statements and genes are addressed only by label, because a position (`action[1]`) changes meaning when an item is inserted before it. A list value, such as `exclude`, is changed as a whole: `set reaction_space.exclude = [light -> energy];`.

**Automatic labels.** In canonical form every gene and every effect statement has a label. `emerge fmt` keeps the labels it finds and adds a derived one where a label is missing:

| Item | Derived label | Example |
| :-- | :-- | :-- |
| Effect statement | the function, then its subject if the subject is a name | `@transfer_carbon`, `@react_rot`, `@diffuse_conserved` |
| Sense or regulation gene | the menu item or kind, then the first signal it writes | `@gradient_food_x`, `@weigh_steer` |
| Reaction gene | the reaction | `@photosynthesis` |
| Action gene | the menu item, then its first parameter if that is a name | `@take_light`, `@move` |

- A label is unique within its rule or genesis block, and, for genes, within its genome. A derived label that is already taken gets `_2`, `_3` and so on, in source order.
- Once written, a label is ordinary source. `emerge fmt` never changes an existing label, even if the item's kind or parameters change later.
- Genes created by evolution have no labels. They are addressed by persistent ID (§10.2).

### 10.2 Persistent IDs

Each item also has a **persistent ID**, a `u64` that never changes and is never reused. IDs are stored in `.emerge/ids.json`, which maps IDs to the paths they had when the file was last written.

When sources are loaded, each item is matched to an existing ID in this order:

1. by label;
2. by named path (`Dish.rules.rotting`);
3. by kind and position within its block, only if the item's kind and menu choice also match. This step exists only for sources edited by hand before `emerge fmt` has labelled them; positions are never written as paths.

Unmatched items get new IDs. IDs of items that no longer exist are retired. A `rename` patch keeps the ID; renaming in a text editor is matched by label if there is one, and is otherwise a removal plus an addition.

The journal, the node editor's `.meta.json` and the API all refer to items by ID.

### 10.3 Patches

```
patch Dish base 41 {
    set upkeep.base = 25mu;
    set rules.sunlight@transfer_light.amount = 3u;
    add after rules.weather: rule wind on cells { diffuse_conserved([carbon], rate: 100bp) @diffuse_conserved; }
    replace rules.smell { self.value = fade(scent.here, keep: 0.8); }
    set reaction_space.exclude = [];
    rename rules.rotting to decompose;
    remove substances.ozone move_to: void;
}
```

- **Paths inside `patch Dish { ... }` are relative to `Dish`:** `upkeep.base` means `Dish.upkeep.base`. The JSON form has no enclosing target, so it always uses full paths. A full path inside a text patch is an error (`E0902`), so each form has one spelling.
- `base` states the revision the patch was written against. The engine resolves every path at that revision, and rejects the patch as stale if the world has moved on (spec §13.2). A patch in a file has no revision, so `base` is optional there.
- A patch is applied as one unit. It is validated, compiled and preflighted in full before commit.
- Removing a substance or field, or lowering a cap below current balances, MUST say where the contents go (`move_to:`), or the patch is rejected (`E0901`).
- **Patches on files (`--patch`).** Every `emerge` command that reads a `.em` file also takes `--patch FILE` (a file of `patch` items) and `--set TARGET.PATH=VALUE` (one `set`, such as `--set Dish.rules.rotting@react_rot.rate=500bp`), repeated and applied in order after the file is read. The patched tree is checked and run in place of the file: a variant is the base plus a small patch, and world and lineage names do not change. `emerge expand FILE --patch P` prints the result; `emerge fmt` does not take patches. Diagnostics on a patched file refer to the patched file as `expand` prints it. A patch names an item as written in the file, so a patch on a lineage does not reach what it takes from a lineage it `extends`. Supported so far:
  - `set` on: a world setting (`max_age`) or one argument of its call (`space.max_interaction_radius`); an entry of a config block (`upkeep.per_gene`); a constant (`consts.NAME`); a menu entry's cost or one parameter (`actions.steal.cost = 200mu`, `actions.steal.max = 0u..1u`, or a list of names); an argument of a labelled rule statement (`rules.sunlight@transfer_light.amount`); a reaction's `catalyze` (`reactions.respiration.catalyze = nitrate(half: 1u, boost: 4.0)`); in a lineage, `mutation.KEY` and `profiles.NAME.KEY`. The item must exist (`E0114`), the value must have the right kind (`E0205`).
  - `add` of a world declaration, appended (`add decompose: rot;`), or placed `after`/`before` a rule, reaction, substance or constant (`rules.weather`) or a setting or block (`remains`); and of a menu entry `after`/`before` another entry of that menu (`add after actions.steal: action guard(...) cost 20mu;`). A name already declared is `E0111`. `emerge fmt` sorts world declarations by kind but keeps the order within a kind, so a position matters for rules and for where a setting lands among the settings.
  - `replace` of a rule's body (`replace rules.smell { ... }`) or of one labelled statement by the statements of the block (`replace rules.weather@transfer_body { ... }`).
  - `remove` of a declaration (`rules.NAME`, `reactions.NAME`, `substances.NAME`, `consts.NAME`, `fields.NAME`, `sources.NAME`, `sinks.NAME`, or a setting by name such as `remains`), of a menu entry (`actions.emit`), or of a labelled statement (`rules.weather@transfer_nitrate`). A substance or field needs `move_to:` (`E0901`); a file holds nothing, so the destination is required and not used. Anything that still names a removed item is reported by the checks that run on the patched file.
  - `rename` of a rule (`rename rules.spread to diffusion;`; a name already declared is `E0111`). Other names are used in genes and reactions, so `rename` of them is `E0904`.
  - Not yet (`E0904`): changes to a lineage other than `set`, and adding genes (use `extends`). A file given to `--patch` that holds anything but `patch` items is `E0903`.
- The API accepts the same operations as JSON: `{"base_revision": 41, "ops": [{"op": "set", "path": "Dish.upkeep.base", "value": "25mu"}]}`.

---

## 11. Grammar

Notation: `'x'` literal token, `A?` optional, `A*` zero or more, `A+` one or more, `A | B` choice, `( )` grouping. `sep(A)` is `(A (',' A)*)?`. Contextual keywords are written as literal tokens; the parser accepts them as `IDENT` everywhere else (§2.1).

```
file        := item*
item        := world | lineage | ruleset | view | chart | explore | patch

// ---- world ----
world       := 'world' IDENT '{' wdecl* '}'
wdecl       := const | element | substance | field | source | sink | waste | reaction
             | menu | rule | genesis | block_cfg | setting
const       := 'const' IDENT '=' expr ';'
element     := 'element' IDENT ';'
substance   := 'substance' IDENT '=' comp 'potential' '-'? INT ';'
comp        := cterm ('+' cterm)*
cterm       := INT? IDENT
field       := 'field' IDENT ':' call ( 'holds' '[' sep(IDENT ':' AMOUNT) ']' | 'signal' ( 'combine' IDENT )? ) ';'
source      := 'source' IDENT ':' '[' sep(IDENT) ']' ';'
sink        := 'sink' IDENT ':' ( 'any' | '[' sep(IDENT) ']' ) ';'
waste       := 'waste' IDENT '->' IDENT ';'
reaction    := 'reaction' IDENT ':' formula ('catalyze' call)? ';'   // catalyze: [P1]
formula     := comp '->' comp
block_cfg   := IDENT '{' entry* '}'            // second_law, reaction_space, upkeep, limits
setting     := IDENT ':' value ';'             // space, remains, max_age
entry       := IDENT ':' value ';'

menu        := ('senses' | 'actions' | 'regulation') '{' mitem* '}'
mitem       := 'sense' IDENT '(' sep(param) ')' ';'
             | 'action' IDENT '(' sep(param) ')' 'cost' AMOUNT ('per' IDENT)? ('requires' expr)? ';'
             | 'gene' IDENT '(' sep(param) ')' 'inputs' RANGE ('weight' RANGE)? ';'
param       := IDENT ':' ( RANGE | 'any' IDENT | '[' sep(IDENT) ']' | IDENT )

// ---- rules and genesis ----
rule        := 'rule' IDENT 'on' IDENT block
genesis     := 'genesis' block
block       := '{' stmt* '}'
stmt        := 'let' IDENT '=' expr ';'
             | 'when' expr block ('else' block)?
             | 'for' IDENT 'in' IDENT block
             | path '=' expr ';'
             | call LABEL? ';'
             | IDENT ':' expr ';'               // lineage genesis: count

// ---- lineages and genomes ----
lineage     := 'lineage' IDENT ('extends' IDENT)? 'in' IDENT '{' ldecl* '}'
ldecl       := 'profile' IDENT '{' entry* '}'
             | 'mutation' '{' entry* '}'
             | seed
             | genesis
seed        := 'seed' 'genome' '{' sigdecl* module+ '}'
sigdecl     := 'signal' IDENT 'keep' FLOAT ';'
module      := 'dormant'? 'module' IDENT '{' gene* '}'
gene        := 'dormant'? gbody LABEL? ('after' LABEL)? ';'   // `after`: only in a lineage that extends another
gbody       := 'sense' call '->' IDENT (',' IDENT)?
             | 'regulate' call '<-' '[' sep(input) ']' '->' IDENT
             | 'reaction' IDENT 'rate' AMOUNT ('<-' source)?
             | 'action' call ('<-' source)? ('threshold' '-'? FLOAT)?
input       := ('-'? FLOAT '*')? IDENT
source      := IDENT | '-'? FLOAT

// ---- rulesets ----
ruleset     := 'ruleset' IDENT '{' rrule* '}'
rrule       := 'rule' IDENT 'weight' FLOAT ':' pattern ('where' expr)? '->' pattern ';'
pattern     := ('sense' | 'regulate' | 'reaction' | 'action') (IDENT | CAPTURE) '(' sep(IDENT ':' pexpr) ')'
pexpr       := CAPTURE | expr

// ---- views, charts, explore ----
view        := 'view' IDENT 'for' IDENT '{' layer* '}'
layer       := 'layer' IDENT '=' call '{' entry* '}'
chart       := 'chart' IDENT 'for' IDENT '{' (call ';')* '}'
explore     := 'explore' IDENT '{' xitem* '}'
xitem       := 'vary' path 'in' expr ';'
             | entry

// ---- patches ----
patch       := 'patch' IDENT ('base' INT)? '{' pop* '}'
pop         := 'set' path '=' value ';'
             | 'add' (('after' | 'before') path ':')? (wdecl | gene | menu_item)
             | 'replace' path block
             | 'remove' path ('move_to' ':' IDENT)? ';'
             | 'rename' path 'to' IDENT ';'

// ---- expressions ----
value       := expr | formula | 'any'
expr        := expr '|>' call
             | expr 'or' expr
             | expr 'and' expr
             | 'not' expr
             | expr ('==' | '!=' | '<' | '<=' | '>' | '>=') expr
             | expr ('+' | '-') expr
             | expr ('*' | '/') expr
             | '-' expr
             | primary
primary     := literal | path | call | list | CAPTURE | '(' expr ')'
list        := '[' sep(litem) ']'
litem       := formula | expr                  // an item containing '->' is a formula
call        := IDENT '(' args? ')'
args        := subject (',' named)* | named (',' named)*
subject     := expr
named       := IDENT ':' value
path        := ('self' | IDENT) ('.' IDENT | LABEL)*
literal     := INT | FLOAT | AMOUNT | RATE | RANGE | 'true' | 'false'

// ---- tokens ----
LABEL       := '@' IDENT
CAPTURE     := '$' IDENT
```

**Precedence**, loosest first: `|>`; `or`; `and`; `not`; comparisons; `+ -`; `* /`; unary `-`. Comparisons do not chain. All binary operators are left-associative. The grammar accepts unary `-` on any expression; the type checker allows it only on `int` and `f32` (§2).

**Subject arguments.** The grammar accepts a positional first argument on any call; the type checker allows it only on built-ins marked as taking a subject in §12, and requires it there. A subject can never be written as a named argument. This keeps one spelling per call.

---

## 12. Built-in functions

Built-ins are grouped by the context that may call them. "Subject" is the one positional argument, where a built-in has one.

**World rules and genesis**

| Built-in | Subject | Named arguments | Result |
| :-- | :-- | :-- | :-- |
| `transfer` | substance | `from`, `to`, `amount`, `overflow` (optional sink) | effect |
| `react` | reaction | `rate` | effect |
| `diffuse_conserved` | list of substances | `rate` | effect |
| `sum`, `min`, `max`, `mean` | collection | — | element type |
| `count` | collection of `bool` | — | `int` |
| `argmin`, `argmax` | collection | — | cell |
| `cap` | substance | — | amount |
| `select` | — | `when`, `then`, `else` | type of `then` |
| `blur` | signal field | `amount` (`f32`) | `f32`: `(1 − amount) · self + amount · mean(von Neumann neighbours)`, reading the start-of-rule state, wrapping as the space does |
| `fade` | `f32` | `keep` (`f32`) | `f32`: `value · keep` |
| `abs`, `sqrt`, `sin`, `tanh` | `f32` | — | `f32` |
| `units` | amount | — | `f32` |
| `amount` | substance | `units` (`f32`) | amount |
| `float` | `int` | — | `f32` |
| `uniform` | — | `x`, `y` (ranges) or `range` | `vec2` or `f32`; genesis only |

**Declarations**

| Built-in | Subject | Named arguments |
| :-- | :-- | :-- |
| `Continuous2D` | — | `w`, `h`, `wrap`, `max_interaction_radius` |
| `Grid` | — | `w`, `h` |
| `balanced` | — | `max_inputs`, `max_outputs`, `max_coeff` |

**Patterns:** `jitter` (subject: value; `sigma`) and the math built-ins.

**Views and charts**

| Built-in | Subject | Named arguments |
| :-- | :-- | :-- |
| `field` | quantity | — |
| `agents` | — | `where` |
| `arrow`, `dot` | — | `dir` (arrow only) |
| `scale` | kind (`linear`, `log`) | `range` |
| `palette` | palette name | — |
| `line` | series | `by` (optional) |
| `ledger` | `elements` or `substances` | — |

**Explore:** `logspace` (`from`, `to`, `steps`), `linspace` (same), `enumerate` (subject: space), `top` (subject: count; `by`), `atlas` (`view`, `every`).

---

## 13. Reference program and fixture

`examples/dish.em` is the reference world of spec §15 and the parser fixture. It MUST satisfy:

1. it parses and type-checks with no diagnostics;
2. parse → `emerge fmt` → parse gives the same AST;
3. `emerge fmt` leaves the file byte-for-byte unchanged;
4. every construct in §6–§9 appears at least once in it or in a sibling fixture under `examples/`.

Any change to the grammar MUST keep these properties, and any example in this document or the spec that is not in a fixture MUST at least parse.

---

## 14. Diagnostics

Every diagnostic is JSON with a stable code, a location, a message and, where possible, a fix that a tool can apply:

```json
{
  "code": "E0402",
  "severity": "error",
  "path": "Dish.reactions.shortcut",
  "id": "0x1f3a9c2e00000017",
  "span": { "file": "dish.em", "line": 31, "col": 5 },
  "message": "reaction `shortcut` is uphill: ΔG = +5 (inputs 0, outputs 5); it must be ≤ −1",
  "fixes": [
    { "title": "couple it to energy", "patch": {
      "base_revision": 41,
      "ops": [ { "op": "replace", "path": "Dish.reactions.shortcut",
                 "value": "energy + carbon + nitrate -> heat + body" } ] } }
  ]
}
```

| Range | Area |
| :-- | :-- |
| `E01xx` | lexical: literals, units, reserved names |
| `E02xx` | types: substance mismatch, conversions |
| `E03xx` | contexts: an effect or read not allowed here, mixed rule kinds |
| `E04xx` | chemistry: balance, dissipation, canonical form, potentials, reaction space size |
| `E05xx` | menus and actions |
| `E06xx` | genomes: ranges, inputs, signals read but never written |
| `E07xx` | routes: payment, overflow, waste, remains, caps |
| `E08xx` | limits and profiles: capacity |
| `E09xx` | patches and hot-swaps: stale revisions, missing destinations, unresolved paths |
| `E10xx`, `W10xx` | the runner: a construct the language allows but the runner cannot run yet |

`emerge check <file>` reports every diagnostic it can find without running the world, in the form `file:line:col: severity[code]: path: message`; `emerge check --json` prints one object per line in the form above (without `id`, and with `fixes` that carry no `base_revision`, because a file has no revision). The position is where the item starts: the declaration, entry, statement or gene. The last line is a verdict: `invalid` (an error was found; exit status 1), `runnable`, or `runnable with N parts left out` (N warnings `W1001`: the file is valid, but the reference runner skips those parts). With `--json` it is `{"verdict":"runnable_with_parts_left_out","left_out":N}`. Codes in use:

| Code | Meaning |
| :-- | :-- |
| `E0001`–`E0007` | syntax: a missing or unexpected token, chained comparisons, a comment at the end of a block |
| `E0101`–`E0107` | literals: amounts that are not whole µ, malformed numbers and units |
| `E0110`–`E0113` | names: keywords, duplicates within a world (`E0111`), signal names that are world names (`E0112`), reserved generated forms (`E0113`) |
| `E0114` | an unknown name: field, source, sink, substance, reaction, lineage |
| `E0115` | an unknown setting or block |
| `E0116` | a required declaration or argument is missing |
| `E0119` | constants that wait on each other in a circle |
| `E0117`, `E0118` | `extends`: a base in another world or that extends too (`E0117`); a repeated signal or label, a `dormant` mismatch, or `after` outside an extending lineage (`E0118`) |
| `E0903`, `E0904` | patch files: an item that is not a patch; an operation not supported on files yet |
| `E0203`–`E0205` | values: out of their type's range (`E0203`: `second_law.min_drop` below 1 or not a whole number; `E0204`: a fraction above `10_000bp`, an amount that does not fit `f32` exactly, §3.5 of the spec), or of the wrong kind (`E0205`) |
| `E0301` | an effect not allowed in this context |
| `E0305` | a value written by several rules without a declared `combine`, or with one that does not apply to it |
| `E0401`–`E0406` | chemistry |
| `E0501`, `E0502` | menus: a sense, action or regulation kind not on the world's menu; an argument that is not one of its parameters |
| `E0601`, `E0602`, `E0605` | genomes: a parameter, input count or weight outside the menu's range; a reaction outside the reaction space's `include` list; a signal read but never written |
| `E0701`–`E0707` | routes: payment route, `base` above `0mu`, a source emitting what it does not declare, a sink that does not accept, `remains` that does not accept everything, a field that cannot hold a substance, an endowment above a cap |
| `E0801`–`E0803` | limits: a seed larger than its profile, a population above `max_entities`, positions outside the space |
| `E1001` | not supported by the runner yet |
| `E1002`, `W1001` | a part of the world the runner cannot run: an error, or with `--partial` a warning naming what was left out |
| `W1201` | a view or chart names a binding that is valid but not computed yet (`species_id`, `by: species`, `metrics.…`); the file stays valid, and the verdict does not count it |
| `N1101`, `N1102` | notes from `emerge check`: substances that can never appear (no store starts with them, no inflow adds them, and every reaction that makes one needs one), and reactions, in the reaction space or in rules, that can never run for that reason. An optimistic closure: amounts, order and costs are ignored, so a note is a proof of impossibility, and no note proves nothing |

---

## 15. What runs where

Each construct is in one of four states: parsed and checked (`emerge check`); run by the CPU runner (`emerge run`, `crates/emerge-ref`); run by the GPU runtime (`emerge-gpu`, `crates/emerge-gpu`, contract mode only); or left out. A part that is **left out** gets `W1001` from `emerge check`. `emerge run` refuses the file unless `--partial` is given, and then runs the world without that part. A gene that needs a left-out part is always an error. `emerge-gpu` always compiles with `--partial`. Checked on 2026-10-20 against the code and with probe files; update this table in the same change as the code.

| Construct | Checked | CPU | GPU |
| :-- | :-- | :-- | :-- |
| **World** | | | |
| `space: Continuous2D(…)` | yes | yes | yes |
| Other spaces | parsed | `E1001` | no |
| No `space` | `E0116` | only worlds built in Rust (the `minds.rs` fixtures) | no |
| One conserved field `Grid(…) holds […]` | yes | yes | yes |
| A second conserved field, other field kinds | parsed | `E1001` | no |
| Signal fields, and rules on them (`fade`, `blur`) | yes | left out | left out |
| Elements, substances, `second_law`, sources, sinks, `waste`, `remains`, `decompose` | yes | yes | yes, at most 32 substances |
| Reactions, `reaction_space` | yes | yes | yes, at most 4 inputs and 4 outputs per reaction |
| Catalysts, `catalyze` (§6.5) | yes | yes, for entity and world reactions | yes |
| `upkeep` (`base`, `per_gene`, `per_signal`, `per_signal_keep`, `per_carried_u`) | yes | yes | yes |
| `limits`, `max_age`, `child_offset`, `mind_steps` | yes | yes | yes; at most 524_288 entities |
| `rounds`, `per_round`, rule `rounds k` and `every N ticks` (§6.6) | not parsed | — | — |
| `rate: evolvable` | parsed | `E1001` | — |
| **World rules and genesis** | | | |
| `transfer` from a source, with `overflow` | yes | yes | yes |
| `react`, `diffuse_conserved` | yes | yes | yes |
| Other rule statements on the conserved field; transfers not from a source | parsed | `E1001` | — |
| World genesis `for cell in … { transfer(…) }`; lineage genesis `count`, `position = uniform(…)`, `transfer` | yes | yes | yes (genesis runs on the CPU before packing) |
| **Genomes and mutation** | | | |
| Senses `inventory`, `gradient`, `probe`, `crowd`, `neighbour`, `clock`, `delta` | yes | yes (`delta` wraps outside contract mode, see `DISCREPANCIES.md`) | yes |
| Other senses (`channel`) | yes | left out | left out |
| Regulation `weigh`, `product` | yes | yes | yes, `weigh` with at most 4 inputs |
| Regulation `delay` and the arithmetic kinds | yes | left out | left out |
| Actions `move`, `turn`, `take`, `drop`, `give`, `steal`, `guard`, `divide` | yes | yes | yes |
| Action `emit` | yes | left out | left out |
| Modules, signals, `keep`, `dormant` | yes | yes | yes, lineages up to 48 genes, 32 signals and 16 modules |
| Mutation operators (spec §8) | yes | yes | yes, child genomes identical to the CPU's |
| Rulesets (rewrite rules) | yes | not applied (left out when mutation is on) | not applied |
| Recombination | not in the language | — | — |
| **Authoring** (resolved before any runner sees the file) | | | |
| `const`, `extends` with `after @label`, patch files, `--patch`, `--set`; `emerge fmt`, `expand`, `order` | yes | yes | yes |
| `view`, `chart` | yes (names and kinds; `W1201` for bindings not computed yet) | noted as "not used by a run"; no renderer yet | — |
| `explore` | parsed only | noted as "not used by a run" | — |
| Hot-swap (a patch at a revision, `world.hotswap` in `emerge-server`) | yes | yes, the Phase 0 subset (`proposals/engine-api.md`: rules, rates and amounts, upkeep costs, limits, `mind_steps`, mutation rates, raised caps); anything else is rejected and changes nothing | yes, the same subset |
| `core.md` syntax (graph core, rules as data, living scopes) | not parsed | — | — |
| **Runner output** | | | |
| `--accounts`, `--blocked`, `--census`, `--contact`, `--changes`, `--inject`, `--clade`, `--features`, `--producers-log` | — | yes | no |
| Population, births, deaths, reaction extent per window; ledger audit (`EMERGE_GPU_AUDIT`) | — | yes (`--every`; conservation tests) | yes |
| Engine API (JSON-RPC over WebSocket, `emerge-server serve`) and MCP (`emerge-server mcp`); run journal and replay | — | yes | yes, except `genome.explain` and `entities.query` |
| Exit test 9's columns: `--flux S`, `--producers F`, `--stop-when-sustained`, `--stop-when-hopeless` | — | yes | yes, same rows as `--contract gpu`; other columns (carriers, thieves, means) CPU only |

The GPU runtime equals the CPU runner in contract mode (`emerge run --contract gpu`) word for word, on one driver at a time (`proposals/gpu-runtime.md`). It is a separate binary, not a mode of `emerge run`.

---

## 16. Open questions

- **Signal units:** signals are plain `f32`. Some worlds may want declared signal units (temperature, concentration) checked like amounts.
- **User functions:** there are none yet. Pure helper functions for world rules (no effects, no recursion) would cut repetition but add a feature to the grammar.
- **Multi-file worlds:** whether a `world` may be split across files (for example chemistry in one file and rules in another) or must stay in one block.
