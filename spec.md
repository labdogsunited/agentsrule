# **Emergent Programming Language & Visual Sandbox**

# **Architecture & Design Specification**

## **1\. Executive Summary & Core Vision**

An IDE and systems programming paradigm built explicitly for emergent complexity. The project unifies the computational exploration of ruliology, the evolutionary optimization of genetic programming, and the data-parallel visual authoring of Houdini VOPs.

Users author environmental topologies, explicit dynamic rules, and agent behaviors via bi-directional node graphs mirrored by a strict textual language. Programs mutate, reproduce, compete, and cooperate within thermodynamically grounded ecosystems—enabling experiments ranging from artificial financial markets to procedural biological worlds.

### **Target Audiences**

* **Technical Artists & Game Designers:** Generating emergent ecosystems, dynamic economies, and complex crowd or NPC behaviors.  
* **Complexity Educators & Science Communicators:** Interactive, visually rich laboratories for demonstrating evolutionary game theory, market dynamics, and morphogenesis.  
* **ALife & Ruliology Researchers:** A unified, GPU-accelerated engine to author, evolve, and share formal rulesets without writing bespoke boilerplate for every simulation.

## **2\. Core Engineering Challenges & Solutions**

### **The "Genetic VOPs" Problem (Safe Graph Mutation)**

Random mutations in strongly-typed node graphs typically break execution or generate invalid operations. This is solved via polymorphic nodes, universal tensor/float wire casting, and explicit `@evolvable` sub-graph boundaries that strictly lock sensory inputs and motor/intent outputs while allowing the hidden internal logic to evolve safely.

### **Performance at Evolutionary Scale**

Unlike standard shaders where every entity runs the exact same program, genetic ecosystems involve thousands of distinct mutated topologies. This is solved by compiling node graphs into a compact linear bytecode executed by a bespoke Virtual Machine inside a single GPU compute shader (Vulkan / WebGPU).

### **Taming "Bloat" in Live Inspection**

Evolved graphs naturally accumulate dead nodes over hundreds of generations. This is solved through automatic algebraic graph pruning (dead-code elimination) and live genealogy heatmaps that highlight active execution paths and color-code nodes by mutation generation.

## **3\. Platform Architecture: The "Language-First" Hybrid Model**

To ensure the programming paradigm remains the centerpiece rather than a lightweight browser demo, the architecture strictly decouples the language compiler from the visual editor:

1. **Canonical Textual Syntax & Intermediate Representation (IR):** A strict 1:1 bi-directional mapping (projectional editing) between the visual node graph and human-readable source code.  
2. **Standalone Native Compiler & CLI ("Headless First"):** Written in a systems language (e.g., Rust \+ `wgpu` or C++) with zero UI dependencies. Supports headless overnight evolutionary sweeps, CLI piping, and C-API integration with DCC tools.  
3. **Dual Deployment (Native \+ WebGPU):** The exact same compiler and GPU Virtual Machine compile to WebAssembly \+ WebGPU for zero-friction browser sharing and genetic "forking" (the Shadertoy model), while wrapping into a native desktop binary (e.g., via Tauri) for uncapped memory and multi-hour background runs.

| Dimension | Native Desktop Target | Web-Native Target (Wasm \+ WebGPU) |
| :---- | :---- | :---- |
| **Compute & Memory** | Uncapped RAM, native Vulkan/Metal/DX12, subgroup ops | Bound by browser tab limits (2–4 GB) and sandbox |
| **Primary Use Case** | Overnight headless evolution, DCC pipeline integration | Zero-friction sharing, live remixing, education |
| **UI Layer** | Shared TypeScript/WebGL Node Editor via Tauri | Shared TypeScript/WebGL Node Editor in browser |

## **4\. The Tri-Layer Execution Paradigm**

Rather than choosing a single computational model, the language layers three paradigms across distinct timescales:

* **Inner Loop (Metabolism) — Synchronous Dataflow:** Per-tick sensory processing and math operations run as a fast, deterministic Directed Acyclic Graph (DAG) on the GPU.  
* **Interaction Layer (Ecology) — Message-Passing & Stigmergy:** Agents interact with each other and the environment by emitting typed intents, signals, and conserved resources.  
* **Meta-Loop (Genetics) — Ruliological Graph Rewriting:** When agents reproduce, pattern-matching rewrite rules operate directly on the agent's Dataflow AST to mutate and evolve its logic.

| Layer | Paradigm | Timescale | Hardware Mapping |
| :---- | :---- | :---- | :---- |
| **1\. Inner Loop** | Synchronous Dataflow (VEX-style) | Every Tick | SIMD/SIMT Compute Shaders |
| **2\. Outer Layer** | Actor / Message-Passing & Fields | Every Tick (Arbitrated) | Spatial Hashes, Atomic Queues, Grids |
| **3\. Meta-Loop** | Pattern-Matching Graph Rewriting | On Reproduction / Division | AST / Bytecode Buffer Mutation |

## **5\. Environment, Topologies, and Conservation Laws**

The environment is treated as a stationary, non-mutating dataflow graph ("World Kernel") executing over a declarative spatial topology.

### **Declarative Spatial Topologies & Occupancy Rules**

When declaring a space, users define both its geometry (Grid2D, Grid3D, Network, OrderBook) and its **Occupancy Mode**:

* **`Exclusive` (Lattice/Grid):** Max 1 agent per discrete cell. Collisions resolved via stochastic arbitration. Ideal for cellular automata, falling-sand physics, and morphogenesis.  
* **`Crowded` (Continuous Spatial):** Floating-point coordinates with soft repulsion and automated spatial hashing (`neighbors(radius)`). Ideal for boids, foraging swarms, and physical ecosystems.  
* **`Multiplexed` (Non-Spatial / Ledger):** Unlimited agents interacting through shared channels or ring buffers. Ideal for artificial financial markets and game-theory tournaments.

### **Thermodynamic & Conservation Primitives**

To prevent mutated graphs from "cheating" and inventing infinite energy or money, the type system enforces a strict distinction between **Signals** (information that can be freely copied) and **Resources** (conserved quantities that can only be moved via zero-sum transfers):

* `sample(field, offset)`: Read-only sensory query of environmental state.  
* `deposit(field, amount)`: Atomic emission of non-conserved signals (e.g., pheromones, market quotes).  
* `transfer(source, target, amount)`: Zero-sum exchange of a `resource` between an agent and the environment, verified by the compiler.

## **6\. The 4-Phase Deterministic Tick Pipeline**

To eliminate order-dependent GPU race conditions, every simulation step executes in four synchronized phases:

1. **Phase 1: Sense & Think (Read-Only Dataflow):** All agents evaluate their bytecode graphs in parallel against read-only snapshots of the world, outputting typed **Intents** (`Move`, `Transfer`, `Deposit`, `Reproduce`).  
2. **Phase 2: Arbitrate & Resolve (Physics & Conservation Pass):** The runtime arbitrates competing intents—splitting contested resources proportionally, resolving spatial collisions, and deducting computational "gas costs" from each agent's metabolic energy.  
3. **Phase 3: World Kernel (Environmental Update):** The passive environment executes its own dataflow graph (diffusing pheromones, regrowing biomass, or clearing matched market orders).  
4. **Phase 4: The Meta-Loop (Life, Death, & Evolution):** Starved agents (`energy <= 0`) despawn and return their biomass to the environment; dividing agents clone their bytecode into free buffer slots and execute `@evolvable` graph-rewrite rules.

## **7\. Example Language Syntax & Structure**

// 1\. Declare the passive environment & occupancy rules

space Savanna : Grid2D(512, 512, occupancy: Crowded(radius: 1.5)) {

    resource biomass : float \= 10.0;   // Conserved: can only be transferred, never copied

    signal   scent   : float \= 0.0;    // Information: freely emitted and diffused

    // Passive World Kernel (runs Phase 3 every tick)

    kernel update() {

        biomass \= clamp(biomass \+ 0.05, 0.0, 20.0);

        scent   \= diffuse(scent, rate: 0.2) \* 0.95;

    }

}

// 2\. Declare an evolvable agent species

agent Forager in Savanna {

    state energy : resource \= 50.0;

    state pos    : vec2;

    // Active Behavior DAG (runs Phase 1 every tick)

    @evolvable(rewrite\_rate: 0.02)

    kernel think() {

        vec2 grad \= gradient(Savanna.biomass, pos);

        float food \= sample(Savanna.biomass, pos);

        // Emit intents to be arbitrated in Phase 2

        intent move(normalize(grad) \* 1.2);

        intent transfer(from: Savanna.biomass, to: energy, amount: food \* 0.5);

        intent deposit(Savanna.scent, 1.0);

        if (energy \> 120.0) {

            intent reproduce(split: 0.5);

        }

    }

}  
