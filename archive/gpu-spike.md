# Proposal: the GPU spike

Status: draft, 2026-10-20. Stop rules are fixed here before any measurement. Roadmap step 5 of spec §16; it answers the questions of §16 and §17 before the GPU runtime is built around them. It does not build the runtime and does not pass exit tests 2 or 3.

## Machine

NVIDIA GeForce RTX 3090 (24 GB), driver 570.211.01, Linux. `wgpu` 30 sees it as a Vulkan discrete GPU (also listed: llvmpipe, a CPU fallback, and an OpenGL entry; the spike selects the Vulkan discrete adapter by name and fails if it is missing). WGSL through `wgpu` is the chosen route, because the spec already reasons about WGSL (division accuracy) and the browser view needs the same shaders. CUDA 12.8 runtime and NVRTC are installed and are the fallback if WGSL cannot express something.

## Questions

A. **Portable minds.** Do the division-free `tanh` and `sigmoid` of spec §17 give the same bits on the GPU as on the CPU, and does the arithmetic around them (`bias + Σ w·s`, `keep·s + Σ contributions`)? The second part is the real risk: a multiply followed by an add may be fused into one rounding by the shader compiler, and WGSL has no way to forbid that.
B. **Varied genomes.** What does a loop over gene slots that reads each slot's kind as data cost, compared with entities that all share one shape? And what would binning by shape cost once genomes have drifted?
C. **Dense contact.** (Later, after A and B: spatial hash and neighbour queries at 100k entities.)

Scale (100k entities at 30 ticks/s) is tracked, not a gate (§16).

## A1. The curves alone

Functions (CPU in Rust, GPU in WGSL), each only `+ − ×`, `abs` and `clamp`:

- `tanh_p(x) = u * (2 − |u|)`, `u = clamp(x * 0.5, −1, 1)`
- `sigmoid_p(x) = 0.5 + 0.5 * tanh_p(x / 2)`, with its input as `v = clamp(x * 0.25, −1, 1)`, so `sigmoid_p(x) = 0.5 + 0.5 * (v * (2 − |v|))`
- fallback named in §17, only if the first two fail: cubic `0.5c − c³/54`.

Test: every one of the 2³² `f32` bit patterns, GPU output read back in chunks and compared bit for bit with the CPU result. Rust does not fuse multiply and add, so the CPU result is the unfused one. Also report the cost: GPU time for 2³² evaluations.

Categories reported separately: normal finite inputs; denormal inputs; infinities; NaNs (including payload and sign).

These forms are very likely immune to fusing (`u * (2 − |u|)` has no add after its multiply, and `0.5 * t` is exact), so a pass here proves little about minds; A2 is the test that matters.

**Pass:** zero mismatches on all normal finite inputs and on both infinities, for both functions. **Denormals and NaNs:** if they differ, that is a finding, not a failure. The decision is then to define them in the spec (a minimum magnitude, or a rule that NaN cannot occur because inputs are clamped first) and test the definition once. No other variant is tried. **Fail:** any mismatch on normal finite inputs. Then the cubic fallback is tested once; if it also fails, the curves become integer table lookups (a new design, written up).

## A2. The arithmetic around the curves

Copy the arithmetic and summation order of `emerge-ref/src/mind.rs` (`think`): `x = fold(bias, acc + w·s)` over 1 to 4 inputs, then the curve; and `next = keep·s`, plus the contributions added one by one in gene order (1 to 4 of them), then `sanitize` (clamp, non-finite to 0). Random tuples, about 2²⁸ of them, drawn so that magnitudes spread over several decades (signals within ±10⁴, weights within ±16, as the runner bounds them). Three CPU references: unfused (`a*b` then `+`), fused (`f32::mul_add`), and each GPU variant: plain `a*b + c` in WGSL, and WGSL `fma(a, b, c)`. Compare bit for bit.

**Decision rule (fixed now).** If plain WGSL equals the unfused CPU result on every tuple: record "no contraction on this GPU and driver" and note that this says nothing about other GPUs. If plain WGSL equals the fused result: the contract becomes "every multiply-add in a mind is a fused multiply-add" (CPU uses `mul_add`), and WGSL `fma` must match it; test once. If WGSL `fma` matches fused and plain matches neither (mixed contraction): record that minds cannot be bit-exact across compilers unless every multiply-add is an explicit `fma`; this changes spec §17 and exit test 3's wording, and goes to the user. A1 passing never settles §17 by itself.

Switching `emerge-ref` to these curves or to fused arithmetic is a runner semantic change: it breaks byte-identity with every archived run. It is not part of the spike and needs the user's approval.

## B. Varied genomes

Mind workload that follows the runner (`think`): sense genes run once per tick (their readings come from a cell array indexed by the entity's cell), regulation genes (`weigh` with its four curves and 1 to 4 inputs, `product`) run `mind_steps = 3` times, and each step updates every signal as `keep·s` plus contributions, then sanitizes. Action and reaction genes only read signals once after the last step. Genomes are packed in a storage buffer, 8 words per slot; signals live in a local array.

**Real genomes first.** Before any GPU code: run the CPU runner (guard or test-9 dish, mutation on, about 20_000 ticks, one seed), take the population, and count (a) active genes per entity, (b) distinct *shapes* (the sequence of gene kinds, curves and input counts of the active genes), (c) the share of entities whose shape is shared with at least one other. Only a few of the 52% of births with a parent's exact genome may differ in shape, so the real shape count can be far below one per entity. The spike crate reads this population from `emerge-ref` directly (as a library). Variant 2 uses these genomes, scaled to 100_000 entities by sampling with repetition; random sequences stay as the stated worst case (variant 2b).

Variants measured, at `N = 100_000` entities:

1. **Slot loop, uniform:** every entity has the same slot sequence (best case, no divergence).
2. **Slot loop, real:** the evolved genomes above. **2b, worst case:** every entity has a different random sequence.
3. **Slot loop, drifted, sorted by first slot kinds:** entities ordered by a cheap key (a hash of the first four kinds), to see whether partial sorting recovers coherence.
4. **Binning cost, by measurement of its parts:** the time to create one specialised pipeline for a shape and to dispatch one tiny bin. With the number of distinct shapes among 100k drifted genomes, this gives the cost per tick of binning by shape (bins × dispatch cost). No full binned implementation is built.

Parameters for the synthetic variants: `G` in {16, 32} slots, `K` in {4, 8, 16, 32} template kinds. Ten timed ticks after a warm-up, with GPU timestamps (`Features::TIMESTAMP_QUERY`, requested explicitly) and wall clock around a device poll as a check.

**Decision rule (fixed now).** The slot loop is adopted for the runtime if variant 2b with `G = 32`, `K = 16` takes **at most 10 ms per tick** at `N = 100_000` (a third of a 30 ticks/s budget, leaving the rest for transfers, reactions, contact and view), and variant 2 (real genomes) is no slower. Otherwise the slot loop at this scale fails; then report which of `G`, `K` or the register layout is to blame, and the measured ratios 2/1 and 2b/1, and stop (a different execution design is a new proposal). Binning is adopted only if variant 4's estimate is below variant 2's time **and** the shape count among the real genomes is under 1_000; the measured shape count is part of the record either way.

## Order

1. Crate `crates/emerge-gpu-spike` (a workspace member with `wgpu`, `pollster`, `bytemuck`, and `emerge-ref` for genomes), listed in `members` but not in `default-members`, so `cargo test` elsewhere does not build `wgpu` or need a GPU. Adapter selection and a timing helper. Dispatches stay within wgpu's defaults: chunks of 2²³ items, workgroups of 256 in two dimensions (the limit is 65_535 per dimension), storage bindings under 128 MB; inputs are generated from the invocation ID inside the shader, so only results cross the bus.
2. A1 and A2, run once, results written into this file.
3. The real-genome census, then B, run once per variant and parameter set, results written into this file.
4. Spec §16/§17: record the decisions (curve definition for denormals, slot loop or binning). Archive this file when the runtime proposal replaces it.

## Not part of the spike

The runtime, arbitration, mutation on the GPU, integer exactness of the world (exit test 3), the view, CUDA. Anything the spike finds that needs one of them goes into the next proposal.

## Results

### A1 and A2 (2026-10-20; `emerge-gpu-spike a1`, `a2`; RTX 3090, Vulkan, driver 570.211.01; second implementation: llvmpipe, LLVM 21.1.8)

**A1, curves alone (all 2³² bit patterns, NVIDIA).** `sigmoid_p`: identical on every input, including denormals, infinities and NaN payloads. `tanh_p`: 2²⁴ mismatches on normal inputs and 16_777_212 of 16_777_214 denormal inputs: the GPU flushes denormals to zero, and `x * 0.5` is denormal for every `|x| < 2⁻¹²⁵`. By the literal pass rule ("zero mismatches on normal finite inputs") this is a fail; the cause is only denormals, so the rule's own next step applied: define the curve so that no denormal appears. The defined forms `tanh_d(x) = tanh_p(x if |x| ≥ 2⁻¹⁰⁰ else 0)` and `sigmoid_d` (same wrapper) match the CPU on all 2³² patterns for both functions, NaNs included. The cubic fallback was not needed. (A model of flush-to-zero after every operation still left two mismatches, at `x = ±0x00ffffff`, because the GPU flushes a result that would round up to the smallest normal; the explicit threshold avoids that case.) Cost: 2³² evaluations of both functions in 0.3–0.5 s of dispatch time.

**A2, the arithmetic around the curves (2²⁸ random tuples per mode).** The finding the decision rule was written for:

- **NVIDIA (Vulkan): the compiler fuses.** Plain WGSL `a*b + c` is bit-identical to the fused CPU result (`f32::mul_add` for every multiply-add of `think`) on all 2²⁸ tuples, and differs from the unfused result on 25% of `x`, 8% of `y` and 20% of the final signals. WGSL `fma()` gives the same bits as the plain form.
- **Denormals:** with values near the denormal edge (mode 1), 146 of 2²⁸ final signals differ from the fused CPU result. The cause is a denormal intermediate that the GPU flushes. With the rule "every multiply-add result below 2⁻¹⁰⁰ in magnitude is 0" applied on both sides (mode 2), mismatches are 0 of 2²⁸ for all three values. Tested once, as the rule said.
- **llvmpipe (the second implementation): the opposite.** Plain `a*b + c` equals the *unfused* result and WGSL `fma()` also equals the unfused result, in every mode: the arithmetic is not fused at all, and `fma()` is not fused either. Against the fused CPU result it mismatches about 12% of `x` in the tiny mode.

So the decision rule's third branch applies, in a stronger form: **there is no single arithmetic contract for minds that two WGSL implementations both follow.** The NVIDIA driver always fuses; llvmpipe never fuses, not even `fma()` (WGSL allows either: the accuracy of `fma` is that of `e1 * e2 + e3`). Bit-exact minds across different GPUs and drivers therefore need something other than float multiply-add in WGSL. Candidates, not decided here and not part of the spike: (a) integer or fixed-point minds (exact everywhere, the way the world already is; a design change to `weigh`, `product` and the signal update); (b) a backend that can ask for no contraction or explicit fused operations (SPIR-V `NoContraction`, CUDA PTX `mul.rn`/`fma.rn`); (c) bit-exactness only per driver, with the CPU reference using `mul_add` to match NVIDIA, and across drivers only the 5% statistical agreement of exit test 3. Exit test 3's wording ("matches every integer") already concerns the world's integers, which are unaffected: minds only produce float intents. The open question is what must be identical for the world to stay deterministic across machines, and that is for the user.

Nothing was changed in `emerge-ref`: its curves still use `exp` and `tanh`, and its arithmetic is unfused. Switching it to the defined forms, or to fused arithmetic, would break byte-identity with the archived runs and needs approval.

### B (2026-10-20; `emerge-gpu-spike census`, `b`, `bin`; RTX 3090)

**Real genomes.** One seed of the test-9 dish (`archive/test9-dish.em`, `Drifter`, mutation on, seed 1), 20_000 ticks, 10_194 entities. Active genes per entity: mean 19.5, max 26, but only 5.4 of them are mind genes (sense, `weigh`, `product`); the rest are actions and reactions, which read one signal. Signals per genome: at most 13. Distinct shapes: **235 mind shapes** (99.3% of entities share their shape with another; the 10 most common cover 68.6%), 1_730 full shapes (kind sequence of all active genes; 91.0% shared). So the shape count is far below one per entity, as the 52%-identical-births figure suggested. One seed, one snapshot, one dish: the counts say nothing yet about how fast new shapes appear.

**Slot loop, 100_000 entities, 3 mind steps, GPU time per tick (timestamps; wall clock agrees within 0.02 ms):**

| Variant | Slots | Kinds | ms |
| :-- | :-- | :-- | :-- |
| 2 real genomes (sampled with repetition) | 19.5 mean | 7 | **1.8** |
| 2 real genomes, sorted by first four kinds | 19.5 | 7 | 1.8 |
| 1 uniform (first real genome) | 22 | 7 | 1.9 |
| 1 uniform, synthetic | 16 / 32 | 8–32 | 2.0–2.7 / 3.6–4.8 |
| 2b random sequences | 16 | 8 / 16 / 32 | 3.4 / 3.6 / 4.1 |
| 2b random sequences | 32 | 8 / 16 / 32 | 6.9 / **6.4–6.7** / 7.3 |
| 3 random, sorted by first four kinds | 32 | 16 | 6.3–6.9 |

The decision rule asked for variant 2b at `G = 32`, `K = 16` at most 10 ms, and variant 2 no slower: **6.4–6.7 ms and 1.8 ms. The slot loop is adopted.** Divergence costs about 1.7× (2b against 1 at equal size) and grows slowly with the number of kinds (6.9 to 7.3 ms from 8 to 32 kinds); sorting by a short key recovers nothing. Real minds are small enough that divergence does not show (variant 2 equals variant 1). Signals live in local arrays (five of 32 floats per thread); that was the straightforward layout and was fast enough, so no register work was needed.

**Binning (variant 4, by parts).** Creating one pipeline for a 12-gene unrolled shape: 52–85 ms, mean 64 ms, cold (a second run is faster because the driver caches shaders). A one-workgroup dispatch costs 7–8 µs on the GPU, including the barrier between dispatches. With the 235 real mind shapes: 235 × 7.2 µs = **1.7 ms of dispatch overhead alone**, plus the work in each bin (about the 1.8 ms of the slot loop at best), so binning is not below the slot loop; compiling every shape once would take about 15 s cold, and each new shape costs 64 ms on the CPU. With the 1_730 full shapes the dispatch overhead alone is 12.5 ms. **Binning is not adopted** (the rule needed its estimate below variant 2's time).

**Not measured, so not claimed:** transfers, reactions, contact and view on the GPU; mutation on the GPU; how the shape count grows over time or across seeds and dishes; larger genomes than the 26 active genes seen here (the synthetic `G = 32` rows bound that); whether other GPUs behave alike.

### Decisions to record in the spec

1. Curves: use `tanh_d` and `sigmoid_d` (A1). Arithmetic contract (A2), decided 2026-10-20: exact per driver, CPU reference with `mul_add` and a `2^-100` flush on multiply-add results; integer minds and a no-contraction backend stay open.
2. Execution: a slot loop over gene slots that reads each slot's kind as data (B).
3. Scale: the mind step is about 2–7 ms of a 33 ms tick at 100_000 entities.
