# Review of spec v0.4

The architecture is coherent, but the reference experiment cannot evolve as written, and several runtime contracts need tightening before implementation. Below are proposed design fixes, not implemented or validated solutions. Reviewed v0.4 and the v0.3 definitions it explicitly inherits.

## Blocking problems

### 1. The starting population cannot reach its first mutation (§15)

Seeds receive only energy, have no `take` genes, and cannot acquire the carbon/light required by their reactions. They also lack both `body` and `build`. Division requires body, while mutation happens only at division: discovering `build` is unreachable. With base upkeep alone, 5,000 µ lasts at most 100 ticks, assuming upkeep uses µ.

**Ready fix:** seed functioning uptake, metabolism, and `build`, then measure discovery of a different useful capability. To retain discovery of `build` as the milestone, provide an external body supply and seed body uptake so reproduction works before discovery; measure subsequent synthesis of body separately from uptake. Initial body alone only supports a finite number of divisions.

### 2. Sink typing and death routes contradict the example (§§3–4, 7.4, 15)

`heat_sink` accepts heat, yet weather routes carbon and nitrate into it. Those cannot become heat while conserving C and N. Upkeep transfers energy to that sink without defining energy → heat conversion. `remains: cells` also cannot accept energy or heat because those substances are absent from `cells`.

**Ready fix:** distinguish transfer from balanced conversion. Declare upkeep as energy → heat followed by heat disposal. Add substance-compatible overflow/death sinks for every inventory substance, including full destination cells. Define external sources/sinks as ledger boundaries, excluded from stored inventory totals, to avoid double counting emissions/absorptions.

### 3. Source arbitration does not enforce destination capacity (§§3.1, 7.2, 9.1)

Two independent sources can each see 10 µ of destination headroom and each send 10 µ. Source-only pro-rata allocation permits overfilling. Separately clamping each request, including the inherited v0.3 rule, does not solve this.

**Ready fix:** reserve grants against source availability, then arbitrate those grants against shared destination headroom. Commit only final grants; rejected quantities stay at their sources unless an explicit compatible overflow route succeeds. Define whether caps apply per substance or across a store. Use snapshot balances so incoming stock cannot fund outgoing transfers in the same phase. Accept possible underutilization initially; specify remainder allocation at both stages.

### 4. Failed upkeep does not clearly stop execution (§7)

Death occurs in Life, after actions and reactions. An entity that fails upkeep could still emit free actions, receive resources, or react unless its eligibility is explicitly removed. Action costs also compete for the same balance without a defined order.

**Ready fix:** failed upkeep immediately marks an entity dying and cancels its remaining intents, reactions, and division. Dispose of its inventory in Life. Define a stable action priority and whether failed/partial actions pay an attempt fee or a cost proportional to execution. For P0, a declared attempt fee is simplest.

### 5. Reactions need transactional scheduling (§§4.2, 7.3)

Two genes can compete for the same input; outputs can exceed caps; sequential reactions can consume products created earlier in the same tick. “Limited by the scarcest input” settles none of these interactions.

**Ready fix:** execute genes in slot order, calculating an integer reaction extent limited by inputs, rate, and net output capacity. Commit each reaction atomically. Explicitly permit or prohibit same-tick chaining. Apply similarly explicit ordering to world rules and conservative diffusion. Test contention, full products, and rounding.

## Important design corrections

### 6. Balanced chemistry permits metabolic shortcuts (§4.3)

Because light, energy, and heat all equal E, generation permits light → energy and heat → energy. Carbon + nitrate → body also balances without energy expenditure. Conservation therefore does not ensure the intended metabolic chain or irreversible expenditure of usable energy.

**Ready fix:** for the reference experiment use `generate: none` with a curated reaction menu. Later add world-defined eligibility constraints, catalysts, or energetic restrictions to generated reactions. Whether unrestricted shortcuts are desirable elsewhere is a product choice; they invalidate the present experiment's assumptions.

### 7. Growth and arithmetic need explicit limits (§§3, 5, 7–9)

Per-store caps alone do not bound aggregate requests, cumulative ledger totals, genome growth, generated reaction counts, or population allocation. The inherited 8-byte CGP node uses 8-bit references (`I + N ≤ 256`); an unchanged layout cannot support arbitrary growth in `free`.

**Ready fix:** declare limits for population, intents, genomes, reaction enumeration, and rewrites. Failed child allocation leaves the parent inventory unchanged. Validate aggregate arithmetic bounds and halt before overflow. Bound `free` to the inherited index range initially, or version its node encoding with wider references and a dynamic active mask.

### 8. Hot-swaps need atomicity and precise activation rules (§13)

Upkeep changes “at the next division” leave unclear which costs existing entities pay. Removing a substance needs migration of reactions, genes, views, and menus as well as stock. Migration before pipeline compilation risks leaving a partially changed world after failure.

**Ready fix:** stage and compile a complete candidate, validate every dependency and capacity, then commit migration and configuration together at a tick boundary. On failure retain the old state. Upkeep changes apply globally next tick; mutation settings apply at births. Keep stable reaction IDs/tombstones so removal cannot silently retarget dormant genes. Measure compilation separately from the 200 ms pause target.

### 9. Metrics and exit criteria can mislead (§§8.3, 16)

“Longest reaction chain” is ambiguous with cycles and multi-input reactions. Persistence alone does not demonstrate adaptation. “No entity survives 1,000 ticks without income” cannot hold for arbitrary starting stock and configurable upkeep. Replay is both “not a gate” and an exit criterion.

**Ready fix:** define chain depth on the condensation DAG of the observed substance/reaction graph, with cycles reported separately. Use matched controls before describing persistence as adaptation. Bound starvation tests by starting payable stock and minimum upkeep. Require actual positive `build` flux in the evolution test. Choose explicitly whether replay blocks P0.

### 10. Several small contracts will become incompatible implementations

| Gap | Proposed resolution |
| :-- | :-- |
| Units mix µ, units, percentages, and fractional costs. | Require explicit units; define fixed-point conversion and rounding, retaining fractional charge residuals if needed. |
| Dormant genes are “cheaper” (§3.2) and free (§5.2). | Choose one policy; cap allocated genome storage independently. |
| Unexpressed senses/actions and unused `fast` slots lack lane semantics. | Reserve fixed lanes; inactive senses return zero and actions emit nothing; specify unused-slot initialization and register commit timing. |
| Journal omits a complete execution identity. | Record engine/compiler version, initial configuration, device/backend identity, ordered patches, and relevant modes; preserve v0.3's draw indices and stable entity IDs. |
| Sample claims P0 but imports channel/send from P1 and references `Tinker` without a complete standalone declaration. | Make the reference world a self-contained, phase-valid compiler fixture. |

## Suggested implementation order

First repair the reference world and define units, transactions, death, and chemistry policy. Build a small CPU reference executor to check these contracts, then compare GPU kernels on contention and conservation cases. Benchmark heterogeneous genomes, dense contact, and view streaming early; the scale target needs an exact GPU and workload. Add hot-swaps and richer authoring after those foundations pass.

The fixes above are concrete enough to turn into a v0.5 revision. The main remaining choices are the first evolutionary milestone, allowed chemical shortcuts, and whether deterministic replay is required for P0.
