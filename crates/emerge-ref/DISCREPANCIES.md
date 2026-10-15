# Runner findings for the next spec revision

What building and testing `emerge-ref` shows about the current spec and `core.md`. Each item says what the spec says, what the runner does, and what the spec should say. When an item is settled in the spec, remove it here.

## Open

1. **Reading units (spec §5.5, §10.1).** The spec says amounts are reported to senses "in units as `f32`", but not how they are converted. Division is not portable (WGSL division is accurate only to 2.5 ULP), so the runner computes `amount as f32 × 0.001`. `0.001` is not exact in `f32`, so `10.5u` reads as `10.500001`. The spec should define the conversion as `f32(amount) × 0.001f32`, and say that a gate at an exact amount (such as `bias: -10.0` on `inventory(body)`) may open one µ early or late. Test: `each_regulation_gene_adds_one_mind_step_of_delay`.
2. **Float products with amounts (spec §5.2, §5.5, §3.3).** Action sizes `floor(clamp(o, 0, 1) × max)`, reaction caps `floor(rate × clamp(s, 0, 1))` and the retention cost `ceil(per_signal_keep × keep)` mix a float and an amount. The runner computes them in `f32`, as a GPU would. The spec should say so, and bound the amounts involved by `2^24` µ (about `16_777u`) so the amount converts to `f32` exactly; menu ranges already stay far below that.

## Not yet covered by the runner

Senses other than `inventory` and the amount in the entity's own cell; `delay` and arithmetic regulation kinds; movement and positions; `give`, `steal`, `emit`; mutation; derived edges; `charge` laws in their general form (only upkeep); catalysts; the birth log and attribution counters.
