# Runner findings for the next spec revision

What building and testing `emerge-ref` shows about the current spec and `core.md`. Each item says what the spec says, what the runner does, and what the spec should say. When an item is settled in the spec, remove it here.

## Open

Spec v0.7.5 settled items 1–11 of the previous list (Appendix F). Found since, by the compiler (`src/compile.rs`):

1. **`weigh` defaults (spec §5.3).** The menu declares `bias` and `curve` but the spec gives no default for a gene that omits them; `dish.em`'s `weigh(curve: tanh)` relies on one for `bias`. The compiler uses `bias: 0.0` and `curve: linear`. The spec should state both.
2. **`split` as a share (spec §7.5).** The child gets `floor(split × amount)`, with `split` an `f32`. The compiler converts `split` once to whole basis points, `round(split × 10_000)`, and the runner computes `floor(amount × bp / 10_000)` in integers, as other fractions of amounts do (§3.5). The spec should say so: then division is exact on CPU and GPU without relying on float products.

## Not yet checked by the compiler

- §3.5: every amount that enters a float product (menu ranges, gene rates, costs) is at most `2^24 µ`.

## Not yet covered by the runner

Senses other than `inventory`, the entity's own cell and `gradient`; `delay` and arithmetic regulation kinds; `give`, `steal`, `emit` and field signals; mutation; derived edges; `charge` laws in their general form (only upkeep); catalysts; the birth log and attribution counters.
