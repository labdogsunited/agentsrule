# Runner findings for the next spec revision

What building and testing `emerge-ref` shows about the current spec and `core.md`. Each item says what the spec says, what the runner does, and what the spec should say. When an item is settled in the spec, remove it here.

## Open

None. Spec v0.7.8 settled what implementing mutation and contact found (Appendix I): exact mutation operators and draws, the order of the reaction space, the senses the spec only listed, contact targets, and partial runs that leave out menu items.

**Found by the GPU runtime (2026-10-20), not changed in the default mode so that archived runs stay reproducible:**

- `delta(s)` subtracts two `u32` amounts, which wraps when the amount fell, so a decrease reads as a huge positive number (clamped to the reading limit). `archive/plan-test9.md` records the `test9q` experiment (`~/test9q/`) that used this sense; its result may have been affected. Contract mode (`--contract gpu`) uses the signed difference.
- The contract mode (`--contract gpu`, `crates/emerge-ref/src/contract.rs`) is an opt-in set of definitions for float operations that a GPU can reproduce bit for bit (fused multiply-add everywhere, a flush rule below 2⁻¹⁰⁰, polynomial `sin`/`cos`, a heading renormalised without `sqrt`, a normal draw as twelve uniforms, `floor(x + 0.5)` for rounding, the clock phase as an integer fraction, an integer difference in the gradient and `delta`). It changes results slightly; it is off by default.
- Catalysts (spec §4.5) compute their factor in `f64`, with a division, which a GPU need not round the same way. Contract mode uses an integer form instead (`contract::catalyst_scale`: `c / (c + half)` and `boost − 1` each to 2⁻¹⁶), which differs from the `f64` result by at most about `(boost + 1) × 2⁻¹⁶` of the cap. The spec should say it.

**Changed in both modes (2026-10-20):** a catalysed world-reaction rate is capped at `10_000bp`. Above that, the reaction took more than the cell held and an amount underflowed. No archived dish catalyses a world reaction (`archive/catalyst-dish.em` catalyses `respiration`, an entity reaction), so archived runs are unchanged. The spec should say it.

## Not yet covered by the runner

The `channel` sense; `delay` and arithmetic regulation kinds; `emit` and field signals (so mutation cannot insert `emit` genes yet); rewrite rulesets; rounds and `every N ticks` (`core.md` §6.5); `rate: evolvable`; recombination; derived edges; `charge` laws in their general form (only upkeep); the birth log and attribution counters.

What each construct does in the checker, the CPU runner and the GPU runtime is in `language.md` §15.
