# Runner findings for the next spec revision

What building and testing `emerge-ref` shows about the current spec and `core.md`. Each item says what the spec says, what the runner does, and what the spec should say. When an item is settled in the spec, remove it here.

## Open

None.

Settled: a rule that mixes kinds of effect is `E0303`, and a rule with no effect `E0304`, as spec §7.4 says (2026-10-21, decision 9; the compiler accepted both before). `delta(s)` is the signed difference in the default mode too (2026-10-21, commit `8cb5436`; spec v0.7.9 §10.1). Settled in spec v0.7.9 (Appendix J): contract mode (§10.1), the contract-mode catalyst factor and the `10_000bp` cap on a catalysed world rate (§4.5).

## Not yet covered by the runner

The `channel` sense; `delay` and arithmetic regulation kinds; `emit` and field signals (so mutation cannot insert `emit` genes yet); rewrite rulesets; rounds and `every N ticks` (`core.md` §6.5); `rate: evolvable`; recombination; derived edges; `charge` laws in their general form (only upkeep); the birth log and attribution counters.

What each construct does in the checker, the CPU runner and the GPU runtime is in `language.md` §15.
