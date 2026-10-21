# Runner findings for the next spec revision

What building and testing `emerge-ref` shows about the current spec and `core.md`. Each item says what the spec says, what the runner does, and what the spec should say. When an item is settled in the spec, remove it here.

## Open

- `delta(s)` subtracts two `u32` amounts in the default mode, which wraps when the amount fell, so a decrease reads as a huge positive number (clamped to the reading limit). Contract mode (`--contract gpu`) uses the signed difference. Spec v0.7.9 §10.1 records this as a known deviation of the default mode, kept so archived runs reproduce. `archive/plan-test9.md` records the `test9q` experiment (`~/test9q/`) that used this sense; its result may have been affected. Fixing the default mode changes default evolutionary behaviour, so it waits for the user.

Settled in spec v0.7.9 (Appendix J): contract mode (§10.1), the contract-mode catalyst factor and the `10_000bp` cap on a catalysed world rate (§4.5).

## Not yet covered by the runner

The `channel` sense; `delay` and arithmetic regulation kinds; `emit` and field signals (so mutation cannot insert `emit` genes yet); rewrite rulesets; rounds and `every N ticks` (`core.md` §6.5); `rate: evolvable`; recombination; derived edges; `charge` laws in their general form (only upkeep); the birth log and attribution counters.

What each construct does in the checker, the CPU runner and the GPU runtime is in `language.md` §15.
