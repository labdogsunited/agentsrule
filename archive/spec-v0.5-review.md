# Opinion on spec v0.5 and language.md

**My assessment:** this is a substantial improvement over v0.4. The conservation boundaries, two-stage arbitration, explicit dormancy, and CPU reference executor give the implementation a much firmer foundation. Substance potentials are a useful experiment mechanism, and the distinction between forbidden uphill reactions and permitted downhill shortcuts is clear.

I would start a small parser and reference executor now, while resolving the contracts below. I would not yet treat either document as an executable specification. These are design findings and proposed fixes; I have not run a simulation. `language.md` takes precedence for syntax, so its intentional differences from v0.5 are not counted as bugs.

## 1. The grammar rejects the canonical example

**Language §§2, 11, 13.** Calls allow only `IDENT: expr` arguments, but examples use `transfer(light, ...)`, `react(rot, ...)`, `field(cells.sugar)`, and `line(population, ...)`. Lists are values but not expressions, so `diffuse_conserved(substances: [...], ...)` also fails the stated grammar.

Reserved words create another conflict: `self` cannot start a `path`, and `to`, `where`, `senses`, and `brain` cannot appear where the grammar requires `IDENT`. The examples use them in precisely those positions. `explore` values such as `... in [...]` and `Main every 5_000` have no productions either.

**Fix:** choose named arguments consistently, or explicitly permit one positional subject for designated built-ins. Add list expressions, contextual keyword rules, and dedicated exploration productions. Make the full canonical world a parser fixture and require parse → format → parse to preserve its AST. This is the highest-priority language task.

## 2. Action fees still have no conservation route

**Spec §§3.3, 7.2.** Upkeep explicitly converts energy to heat and vents it. Action fees merely get “paid”; their destination or conversion is unspecified. Subtracting them would destroy E. Menus can also name a payment substance different from upkeep's.

**Fix:** in P0 require action costs to use the upkeep payment substance and its conversion/vent route. Charge fees through that same atomic operation. Later support explicitly declared payment conversions. Require positive minimum upkeep if avoiding upkeep is truly forbidden.

## 3. Overflow must distinguish unavailable stock from rejected stock

**Spec §7.4.** Sending “any rejected amount” to overflow is unsafe when rejection happened because the origin lacked stock. If requests total 20 µ but the origin has 10 µ, the missing 10 µ cannot be sent anywhere.

**Fix:** only `stage1_grant − stage2_grant` may overflow. Debit the origin by delivered plus overflowed amounts; ungranted requests have no effect. Define boundary handling separately: sources emit accepted deliveries plus actual overflow, while sinks have no capacity balance. Include a contention test with insufficient origin stock and a full destination.

## 4. Mixed effects inside a world rule need an execution order

**Spec §7.4; language §7.2.** Transfers are batched, but a rule may also contain multiple reactions and diffusion. The reference rule combines rot and diffusion. It is unclear whether diffusion sees pre-rot or post-rot stock, and how reactions competing with transfers reserve their inputs. Statement-order independence cannot describe arbitrary sequences of reactions without another arbitration rule.

**Fix:** for P0, allow either a transfer/diffusion batch, a sequence of local reactions, or signal assignments in one rule. Split reference rot and diffusion into consecutive rules. Each rule reads one snapshot and commits before the next. This keeps the execution contract small and testable.

## 5. Hot-swap validation can become stale

**Spec §13.2.** Compilation runs while the world changes. A migration destination that had capacity during validation may be full at commit. Two pending patches may also validate against the same configuration. The claim that commit cannot fail is therefore unsupported.

**Fix:** stage against a configuration revision. At the tick boundary, reject stale revisions, revalidate current balances and caps, and reserve migration resources before changing live state. Reject cleanly if preflight fails. Lowered inventory caps need migration or rejection rules, just like removed stores.

## 6. Reaction identity and rate normalization are underspecified

**Spec §§4.2–4.3, 7.3.** `light → energy` and `2 light → 2 energy` describe the same net conversion but move different amounts at the same extent cap. Without normalization, scaled formulas can evade an exclusion or obtain different IDs and effective kinetics. Substances appearing on both sides also interact badly with the output-headroom formula: an unchanged substance at capacity can prevent a reaction from running.

**Fix:** define canonical reactions before hashing, excluding, checking potential drop, or assigning rates: aggregate repeated terms, cancel common terms, reduce coefficients by their GCD, and sort by stable substance ID. For P0, disallow implicit catalysts; introduce explicit catalyst requirements later. Define included reactions to take precedence when generation produces the same canonical formula. Stop enumeration at the configured limit instead of requiring an exhaustive count before reporting failure.

## 7. Paths are not stable identities

**Language §10.** `action[4]` and `react[0]` change meaning when earlier items are inserted. Internal IDs survive rename, but the API example addresses paths, and persistence of those IDs across source reloads is unspecified. A delayed patch can therefore target the wrong item.

**Fix:** distinguish readable paths from persistent IDs. Resolve a path against an explicit base revision, journal the resolved ID, and reject stale patches. Persist IDs in a defined project artifact. Specify how text edits preserve or replace IDs, especially for anonymous genes and statements.

## 8. The reference world is plausible, but its claims need calibration

**Spec §15.1.** The reproduction deadlock is repaired. However, motion depends on the x component of a light gradient, which is zero in a uniform field; travel is not assured. Body uptake and diffusion might still permit reproduction without travel, so this is a calibration concern, not proof of failure.

**Fix:** establish a mutation-off baseline with measured survival, births, and resource budgets before testing discovery. If travel is necessary, seed constant forward motion and an explicit heading policy. Add a positive control with body synthesis and nitrate uptake already enabled. This distinguishes an inaccessible mutation path from a world unable to support the target metabolism.

## 9. Close a few smaller semantic gaps

| Gap | Proposed decision |
| :-- | :-- |
| Generic output-to-amount mapping does not define move, turn, or multiple movement/division intents. | Specify scaling per action and how multiple intents combine; allow at most one division per parent per tick. |
| A pure `blur(f32)` cannot identify a spatial neighbourhood. | Make blur an explicit field-sampling intrinsic with field, location, boundary, and kernel semantics. |
| Unassigned registers “read as zero,” despite reads referring to previous state. | Separate reads from next-state outputs; choose hold or reset for unwritten registers. |
| `Select` inherits condition-last semantics from v0.3, but the register example looks condition-first. | Publish the complete opcode signatures here and make the example match. |
| Amount addition, division by zero, and conversion of infinity/out-of-range floats are unspecified. | Define checked arithmetic and deterministic conversion bounds; never rely on backend overflow behavior. |
| CPU/GPU full-state equality is required in deterministic mode, while replay is optional. | Separate exact integer contract checks from float comparisons; specify a shared numeric contract before requiring full-state hashes. |

My suggested next milestone is one canonical world that parses, runs on the CPU, conserves elements through every fee and overflow, reproduces without mutation, and supports a rejected hot-swap without changing state. That will provide better evidence for the larger design than adding more language features now.
