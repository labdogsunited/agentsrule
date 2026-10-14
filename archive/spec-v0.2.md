# **Emergent Programming Language & Visual Sandbox**

# **Architecture & Design Specification (v0.2 Engineering Revision)**

# **1\. Executive Summary & Core Vision**

An IDE and systems programming paradigm built explicitly for emergent complexity. The project unifies the computational exploration of **ruliology**, the evolutionary optimization of **genetic programming**, and the data-parallel visual authoring of Houdini VOPs.

Users author environmental topologies, explicit dynamic rules, and agent behaviors via bi-directional node graphs mirrored by a strict textual language. Programs mutate, reproduce, compete, and cooperate within thermodynamically grounded ecosystems—enabling experiments ranging from artificial financial markets to procedural biological worlds.

## **Target Audiences**

* **Technical Artists & Game Designers:** Generating emergent ecosystems, dynamic economies, and complex crowd or NPC behaviors (both open-ended and goal-directed via fitness functions).  
* **Complexity Educators & Science Communicators:** Interactive, visually rich laboratories for demonstrating evolutionary game theory, market dynamics, and morphogenesis.  
* **ALife & Ruliology Researchers:** A unified, GPU-accelerated engine to author, evolve, and share formal rulesets without writing bespoke boilerplate for every simulation.

# **2\. Platform Architecture: The "Language-First" Hybrid Model**

To ensure the programming paradigm remains the centerpiece rather than a lightweight browser demo, the architecture strictly decouples the language compiler from the visual editor:

1. **Canonical Textual Syntax & Intermediate Representation (IR):** A strict bi-directional mapping between the visual node graph and human-readable source code. Evolved sub-graphs serialize as clean, tabular **SSA (Static Single Assignment)** register lists in the source file, while visual node X/Y layout coordinates and user comments are stored in a `.meta.json` sidecar file.  
2. **Standalone Native Compiler & CLI ("Headless First"):** Written in Rust \+ `wgpu` with zero UI dependencies. Supports headless overnight evolutionary sweeps, deterministic regression suites, CLI telemetry piping (CSV/Parquet), and C-API integration with DCC tools.  
3. **Dual Deployment (Native \+ WebGPU):** The exact same compiler and GPU execution engine compile to WebAssembly \+ WebGPU for zero-friction browser sharing and genetic "forking", while wrapping into a native desktop binary (via Tauri) for uncapped memory and multi-hour background runs.

| Dimension | Native Desktop Target | Web-Native Target (Wasm \+ WebGPU) |
| :---- | :---- | :---- |
| **Compute & Memory** | Uncapped RAM, native Vulkan/Metal/DX12, subgroup ops | Bound by browser tab limits (2–4 GB) and sandbox |
| **Determinism Scope** | Bit-identical across runs on the same backend | Statistically equivalent to native; bit-identical in software fixed-point mode |
| **Primary Use Case** | Overnight headless evolution, DCC pipeline integration | Zero-friction sharing, live remixing, education |
| **UI Layer** | Shared TypeScript/WebGL Node Editor via Tauri | Shared TypeScript/WebGL Node Editor in browser |

# **3\. Thermodynamic Type System & Conservation Laws**

To prevent mutated graphs from inventing free energy or money, conservation is enforced across both the **compile-time linear type system** and **runtime fixed-point arbitration**.

## **Signals vs. Resources**

* **`signal` (`f32` / `vec2` / `vec3`):** Non-conserved information that can be freely copied, transformed via standard math operators, and emitted via `deposit()`.  
* **`resource` (`u64` Fixed-Point Integer):** Strictly conserved quantities (e.g., 1 unit \= 1,000,000 micro-units).  
  * **Compile-Time Linearity:** Standard arithmetic (`+`, `-`, `*`, `clamp`) and implicit dropping or duplication are compile-time errors on `resource` types. To inspect a resource's magnitude inside math expressions, agents must call `amount_of(res)`, which returns a read-only `f32` signal.  
  * **Zero-Sum Transfers:** Resources can only be moved via `transfer(from, to, requested_amount, cap, overflow)`.  
  * **Explicit Open-System Reservoirs (`source` and `sink`):** Worlds account for external energy inflows and entropy via explicit `source` (e.g., sunlight) and `sink` (e.g., waste heat) reservoirs. Every tick satisfies the invariant: `sum(all_resources) == sum(sources) - sum(sinks)`.  
  * **Mass-Conserving Diffusion:** Calling `diffuse_conserved(resource_field, rate)` compiles to an integer finite-volume flux exchange between adjacent cells, guaranteeing bit-exact mass preservation across the grid.

# **4\. GPU Execution Engine: Fixed-Length CGP & Warp Coherence**

Running thousands of heterogeneous, mutated programs inside a GPU compute shader via a naïve `switch(opcode)` interpreter causes severe warp divergence (10–100x slowdown). Instead, the engine uses **Fixed-Length Cartesian Genetic Programming (CGP) Register Tables**:

* **Uniform Instruction Stride:** Every agent within a species has a fixed-size instruction array of length `N` (e.g., 32 or 64 slots). Each slot is a compact 64-bit struct: `(opcode: u8, dst_reg: u8, src_a: u8, src_b: u8, const_param: f32)`.  
* **Feed-Forward Lockstep Evaluation:** All loops and jumps are forbidden inside `@evolvable` blocks. Conditionals compile to branchless hardware predicates (`select(cond, val_true, val_false)`). Every thread in a warp steps through slots `0..N` in lockstep.  
* **Genome Hash Binning:** During Phase 4 stream compaction, agents are radix-sorted by a 16-bit hash of their active opcode sequence so threads within the same GPU warp share near-identical instruction sequences.  
* **Dormant Introns & Zero-Cost Pruning:** Disconnected slots in the `N`\-slot table act as genetic introns (dormant nodes that can be reactivated by a single operand index mutation). A bitmask of active nodes (computed backward from the output registers on reproduction) allows the GPU to skip evaluating dead slots at runtime and grays them out in the visual inspector.

# **5\. Ruliological Graph Rewriting & Locked Evolvable Boundaries**

## **Locked Functional Boundary (`@evolvable`)**

Mutations cannot delete `intent` emissions or corrupt metabolic accounting. An `@evolvable` block is a **pure function with a locked input/output signature** (`Sensors -> Actuators`). The outer `kernel think()` acts as immutable scaffolding that gathers sensor readings, invokes the evolved function, and passes its outputs into fixed `intent` statements.

## **Programmable Rewrite Rulesets (`ruleset`)**

Users author explicit algebraic and topological rewrite rules that execute on the child's N-slot CGP array during Phase 4 reproduction—entirely on the GPU without dynamic pointer allocation:

* **Point Perturbation:** `Const($c) -> Const($c + philox_normal(0.0, 0.05))`  
* **Topological Expansion:** `Add($a, $b) -> Mul(Add($a, $b), Sin($a))` (allocates an inactive intron slot in the CGP table).  
* **Algebraic Simplification (Anti-Bloat):** `Mul($x, Const(0.0)) -> Const(0.0)`  
* **Homologous Crossover:** Sexual reproduction (`intent mate`) combines fixed-length CGP tables from two parents using a dependency-slice mask.

# **6\. Spatial Topologies & Hybrid Coordinates**

When declaring a `space`, users specify its geometry and occupancy semantics:

* **`Exclusive` (Discrete Lattice):** Max 1 agent per cell (`pos : ivec2`). Collisions resolved via deterministic arbitration. Ideal for cellular automata, falling-sand physics, and morphogenesis.  
* **`Crowded` (Eulerian Grid \+ Lagrangian Agents):** The environment is a discrete `Grid2D` field while agents move in continuous `vec2` coordinates with soft collision radii. Continuous fields are read via `sample_bilinear(field, pos)`, whereas conserved resource transfers target the discrete cell (`cell_at(pos)`).  
* **`Multiplexed` (Non-Spatial Ledger):** Unlimited agents share an abstract matching channel or ring buffer. Ideal for artificial financial order books and game-theory tournaments.

# **7\. The 4-Phase Deterministic Tick Pipeline & PRNG**

All stochastic operations use a stateless, counter-based PRNG (**Philox-4x32-10**) keyed by `(global_seed, tick_index, entity_id, pass_id)`.

1. **Phase 1: Sense & Think (Read-Only Dataflow):** All agents evaluate their CGP tables in parallel against read-only snapshots of the world, outputting typed **Intents** (`move`, `transfer`, `deposit`, `reproduce`).  
2. **Phase 2: Arbitrate & Resolve (Physics & Conservation Pass):**  
   * Competing resource transfers on the same cell are sorted by `(cell_id, agent_id)` and split via deterministic integer division: `grant_i = (available * req_i) / sum(req)`, with integer remainders distributed via Philox tie-breaking.  
   * Instruction execution gas costs (`u64`) are deducted from `agent.energy` and transferred directly to the environment's `heat` sink.  
3. **Phase 3: World Kernel (Environmental Update):** The passive environment executes its dataflow graph (inflow from `source`, mass-conserving diffusion, or market order clearing).  
4. **Phase 4: The Meta-Loop (Life, Death, & Evolution):**  
   * Starved agents (`energy == 0`) despawn, transferring any remaining body mass to the local cell's detritus/biomass field.  
   * Reproducing agents transfer an exact integer fraction of their `energy` to their offspring. Newborn buffer slots are allocated deterministically via a parallel prefix-sum (Blelloch scan), followed by GPU execution of the `@evolvable` rewrite `ruleset`.

# **8\. Observation, Fitness, & Telemetry API**

To support both open-ended ALife and goal-directed technical art:

* **`fitness` Blocks:** Optional objective functions evaluated every `K` ticks to cull low-performing agents or bias reproduction toward a target behavior.  
* **`observe` Blocks:** Declarative telemetry hooks that stream population counts, Gini coefficients, thermodynamic balances, and phylogenetic trees to standard output (Parquet/CSV) or live inspector charts.

# **9\. Reference Language Program (v0.2 Syntax)**

space Savanna : Grid2D(512, 512, occupancy: Crowded(radius: 1.5)) {

    source sunlight : resource(rate: 50\_000); // Explicit inflow per cell/tick

    sink   heat     : resource;               // Explicit metabolic & decay sink

    resource biomass : u64 \= 100\_000;         // Fixed-point conserved resource

    signal   scent   : f32 \= 0.0;             // Non-conserved information field

    kernel update() {

        // Zero-sum inflow from solar source; excess above cap routes to heat sink

        transfer(from: sunlight, to: biomass, amount: 50\_000, cap: 2\_000\_000, overflow: heat);

        diffuse\_conserved(biomass, rate: 0.05);

        scent \= diffuse\_bilinear(scent, rate: 0.2) \* 0.95;

    }

}

ruleset BrainMutator {

    rule tweak(weight \= 0.8)  : Const(\$c) \-\> Const(\$c \+ philox\_normal(0.0, 0.1));

    rule rewire(weight \= 0.2) : Add(\$a, \$b) \-\> Select(Gt(\$a, \$b), \$a, \$b);

}

agent Forager in Savanna {

    state energy : resource \= 500\_000;

    state pos    : vec2;

    // Locked I/O signature, compiled to fixed 32-slot CGP table, mutated by BrainMutator

    @evolvable(slots: 32, rules: BrainMutator)

    fn brain(grad: vec2, food\_level: f32, curr\_energy: f32) \-\> (vec2, f32, f32) {

        vec2 dir \= normalize(grad) \* 1.2;

        f32 eat\_frac \= 0.5;

        f32 repro\_trigger \= select(curr\_energy \> 1\_200\_000.0, 1.0, 0.0);

        return (dir, eat\_frac, repro\_trigger);

    }

    // Immutable scaffolding: guarantees intents cannot be deleted by mutation

    kernel think() {

        vec2 grad \= sample\_gradient(Savanna.biomass, pos);

        f32  food \= amount\_of(Savanna.biomass, cell\_at(pos));

        (vec2 move\_vec, f32 eat\_frac, f32 repro\_flag) \= brain(grad, food, amount\_of(energy));

        intent move(move\_vec);

        intent transfer(from: Savanna.biomass\[cell\_at(pos)\], to: energy, amount: u64(food \* clamp(eat\_frac, 0.0, 1.0)));

        intent deposit(Savanna.scent, cell\_at(pos), 1.0);

        if (repro\_flag \> 0.5) {

            intent reproduce(split: 0.5, min\_energy: 1\_000\_000);

        }

    }

}

# **10\. Phase 0 Engineering Milestone (Proof of Concept)**

To validate the highest-risk technical assumptions before building the visual IDE, Milestone 0 scopes strictly to:

1. **Rust CLI \+ `wgpu` Compute Kernel** (headless, text syntax only).  
2. **Single Topology:** `Grid2D` with `Exclusive` occupancy.  
3. **CGP Benchmark:** 64-slot fixed-length CGP interpreter running 100,000 heterogeneous mutated agents in parallel, benchmarking raw execution vs. genome hash-binning.  
4. **Verification Suite:** Automated tests verifying bit-identical Philox determinism across repeated runs and strict `u64` conservation (`sources - sinks == total_resources`) over 10,000 ticks.

