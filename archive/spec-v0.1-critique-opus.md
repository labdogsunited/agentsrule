# Critique of spec.md

Overall it's a strong concept doc. The vision holds together, and some of the design choices are better than what I usually see in ALife proposals. It reads like a pitch rather than a spec, though: the hardest problems are described as solved, and the one genuinely new piece is the least specified part.

## What's good

- **Intents plus arbitration (§6).** Collecting intents read-only, then resolving them in a separate pass, is the right way to avoid GPU race conditions. Most hobby ALife engines get this wrong.
- **The Signal vs Resource type distinction (§5).** I think this is the most original idea in the document. Making conservation a type-level property is what stops evolved programs from inventing free energy.
- **Gas costs taken from metabolic energy.** This gives selection pressure against bloat at no extra cost, so it complements the dead-code pruning in §2.
- **Headless first, editor second.** This is the right order of priorities.

## Real problems

1. **The example breaks the spec's own conservation law.** `biomass = clamp(biomass + 0.05, 0.0, 20.0)` (spec.md:102) creates a `resource` from nothing, and the clamp destroys it. You need explicit sources and sinks (sunlight, a heat sink) with accounting. Otherwise the conservation guarantee is just a convention. Related gaps:
   - Where does deducted gas go?
   - Does `diffuse` on a resource conserve mass?
   - Does `reproduce(split: 0.5)` move energy, or copy it?
2. **"Verified by the compiler" (§5) is the wrong layer.** `transfer` is zero-sum by construction. What can actually go wrong is overdraw when many agents claim the same cell, and only runtime arbitration can catch that. Also, float32 plus atomic adds on the GPU isn't exactly conservative. Resources probably need to be fixed-point integers.
3. **The determinism claim isn't backed up.** "Stochastic arbitration", float atomics and allocating free slots for offspring are all order-dependent unless you specify three things:
   - a counter-based RNG keyed by (seed, tick, agent id), such as Philox;
   - deterministic accumulation;
   - deterministic stream compaction.

   Even then, native and WebGPU builds won't produce bit-identical results (FMA and transcendental precision differ). That matters if "forking" a shared simulation is supposed to reproduce it.
4. **The GPU bytecode VM is the biggest technical risk, and it gets one sentence.** If thousands of distinct programs run in one warp, a switch-dispatch interpreter diverges badly and could be 10–100× slower than you'd expect. Options worth considering:
   - sort or bin agents by genome hash;
   - use a fixed-length genome like Cartesian GP, which is linear, has naturally inactive "dead" nodes, and mutates easily on the GPU;
   - benchmark this before anything else.
5. **Graph rewriting, the core novelty, has no syntax.** "Ruliological pattern-matching rewrite rules" only appear as `rewrite_rate: 0.02`. Some basic questions are unanswered:
   - Who writes the rules, and what does one look like?
   - Do the rules run on the GPU (pointer-heavy and painful) or on the CPU (a round-trip every time an agent reproduces)?
   - Is there crossover, or only mutation?
6. **The evolvable boundary is unclear.** "Lock inputs/outputs" sounds simple, but `think()` contains intent emissions and an `if` (spec.md:136). Can mutation remove the reproduce intent or change its threshold? And how is an `if` inside a "DAG" compiled? Presumably as a select/predicate.
7. **1:1 bidirectional text ↔ graph is harder than it sounds.** Node layout and comments need a home, probably a sidecar file. And an evolved graph's generated text will be unreadable unless you plan for that.
8. **The example mixes up spaces.** `Grid2D` with `Crowded` (continuous) occupancy, while agents have `pos : vec2`. Is the field a grid sampled with interpolation while agents move continuously? The spec should say so.

## Missing

- An observation and metrics API (population counts, lineage export).
- Genome serialization and speciation.
- Seeds and reproducibility.
- Directed fitness. Technical artists often want "evolve toward X", not only open-ended ecology.

## Scope

You have three audiences, two deployment targets, a language, a compiler, a GPU VM, an evolution engine and an IDE. I'd suggest a first milestone of: a Rust CLI, one topology (Grid2D with Exclusive occupancy), fixed-length genomes, text syntax only, and a determinism test suite. Add the visual layer once the VM benchmark looks good.

A smaller point: the file looks like a Google Docs export. It has escaped `\=` and `\*` characters and an unfenced code block.
