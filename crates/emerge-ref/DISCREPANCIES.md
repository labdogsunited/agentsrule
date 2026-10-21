# Runner findings for the next spec revision

What building and testing `emerge-ref` shows about the current spec and `core.md`. Each item says what the spec says, what the runner does, and what the spec should say. When an item is settled in the spec, remove it here.

## Open

**A rule that mixes kinds of effect is not rejected.** Spec v0.7.9 §7.4 (line 539) and `language.md` §7 say the compiler rejects it (`E0303`, and `E0304` for a rule with no effect). No crate reports either code: the compiler accepts such a rule, the CPU runner builds a law for each statement and runs it in that statement's step, and `emerge order` lists each statement under its step (2026-10-21, decision 7). No file in the repository mixes kinds. The spec should say either that the compiler rejects it (and the compiler should) or that a mixed rule is allowed and runs statement by statement; see decision 9 in `proposals/decisions-for-review.md`.

Settled: `delta(s)` is the signed difference in the default mode too (2026-10-21, commit `8cb5436`; spec v0.7.9 §10.1). Settled in spec v0.7.9 (Appendix J): contract mode (§10.1), the contract-mode catalyst factor and the `10_000bp` cap on a catalysed world rate (§4.5).

## Not yet covered by the runner

The `channel` sense; `delay` and arithmetic regulation kinds; `emit` and field signals (so mutation cannot insert `emit` genes yet); rewrite rulesets; rounds and `every N ticks` (`core.md` §6.5); `rate: evolvable`; recombination; derived edges; `charge` laws in their general form (only upkeep); the birth log and attribution counters.

What each construct does in the checker, the CPU runner and the GPU runtime is in `language.md` §15.
