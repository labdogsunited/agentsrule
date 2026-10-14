# Emergent Programming Language & Visual Sandbox

**Architecture & Design Specification — v0.3 (Opus revision)**

Status: draft for Phase 0 implementation. Supersedes `spec.md` (v0.1) and `Spec-v0.2.md`.

Conventions: **MUST** / **MUST NOT** mark normative requirements. "Unit" means one whole unit of a conserved quantity; "µ" means one micro-unit (`1 unit = 1_000 µ`).

---

## 1. Vision

An IDE and systems programming language built for emergent complexity. It combines ruliology (exploring simple rule systems), genetic programming (evolving programs), and data-parallel visual authoring in the style of Houdini VOPs.

Users author spaces, world rules and agent behaviours in a strict textual language that maps both ways to a node-graph editor. Agent programs mutate, reproduce and compete inside worlds where conserved quantities are tracked exactly, so evolved programs cannot cheat by inventing energy or money.

### Audiences

- **Technical artists and game designers:** emergent ecosystems, economies, crowd and NPC behaviour, both open-ended and goal-directed.
- **Educators and science communicators:** interactive labs for evolutionary game theory, market dynamics and morphogenesis.
- **ALife and ruliology researchers:** a GPU engine to author, evolve and share rulesets without writing bespoke boilerplate for each simulation.

### Non-goals (for now)

- Bit-identical simulation across different GPUs or vendors.
- Loops, recursion or unbounded computation inside evolved code.
- Continuous (`Crowded`) and non-spatial (`Multiplexed`) spaces before Phase 1.
- The visual editor before the Phase 0 exit criteria (§13) are met. Language-native views (§11) are separate from the editor and start in Phase 0.

---

## 2. Platform architecture

1. **Language first.** A canonical text syntax and IR map 1:1 to the node graph. Node X/Y layout and editor comments live in a `<file>.meta.json` sidecar file, never in the source. Evolved genomes serialize as positional CGP tables (§5.6).
2. **Headless first.** A Rust compiler, runtime and CLI built on `wgpu`, with no UI dependencies. It supports overnight sweeps, deterministic regression tests, CSV/Parquet telemetry, and a C API for DCC tools.
3. **Two deployment targets.** The same compiler and GPU kernels compile to Wasm + WebGPU (sharing and forking in the browser) and to a native desktop binary via Tauri (long runs, uncapped memory).

| | Native | Web (Wasm + WebGPU) |
| :-- | :-- | :-- |
| Compute and memory | Vulkan / Metal / DX12, no fixed memory cap | Browser tab limits (roughly 2–4 GB) |
| Optional GPU features | `subgroups`, `shader-int64` where available | `subgroups` only where the browser exposes it |
| Determinism | Bit-identical across runs on the same GPU, driver and shader compiler | Same as native on the same device; the ledger is exact everywhere |
| Primary use | Headless evolution, DCC pipelines | Sharing, remixing, teaching |

The engine **MUST NOT** depend on 64-bit integer types or float atomics in shaders. All 64-bit quantities use `vec2<u32>` (`lo`, `hi`) with explicit carry propagation.

---

## 3. Conserved quantities and the ledger

### 3.1 Quantities and stores

A world declares one or more **quantities** (for example `energy`, `money`). A **store** is a place that holds an amount of exactly one quantity:

- a **field store** has one `u32` value per grid cell (for example `biomass of energy`);
- an **agent store** has one `u32` value per agent (for example `state energy of energy`);
- a **source** is an unbounded external reservoir that only emits (for example `sunlight`);
- a **sink** is an unbounded external reservoir that only absorbs (for example `heat`).

Every field store and agent store declares a `cap` (µ). The compiler **MUST** reject `cap > 2^31`. This leaves headroom so diffusion and pro-rata arithmetic cannot overflow.

`transfer` is only legal between stores of the **same quantity**. Converting between quantities (for example metabolism turning `energy` into `waste`) is deferred to a future `reaction` construct.

### 3.2 Linear typing

Store values are **linear** at compile time:

- Arithmetic (`+ - * /`), `clamp`, assignment, copying or implicitly dropping a store value is a compile error.
- `read(store)` and `read(field, cell)` return a read-only `f32` signal in whole units (µ / 1000). Reading never changes the store.
- `to_micro(x: f32) -> amount` converts a signal into a **request amount** (a `u32` that is a number, not a resource). Request amounts can only be passed to `transfer`, so creating one has no effect on conservation.

The only operations that change stores are `transfer`, `diffuse_conserved`, and the runtime operations defined in §8 (gas, death, reproduction). Each of these is zero-sum by construction.

### 3.3 Transfers and headroom

`transfer(from, to, amount, overflow?)` moves resource from one store to another:

1. The grant is limited by what the source store holds. When several requests compete, it is limited by arbitration (§8.2).
2. If `overflow` is given, the part of the grant above the receiver's headroom (`cap − current`) goes to the `overflow` store, which is usually a sink.
3. If `overflow` is absent, the request is clamped to the receiver's headroom **before** arbitration. Nothing is ever lost or silently saturated.

All `amount × ratio` arithmetic uses `mul_div_u32(a, b, d)`, which computes `floor(a·b/d)` with a 64-bit intermediate built from 32-bit limbs.

### 3.4 Genesis

Literal initializers on store declarations are compile errors. Initial stock is created in `genesis` blocks, which run once before tick 0, and only by `transfer` from a `source`. Genesis therefore follows the same accounting as every other tick, and there is no separate endowment term.

### 3.5 The ledger invariant

For every quantity `q`, after genesis and at the end of every tick `t`:

```
Σ stores_q(t) + absorbed_q(t) − emitted_q(t) == 0
```

`emitted_q` and `absorbed_q` are the cumulative 64-bit totals of all sources and sinks of quantity `q`. `Σ stores_q(t)` **MUST** be computed with a 64-bit (`vec2<u32>`) tree reduction, because 262,144 cells at up to 2^31 µ each overflow 32 bits. The audit runs every tick in debug builds and every `K` ticks in release builds (`K` is configurable). A failed audit is a hard error that reports the tick and quantity.

### 3.6 Conserved diffusion

`diffuse_conserved(field, rate_bp, boundary)` requires `rate_bp ≤ 10_000`, with `10_000 bp = 100%`. It runs as a **gather**, which needs no atomics:

```
out(v)  = mul_div_u32(v, rate_bp, 40_000)      // sent to EACH cardinal neighbour
new(c)  = v_c − n_c · out(v_c) + Σ_{nbr} out(v_nbr)
```

`n_c` is the number of neighbours cell `c` actually sends to:

- `boundary: wrap` — the grid is a torus, so `n_c = 4` everywhere.
- `boundary: closed` — edge cells don't send across the edge, so `n_c` is 2 or 3 at edges and corners.

The total is preserved exactly. Each new value is a weighted average of a cell and its neighbours (minus rounding), so values cannot exceed the previous maximum.

---

## 4. Signals

A `signal` is non-conserved `f32` information (scalar, `vec2` or `vec3`). Signals can be freely copied and used in any math.

### 4.1 Deposits

`deposit(field, cell, x)` adds to a signal field without float atomics:

1. `x` is clamped to `±deposit_max`, which the field declares (default `1000.0`).
2. It is quantized to `i32(round(x · deposit_scale))` (default scale `1000`).
3. It is added with `atomicAdd` into an `atomic<i32>` staging buffer. Integer addition commutes, so the result is deterministic.
4. At the start of Phase 3, the staged values are converted back and added to the `f32` field, and the staging buffer is cleared.

The compiler **MUST** prove that `max_depositors_per_cell · deposit_max · deposit_scale < 2^31`. In `Exclusive` spaces an agent can only deposit into its own cell, so `max_depositors_per_cell = 1`.

### 4.2 Signal operators

`diffuse(field, rate, boundary)`, `decay(field, factor)`, `sample(field, cell)`, and `gradient(field, cell)`. The gradient uses central differences and returns a `vec2`.

---

## 5. Evolvable programs: positional CGP

### 5.1 Locked boundary

`@evolvable fn` declares a pure function with a fixed signature of **scalar `f32` inputs → scalar `f32` outputs**. Vector values are split into lanes by the surrounding scaffolding. The function body is compiled once into the **seed genome**, and the compiler **MUST** reject a body that needs more than `nodes` nodes.

Only genomes evolve. The calling `kernel think()` is immutable scaffolding: it reads sensors, calls the function, and passes the sanitized outputs into fixed `intent` statements. Mutation therefore cannot delete intents, change accounting, or reach any store.

### 5.2 Genome layout

With `I` inputs, `N` nodes and `O` outputs (constraint: `I + N ≤ 256`):

- Value slots `0 .. I−1` hold the inputs. Node `j` (for `0 ≤ j < N`) writes its result to value slot `I + j`.
- **Feed-forward invariant:** every source index of node `j` is `< I + j`.
- **Output genes:** `out[0 .. O−1]`, each a value-slot index.

```rust
#[repr(C)]
struct CgpNode {        // 8 bytes
    opcode: u8,         // see §5.3
    src_a:  u8,         // value-slot index < I + j
    src_b:  u8,
    src_c:  u8,         // third operand (SELECT, MULADD); ignored otherwise
    param:  f32,        // constant / scale
}

struct Genome {
    nodes:  [CgpNode; N],
    out:    [u8; O],
    active: [u32; 2],   // bitmask of nodes reachable from `out`; derived, never mutated directly
}
```

Operand fields an opcode doesn't use are still stored and still mutated. They carry neutral variation, which becomes active if a later mutation changes the opcode.

### 5.3 Opcodes (protected, branch-free)

| # | Op | Arity | Semantics |
| :-- | :-- | :-- | :-- |
| 0 | CONST | 0 | `param` |
| 1 | ADD | 2 | `a + b` |
| 2 | SUB | 2 | `a − b` |
| 3 | MUL | 2 | `a · b` |
| 4 | DIV | 2 | `abs(b) < 1e-6 ? 1.0 : a / b` |
| 5 | MIN | 2 | `min(a, b)` |
| 6 | MAX | 2 | `max(a, b)` |
| 7 | ABS | 1 | `abs(a)` |
| 8 | NEG | 1 | `−a` |
| 9 | SIN | 1 | `sin(a)` |
| 10 | TANH | 1 | `tanh(a)` |
| 11 | SQRT | 1 | `sqrt(abs(a))` |
| 12 | GT | 2 | `a > b ? 1.0 : 0.0` |
| 13 | SELECT | 3 | `c > 0.5 ? a : b` |
| 14 | MULADD | 3 | `a · b + c` |
| 15 | SCALE | 1 | `a · param` |

After every node, the value is sanitized: NaN and ±Inf become `0.0`, and the result is clamped to `±1e6`. Outputs are clamped again to `±1e4` before they reach the scaffolding.

### 5.4 Active set

At birth, the runtime marks every node reachable backward from `out[]`. This produces the `active` bitmask and `active_count`. Inactive nodes are **introns**: they stay in the genome and are inherited and mutated, but they are never evaluated (subject to §6) and they cost no gas.

The genome is **never compacted or reordered**. Node positions are stable across generations, which keeps neutral drift possible and keeps crossover homologous.

### 5.5 Variation operators

These are applied to a child genome in Phase 4, in this order. Every random draw uses `philox(seed, tick, child_id, PASS_MUTATE, draw_index)`.

**(1) Point mutation (always on).** Each gene of every node, active or inactive, mutates independently with probability `mutation.rate`:

- `opcode`: a uniformly random new opcode.
- `src_*`: a uniformly random index in `[max(0, I + j − levels_back), I + j)`. `levels_back` defaults to unbounded.
- `param`: `param + normal(0, mutation.param_sigma)`. With probability `mutation.param_reset`, it is instead redrawn as `normal(0, 1)`.
- `out[k]`: a uniformly random index in `[0, I + N)`.

**(2) Rewrite rules (optional).** After point mutation, the species' `ruleset` is applied `rewrite.count` times (default 1). Each application:

1. Chooses a rule, weighted by `weight`.
2. Collects the match sites: **active** nodes matching the left-hand side and satisfying its `where` guard. If there are none, the application does nothing.
3. Picks one site uniformly with Philox.
4. Rewrites it:
   - **In-place rule** (the right-hand side needs no new nodes): node `k` is overwritten. Its unused operand fields stay as neutral material.
   - **Expanding rule** (the right-hand side needs `S` new nodes): the runtime looks for `S` *inactive* slots that satisfy the feed-forward invariant for the new subgraph, taking the lowest valid indices. All consumers of `k` (nodes and output genes) are then redirected to the root of the new subgraph. If no valid slots exist, the application does nothing. Nothing is ever shifted or renumbered.
5. Recomputes the active set.

**(3) Crossover (Phase 1).** This happens only when reproduction is sexual (§8.4). Both parents must have the same `I`, `N`, `O` and ruleset. The child takes each node from parent A or parent B by uniform crossover, with a 50% Philox draw per node, and likewise each output gene. The result is always valid, because the feed-forward invariant depends only on node position.

### 5.6 Textual serialization

Evolved genomes serialize as a table that the compiler can load as a seed:

```
genome Forager.brain #id=0x00000000_0001f3a2 gen=812 {
  //  slot  op      a   b   c   param       active
      4:    GT      3   8   -   -           *
      5:    SCALE   0   -   -   1.2         *
      6:    SIN     2   -   -   -
      ...
  out: [5, 9, 12, 4]
}
```

---

## 6. GPU execution

### 6.1 Interpreter

One thread runs one agent. It loads the inputs into a private `array<f32, I+N>`, then loops `j = 0 .. N−1`, running a `switch(opcode)` for each node. That private array is up to 1 KB per thread, so it may spill out of registers. Phase 0 measures this cost.

### 6.2 Warp coherence (Phase 0 benchmark)

| Variant | Agent order | Node evaluation |
| :-- | :-- | :-- |
| A | Birth order | All `N` nodes |
| B | Radix-sorted by 16-bit genome hash | All `N` nodes |
| C | Same as B | Skip node `j` when `subgroupAll(!active[j])` is true across the subgroup |
| D | Same as B | Skip node `j` when this thread's node `j` is inactive (expected to diverge; used as a control) |

- **Genome hash:** a 16-bit hash of the opcode sequence with inactive nodes treated as NOP. It is recomputed at birth.
- **Fallback:** Variant C needs the `subgroups` feature. The runtime falls back to B when it is unavailable.
- **Two orderings per tick:** the agent buffer's physical order is genome-hash order (used in Phase 1). Spatial arbitration uses a separate per-cell CSR index (§8.2), built by counting sort each tick. Agents are never physically re-sorted by cell.

### 6.3 Gas

```
gas = species.base_metabolism + active_count · species.gas_per_node      // µ per tick
```

Gas is charged in Phase 2 by transferring `min(gas, energy)` from the agent's energy store to the species' `heat` sink. Introns are free, while active bloat is paid for every tick.

---

## 7. Spaces

| Mode | Phase | Agent position | Notes |
| :-- | :-- | :-- | :-- |
| `Exclusive` | 0 | `ivec2`, at most 1 agent per cell | The only mode in Phase 0 |
| `Crowded` | 1 | `vec2`; fields remain grids | `cell_at(pos)` for stores, bilinear sampling for signals, a bounded `max_density` for deposit proofs |
| `Multiplexed` | 2 | none | Channels and order books |

### 7.1 Moore actions in `Exclusive` mode

The scaffolding converts continuous brain outputs into one of 9 Moore actions with `moore(x, y)`:

- if `x² + y² < 0.25`, the action is `STAY`;
- otherwise it is the 45° sector containing `atan2(y, x)`.

A move request may only target a cell that was **empty at the start of the tick**. Known consequence: agents can't swap places, and agents inside a dense cluster can't move until a neighbouring cell frees up. This is accepted for Phase 0.

---

## 8. Tick pipeline

Agent IDs are 64-bit (`vec2<u32>`), assigned at birth as `next_id + prefix_offset`, and never reused or changed. All randomness uses Philox-4x32-10:

```
key     = (seed_lo, seed_hi)
counter = (tick, id_lo, id_hi, pass_id << 16 | draw_index)
```

Ties between equal Philox keys are broken by the lower agent ID.

### 8.1 Phase 1 — Sense and think (read-only)

Each agent runs its scaffolding and genome against snapshots of the world taken at the start of the tick, and emits intents: `move`, `transfer`, `deposit`, `reproduce`. Requests to receive resource are clamped to the receiver's headroom at this point (§3.3).

### 8.2 Phase 2 — Arbitrate and resolve

Sub-steps run in this fixed order:

1. **Build the CSR.** A per-cell offset table indexes the agents by cell (counting sort).
2. **Gas.** Each agent pays gas from its energy store to the heat sink (§6.3).
3. **Resource transfers.** These use each agent's **pre-move** cell. For each source store with contending requests `req_i`:
   - If `Σ req ≤ available`, every request is granted in full.
   - Otherwise `grant_i = mul_div_u32(available, req_i, Σ req)`. `Σ req` is summed in 64 bits. The remainder `R = available − Σ grant` (where `R` is less than the number of contenders) is given out 1 µ each to the `R` contenders with the highest `philox(…, PASS_TIEBREAK)`.
4. **Movement** (`Exclusive` mode). Among agents targeting the same empty cell, the agent with the highest `philox(…, PASS_MOVE)` wins. Losers stay where they are.

### 8.3 Phase 3 — World kernel

Folds staged deposits into signal fields (§4.1), then runs each space's `kernel update()`: source inflows, `diffuse_conserved`, signal diffusion and decay.

### 8.4 Phase 4 — Life, death, and variation

1. **Death.** An agent dies if `energy == 0` or `age ≥ max_age`. Any remaining energy is transferred to the cell's field store of the same quantity. Whatever exceeds that store's headroom goes to the overflow sink.
2. **Reproduction eligibility.** An agent is eligible if it emitted `reproduce(split_bp, min_energy)` and has `energy ≥ min_energy`.
3. **Placement** (`Exclusive` mode). Each eligible parent picks an empty Moore cell (using post-movement occupancy) with `philox(…, PASS_PLACE)`. When several parents pick the same cell, the highest `philox(…, PASS_PLACE_WIN)` wins. Losers and parents with no empty neighbour do not reproduce and keep their energy.
4. **Allocation.** Winners get child slots and IDs through a Blelloch prefix sum. Each parent transfers `mul_div_u32(energy, split_bp, 10_000)` to the child.
5. **Variation** (§5.5), then recompute the active set and genome hash.
6. **Compaction.** The agent buffer (not the genomes) is compacted by prefix sum and re-sorted by genome hash for the next tick.

**Sexual reproduction (Phase 1)** uses `reproduce(…, mate: true)`. The parent picks a partner uniformly with Philox from Moore neighbours of the same species that also emitted `mate: true` this tick. Only the initiating parent pays energy, and the partner contributes only its genome. If there is no partner, the agent reproduces asexually.

---

## 9. Fitness and observation

- **`fitness` blocks** (Phase 1) return an `f32` score every `K` ticks. The score only **gates or weights reproduction eligibility**: for example, eligibility requires the score to exceed a percentile, or `min_energy` is scaled by rank. Fitness never deletes agents or moves resources.
- **`observe` blocks** declare telemetry columns streamed to CSV/Parquet every `K` ticks: population, the 64-bit ledger per quantity, active-node histogram, genome-hash diversity, births and deaths, and (Phase 1) lineage edges `(child_id, parent_id, tick)`.

---

## 10. Reference program (Phase 0, `Exclusive`)

```
quantity energy;

space Savanna : Grid2D(512, 512, occupancy: Exclusive, boundary: wrap) {
    source sunlight : energy;
    sink   heat     : energy;

    field biomass : store of energy (cap: 2_000_000);      // µ
    field scent   : signal f32 (deposit_max: 10.0);

    genesis {
        transfer(from: sunlight, to: biomass, amount: 100_000);  // per cell; counted as emitted
    }

    kernel update() {
        transfer(from: sunlight, to: biomass, amount: 50_000, overflow: heat);
        diffuse_conserved(biomass, rate_bp: 500);
        scent = decay(diffuse(scent, rate: 0.2), 0.95);
    }
}

ruleset BrainRules {
    rule wrap_sin (weight = 0.6) : Add($a, $b) -> Mul(Add($a, $b), Sin($a));
    rule gate     (weight = 0.3) : Scale($x, $p) -> Select(Scale($x, $p), Const(0.0), Gt($x, Const(0.0)));
    rule prune    (weight = 0.1) : Mul($x, Const($c)) where abs($c) < 1e-3 -> Const(0.0);
}

agent Forager in Savanna {
    state energy : store of energy (cap: 10_000_000);
    state pos    : ivec2;

    metabolism { base: 200, gas_per_node: 10, sink: Savanna.heat }   // µ per tick
    max_age: 5_000;

    genesis(count: 4096) {
        pos = philox_unique_cells();                           // deterministic shuffle, no collisions
        transfer(from: Savanna.sunlight, to: energy, amount: 500_000);
    }

    @evolvable(nodes: 32, rules: BrainRules,
               mutation: { rate: 0.03, param_sigma: 0.1, param_reset: 0.01 })
    fn brain(gx: f32, gy: f32, food: f32, nrg: f32) -> (f32, f32, f32, f32) {
        return (gx * 1.2, gy * 1.2, 0.5, select(1.0, 0.0, nrg > 1200.0));
    }

    kernel think() {
        ivec2 c    = pos;
        vec2  g    = gradient(Savanna.biomass, c);
        f32   food = read(Savanna.biomass, c);

        (f32 mx, f32 my, f32 eat, f32 rep) = brain(g.x, g.y, food, read(energy));

        intent move(moore(mx, my));
        intent transfer(from: Savanna.biomass[c], to: energy,
                        amount: to_micro(food * clamp(eat, 0.0, 1.0)));
        intent deposit(Savanna.scent, c, 1.0);
        if (rep > 0.5) {
            intent reproduce(split_bp: 5_000, min_energy: 1_000_000);
        }
    }
}
```

Notes:

- The seed genome uses SCALE, CONST, GT and SELECT. All three rules have potential match sites once point mutation introduces ADD and MUL nodes. Point mutation alone is enough to explore the topology, and rules only bias that search.
- `gradient` on a store field reads it as a signal (in units), which is allowed because reading does not change the store.

---

## 11. Visualization (language-native)

Visualization is part of the language, not an editor plugin. A `view` is a declaration in the same source file as the world it shows. It is type-checked by the same compiler and compiled to WGSL render and compute passes, which read the simulation's GPU buffers directly. The CLI, the desktop app, the web build and the node editor all render the same `view` declarations.

### 11.1 Guarantees

- **Read-only.** View code is a separate effect context. `transfer`, `deposit`, `intent`, assignment to state, and Philox draws keyed to simulation passes are compile errors inside a `view` or `chart`. Views use their own random stream (`PASS_VIEW`) for jitter and similar effects.
- **No effect on determinism.** Views are excluded from the state hash. Exit criterion 8 (§13) checks that rendering on or off produces identical simulation state.
- **No copies of simulation data.** Layers bind the simulation's storage buffers as read-only. A frame never copies world state to the CPU.
- **Decoupled rates.** Simulation ticks per rendered frame are configurable (`sim_per_frame`). A slow view never slows a headless run, because headless runs render only when asked to.

### 11.2 What a view can read

- **World state:** store fields (in units), signal fields, agent state, and agent position.
- **Genome metadata:** `active_count`, `genome_hash`, `generation`, `age`, `parent_id`.
- **Last tick's resolution:** a per-tick **event buffer** written by Phases 2 and 4. It holds resolved moves, transfer grants (`from`, `to`, `amount`), births, deaths with their cause, and contested cells. Recording is free when no view or observer uses it, and the compiler only emits the event types that something reads.
- **Telemetry:** time series from `observe` columns, kept in a GPU ring buffer with a CPU-side history.
- **Probes:** per-node genome values for selected agents (§11.5).

### 11.3 Encodings: scales, palettes, legends

Visual channels (`color`, `size`, `opacity`, `rotation`, `height`) take any read-only expression, piped through built-in scales and palettes:

```
color: read(energy) |> scale(log, 1..10_000) |> palette(magma)
size:  1 + active_count / 8
```

- **Scales:** `linear`, `log`, `sqrt`, `symlog`, `quantile` (computed on the GPU each frame), `diverging(mid)`, `categorical`.
- **Domains:** fixed (`1..10_000`), or `auto` with a smoothing time constant so colours don't flicker.
- **Palettes:** perceptually uniform built-ins (`viridis`, `magma`, `cividis`, `greens`, `rdbu`, and categorical sets), each checked for colour-vision deficiency. User palettes are allowed.
- **Legends and axes:** generated automatically from the scale and the quantity's units.

### 11.4 Layer types

| Layer | Shows | Phase |
| :-- | :-- | :-- |
| `field(f)` | A grid field as a heatmap, with nearest or bilinear filtering | P0 |
| `agents(Species)` | One glyph per agent (`point`, `square`, `circle`, `arrow`, `sprite`) with mapped channels | P0 |
| `contour(f, levels)` | Isolines of a field | P1 |
| `vectors(f)` | Gradient or flow arrows, or line-integral-convolution streaks | P1 |
| `trails(Species, length)` | Recent paths of agents, fading over time | P1 |
| `events(kind)` | Births, deaths and contested cells as fading markers | P1 |
| `flows(quantity)` | Transfer grants drawn as animated particles moving along the flow | P1 |
| `links(relation)` | Parent–child, mate or neighbour edges | P1 |
| `spacetime(f, axis)` | A 1D slice stacked over ticks: the classic ruliology spacetime diagram | P1 |
| `custom fn` | A per-cell or per-agent `fn shade(...) -> vec4` written in the signal subset of the language and compiled to a fragment shader | P1 |
| `volume`, `slice`, `isosurface` | `Grid3D` fields | Later |

Layers are composited in declaration order with `blend` (`over`, `add`, `multiply`) and `opacity`. They can be toggled at runtime with `hotkey`.

### 11.5 Inspecting evolution

These views come with the language; the user doesn't author them:

- **Genome inspector.** The selected agent's CGP table drawn as a DAG: active nodes highlighted, introns greyed, nodes coloured by the generation they last mutated in.
- **Live wire values.** For the selected agent, each wire is labelled with its current value. For a whole species, hovering a wire shows a histogram of that value across all agents, computed on the GPU. This is the VOP-style "sparkline on every wire".
- **Diff view.** Parent and child genomes side by side, with mutated genes and rewrite sites marked.
- **Lineage views.** A phylogenetic tree, and a Muller plot of genome-hash or clade frequencies over time.
- **Bloat and diversity.** A histogram of active-node counts over time, and genome-hash entropy.

### 11.6 Charts

`chart` blocks plot `observe` columns and event aggregates: `line`, `area`, `stacked`, `histogram`, `heatmap_over_time`, `scatter` (per-agent, for example energy against active nodes), `muller` and `tree`. Ledger charts show each quantity's stores, emitted and absorbed totals, and the conservation residual, which should be a flat zero line.

### 11.7 Interaction and time

- **Selection.** Click or box-select agents. The selection is exposed to views as `selected`, and the camera can `follow` an agent.
- **Time controls.** Play, pause and single-step. **Rewind** works by restoring the nearest checkpoint (saved every `K` ticks) and re-simulating, which is exact because runs are deterministic on the same device.
- **Comparison.** Two runs side by side (for example two seeds, or mutation on and off), or a `diff(runA.f, runB.f)` field layer.

### 11.8 Headless rendering

```
emerge render world.em --view Main --ticks 0..10000 --every 10 -o frames/   # PNG sequence
emerge render world.em --view Main --chart Population -o run.mp4             # video
emerge run    world.em --snapshot-view Main --every 1000                      # thumbnails during sweeps
```

Headless rendering uses the same WGSL passes with an offscreen target, so overnight sweeps produce images and videos without a window.

### 11.9 Example

```
view Main for Savanna (sim_per_frame: 4) {
    layer ground = field(Savanna.biomass)
        color: read |> scale(linear, 0..2_000) |> palette(greens);

    layer smell = contour(Savanna.scent, levels: 6)
        stroke: white, opacity: 0.4;

    layer foragers = agents(Forager)
        glyph:  arrow(dir: last_move)
        color:  read(energy) |> scale(log, 1..10_000) |> palette(magma)
        size:   1 + active_count / 8;

    layer births = events(birth) glyph: ring, fade: 30 ticks;

    legend auto;
    hotkey "s" toggles smell;
}

chart Evolution {
    line(population(Forager));
    histogram_over_time(Forager.active_count);
    muller(Forager.genome_hash, top: 12);
    ledger(energy);
}
```

---

## 12. Phase 0 scope

1. **Rust CLI + `wgpu`.** Headless, text syntax only, `Exclusive` 2D grid, one quantity, asexual reproduction.
2. **Compiler subset.** Enough to compile §10: `quantity`, `space`, `field`, `source`/`sink`, `genesis`, `kernel update`, `agent`, `@evolvable`, `ruleset`, and the intents. Also a minimal `view`: `field` and `agents` layers, the `linear`/`log` scales and built-in palettes.
3. **Kernels.** Phases 1–4 as specified, including the conserved and signal diffusion, deposit staging, the CSR build, the arbitration paths and the prefix-sum allocation.
4. **Benchmark.** 100,000 agents with 64-node genomes and heterogeneous genomes (seeded by 200 generations of random mutation), measuring Variants A–D (§6.2). Report ticks per second, Phase 1 time, and register or private-memory spill for each variant.
5. **Telemetry.** CSV output from `observe` blocks.
6. **Headless rendering.** `emerge render` to a PNG sequence (§11.8). This is in Phase 0 because it is the main debugging tool for every other item.

## 13. Phase 0 exit criteria

| # | Test | Pass condition |
| :-- | :-- | :-- |
| 1 | Determinism | Identical state hash after 10,000 ticks across 5 runs on the same device |
| 2 | Ledger | Zero drift in the §3.5 invariant at every tick for 10,000 ticks |
| 3 | Contended arbitration | A synthetic test with 1–64 contenders per store and random `req`/`available` values (including `Σ req` > 2^32) matches a CPU reference exactly |
| 4 | Overflow safety | Fuzzed transfers, deposits and diffusion at `cap` and at `2^31` never wrap, verified against a CPU reference |
| 5 | Placement and movement | No cell ever holds 2 agents (checked every tick) |
| 6 | Evolvability smoke test | Over 10,000 ticks, mean harvest per agent per tick rises significantly compared with a control run with mutation off (same seed, 10 seeds) |
| 7 | Performance decision | If B, C or D reaches at least 2× the speed of A, adopt it. Otherwise review the interpreter design (for example per-species uniform programs, or batching by opcode) before Phase 1 |
| 8 | View isolation | State hash after 10,000 ticks is identical with rendering on (every tick) and off |

---

## 14. Open questions

- **Value array spill:** if the private array spills badly at `N = 64`, cap `N` at 32 or move to a register-window scheme.
- **`levels_back` default:** unbounded gives more reach, while small values give more locality and possibly better warp coherence.
- **Multi-quantity reactions:** the syntax and conservation semantics of `reaction` (for example `2 energy → 1 biomass + 1 heat`).
- **Cross-device reproducibility:** whether to offer an optional fixed-point brain mode for shared "forks" that must replay identically on any GPU.
- **Movement freezing:** whether dense clusters need two-phase movement with swap detection in Phase 1.
- **Event buffer size:** with 100k agents, recording every transfer grant each tick could cost tens of MB per tick. Options are sampling, aggregating per cell, or recording only for selected agents.
- **Rewind memory:** checkpoint spacing (`K`) trades memory against re-simulation time. The web build is bounded by tab memory.
- **Editor round-trip:** how an evolved genome should look in the node editor (a raw CGP grid, or a pruned DAG of only active nodes with the introns hidden).
