# Runner findings for the next spec revision

What building and testing `emerge-ref` shows about the current spec and `core.md`. Each item says what the spec says, what the runner does, and what the spec should say. When an item is settled in the spec, remove it here.

## Open

None. Spec v0.7.8 settled what implementing mutation and contact found (Appendix I): exact mutation operators and draws, the order of the reaction space, the senses the spec only listed, contact targets, and partial runs that leave out menu items.

**Found by the GPU runtime (2026-10-20), not changed in the default mode so that archived runs stay reproducible:**

- `delta(s)` subtracts two `u32` amounts, which wraps when the amount fell, so a decrease reads as a huge positive number (clamped to the reading limit). `archive/plan-test9.md` records the `test9q` experiment (`~/test9q/`) that used this sense; its result may have been affected. Contract mode (`--contract gpu`) uses the signed difference.
- The contract mode (`--contract gpu`, `crates/emerge-ref/src/contract.rs`) is an opt-in set of definitions for float operations that a GPU can reproduce bit for bit (fused multiply-add everywhere, a flush rule below 2⁻¹⁰⁰, polynomial `sin`/`cos`, a heading renormalised without `sqrt`, a normal draw as twelve uniforms, `floor(x + 0.5)` for rounding, the clock phase as an integer fraction, an integer difference in the gradient and `delta`). It changes results slightly; it is off by default.

## Not yet covered by the runner

The `channel` sense; `delay` and arithmetic regulation kinds; `emit` and field signals (so mutation cannot insert `emit` genes yet); rewrite rulesets; rounds and `every N ticks` (`core.md` §6.5); `rate: evolvable`; recombination; derived edges; `charge` laws in their general form (only upkeep); the birth log and attribution counters.

Catalysts (§4.5) run on the CPU, for entity and world reactions (`tests/catalyst.rs`); the GPU runtime refuses a world that has them.
